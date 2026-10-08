#pragma once
#include "bridge.h"
#include <QAction>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusServiceWatcher>
#include <QKeySequence>
#include <QTimer>
#include <KGlobalAccel>
#include <KGlobalShortcutInfo>

// This object exists only in --hotkey-only mode. Every presentation surface
// shares this independent listener through the engine, including terminal mode.
class GlobalHotkey : public QObject {
    Q_OBJECT
public:
    explicit GlobalHotkey(Bridge &bridge, bool test = false)
        : bridge(bridge), testing(test), action(this) {
        action.setObjectName("hold_to_dictate");
        action.setText("Hold to dictate");
        action.setProperty("componentName", "usit");
        action.setProperty("componentDisplayName", "usit");
        connect(&bridge, &Bridge::stateChanged, this, [this] { configure(this->bridge.state()); });
        connect(&bridge, &Bridge::disconnected, this, &GlobalHotkey::shutdown);
        connect(qApp, &QCoreApplication::aboutToQuit, this, &GlobalHotkey::shutdown);
        if (testing) return;
        auto *accel = KGlobalAccel::self();
        connect(accel, &KGlobalAccel::globalShortcutActiveChanged, this,
                [this](QAction *source, bool active) { if (source == &action) setHeld(active); });
        connect(accel, &KGlobalAccel::globalShortcutChanged, this,
                [this](QAction *source, const QKeySequence &key) {
            if (source != &action || changing || !registered) return;
            if (key != desiredKey) {
                setHeld(false);
                registered = false;
                report(false, "The KDE shortcut changed. Disable and re-enable hold-to-talk, or choose another hold key.",
                       key.toString(QKeySequence::PortableText));
            }
        });
        watcher = new QDBusServiceWatcher("org.kde.kglobalaccel", QDBusConnection::sessionBus(),
                                         QDBusServiceWatcher::WatchForOwnerChange, this);
        connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
                [this](const QString &, const QString &, const QString &owner) {
            if (owner.isEmpty()) serviceLost();
            else if (wanted) configure(this->bridge.state(), true);
        });
        // Also cover a complete session-bus disconnect, which cannot deliver an
        // owner-change notification from the vanished bus.
        connect(&health, &QTimer::timeout, this, [this] {
            if (registered && !serviceAvailable()) serviceLost();
        });
        health.start(2000);
    }
    ~GlobalHotkey() override { shutdown(); }

    void shutdown() {
        setHeld(false);
        clearBinding();
        wanted = false;
    }
    void selfTest() {
        configure({{"push_to_talk", true}, {"ptt_shortcut", "F9"}}, true);
        setHeld(true);
        setHeld(true); // auto-repeat must not create duplicate presses
        setHeld(false);
        setHeld(true);
        configure({{"push_to_talk", false}, {"ptt_shortcut", "F9"}});
        configure({{"push_to_talk", true}, {"ptt_shortcut", "F9"}});
        setHeld(true);
        serviceLost();
    }
    void holdForDisconnectTest() {
        configure({{"push_to_talk", true}, {"ptt_shortcut", "F9"}}, true);
        setHeld(true);
    }
private:
    bool serviceAvailable() const {
        const auto bus = QDBusConnection::sessionBus();
        return bus.isConnected() && bus.interface()
            && bus.interface()->isServiceRegistered("org.kde.kglobalaccel").value();
    }
    void report(bool available, const QString &error = {}, const QString &actual = {}) {
        bridge.sendObject({{"type", "hotkey_status"}, {"available", available},
                           {"shortcut", actual.isEmpty() ? requested : actual},
                           {"error", error.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(error)}});
    }
    void setHeld(bool active) {
        if (active && (!registered || !wanted)) return;
        if (held == active) return;
        held = active;
        bridge.send(active ? "ptt_press" : "ptt_release");
    }
    void clearBinding() {
        setHeld(false);
        changing = true;
        if (bound && !testing)
            KGlobalAccel::self()->removeAllShortcuts(&action);
        bound = false;
        if (ownsService) {
            QDBusConnection::sessionBus().unregisterService("org.uspeakitype.PushToTalk");
            ownsService = false;
        }
        registered = false;
        changing = false;
    }
    void serviceLost() {
        setHeld(false);
        registered = false;
        // Keep bound until reconfiguration/destruction so KGlobalAccel can
        // remove its local registration as well if the daemon returns.
        if (wanted) report(false, "KDE's global shortcut service is unavailable; hold-to-talk is disabled.");
    }
    void configure(const QVariantMap &state, bool force = false) {
        const bool enabled = state.value("push_to_talk").toBool();
        const QString key = state.value("ptt_shortcut").toString().trimmed();
        if (!force && configured && enabled == wanted && key == requested) return;
        configured = true;
        clearBinding();
        wanted = enabled;
        requested = key;
        if (!wanted) { report(false); return; }
        desiredKey = QKeySequence::fromString(requested, QKeySequence::PortableText);
        if (desiredKey.isEmpty() || desiredKey.count() != 1 || desiredKey[0].key() == Qt::Key_unknown) {
            report(false, "Choose a single hold key or key combination, such as F9 or Ctrl+Alt+Space.");
            return;
        }
        if (testing) { registered = true; report(true); return; }
        if (!serviceAvailable()) {
            report(false, "KDE's global shortcut service is unavailable; hold-to-talk is disabled.");
            return;
        }
        if (!QDBusConnection::sessionBus().registerService("org.uspeakitype.PushToTalk")) {
            report(false, "Another usit session owns hold-to-talk. Disable it there before enabling it here.");
            return;
        }
        ownsService = true;
        const auto conflicts = KGlobalAccel::globalShortcutsByKey(desiredKey);
        for (const auto &conflict : conflicts) {
            if (conflict.componentUniqueName() == "usit" && conflict.uniqueName() == "hold_to_dictate") continue;
            report(false, QString("%1 is already assigned to %2 — %3. Choose another hold key.")
                   .arg(requested, conflict.componentFriendlyName(), conflict.friendlyName()));
            return;
        }
        if (!KGlobalAccel::isGlobalShortcutAvailable(desiredKey, "usit")) {
            report(false, requested + " is already assigned in KDE. Choose another hold key.");
            return;
        }
        changing = true;
        const bool success = KGlobalAccel::self()->setShortcut(&action, {desiredKey}, KGlobalAccel::NoAutoloading);
        changing = false;
        bound = true;
        const auto actual = KGlobalAccel::self()->shortcut(&action);
        registered = success && actual.contains(desiredKey);
        if (!registered) {
            clearBinding();
            report(false, "KDE could not register " + requested + ". Choose another key or check KDE Shortcuts.");
            return;
        }
        report(true, {}, desiredKey.toString(QKeySequence::PortableText));
    }
    Bridge &bridge;
    bool testing = false, configured = false, wanted = false, registered = false;
    bool held = false, changing = false, bound = false, ownsService = false;
    QString requested;
    QKeySequence desiredKey;
    QAction action;
    QDBusServiceWatcher *watcher = nullptr;
    QTimer health;
};
