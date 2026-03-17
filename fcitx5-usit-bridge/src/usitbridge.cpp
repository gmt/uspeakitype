// fcitx5-usit-bridge: D-Bus bridge for external text injection
// Exposes CommitString method that usit can call to inject text
// through fcitx5's input method protocol.

#include <fcitx/addonfactory.h>
#include <fcitx/addoninstance.h>
#include <fcitx/addonmanager.h>
#include <fcitx/instance.h>
#include <fcitx/inputcontext.h>
#include <fcitx-utils/dbus/bus.h>
#include <fcitx-utils/log.h>
#include "fcitx-module/dbus/dbus_public.h"

namespace fcitx {

FCITX_DEFINE_LOG_CATEGORY(usitbridge, "usitbridge");
#define USITBRIDGE_DEBUG() FCITX_LOGC(usitbridge, Debug)
#define USITBRIDGE_INFO() FCITX_LOGC(usitbridge, Info)
#define USITBRIDGE_ERROR() FCITX_LOGC(usitbridge, Error)

constexpr char USIT_BRIDGE_PATH[] = "/rocks/gmt/usit/FcitxBridge1";
constexpr char USIT_BRIDGE_INTERFACE[] = "rocks.gmt.UsitFcitxBridge1";

class UsitBridge;

class UsitBridgeController : public dbus::ObjectVTable<UsitBridgeController> {
public:
    UsitBridgeController(UsitBridge *bridge) : bridge_(bridge) {}

    void commitString(const std::string &text);
    bool isActive();

private:
    UsitBridge *bridge_;

    FCITX_OBJECT_VTABLE_METHOD(commitString, "CommitString", "s", "");
    FCITX_OBJECT_VTABLE_METHOD(isActive, "IsActive", "", "b");
};

class UsitBridge : public AddonInstance {
public:
    UsitBridge(Instance *instance)
        : instance_(instance),
          dbus_(instance_->addonManager().addon("dbus", true)),
          controller_(this) {

        if (!dbus_) {
            USITBRIDGE_ERROR() << "D-Bus addon not available";
            return;
        }

        bus_ = dbus_->call<IDBusModule::bus>();
        if (!bus_) {
            USITBRIDGE_ERROR() << "Failed to get D-Bus bus";
            return;
        }

        if (!bus_->addObjectVTable(USIT_BRIDGE_PATH, USIT_BRIDGE_INTERFACE, controller_)) {
            USITBRIDGE_ERROR() << "Failed to register D-Bus object";
            return;
        }

        USITBRIDGE_INFO() << "Usit Bridge addon loaded, D-Bus interface ready at "
                         << USIT_BRIDGE_PATH;
    }

    Instance *instance() { return instance_; }

private:
    Instance *instance_;
    AddonInstance *dbus_ = nullptr;
    dbus::Bus *bus_ = nullptr;
    UsitBridgeController controller_;
};

void UsitBridgeController::commitString(const std::string &text) {
    if (text.empty()) {
        USITBRIDGE_DEBUG() << "Empty string, ignoring";
        return;
    }

    auto *ic = bridge_->instance()->mostRecentInputContext();
    if (!ic) {
        USITBRIDGE_ERROR() << "No input context available";
        return;
    }

    USITBRIDGE_INFO() << "Committing string: " << text.size() << " bytes";
    ic->commitString(text);
}

bool UsitBridgeController::isActive() {
    auto *ic = bridge_->instance()->mostRecentInputContext();
    return ic != nullptr;
}

class UsitBridgeFactory : public AddonFactory {
public:
    AddonInstance *create(AddonManager *manager) override {
        return new UsitBridge(manager->instance());
    }
};

} // namespace fcitx

FCITX_ADDON_FACTORY(fcitx::UsitBridgeFactory);
