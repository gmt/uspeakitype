#include <QApplication>
#include "bridge.h"
#include "hotkey.h"
#include <QAction>
#include <QDialog>
#include <QDialogButtonBox>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QMenu>
#include <QSystemTrayIcon>
#include <QVBoxLayout>
#include <QImage>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QQuickItem>
#include <QScreen>
#include <QTimer>
#include <QTest>
#include <LayerShellQt/Window>
#include <csignal>
#include <functional>

// The tray is a second view of engine state; actions use the same commands as QML.
class Tray : public QObject {
    Q_OBJECT
public:
    explicit Tray(Bridge &bridge) : bridge(bridge) {
        showAction = menu.addAction("Hide transcript");
        auto *settingsAction = menu.addAction("Settings…");
        menu.addSeparator();
        listenAction = menu.addAction("Listening");
        listenAction->setCheckable(true);
        holdAction = menu.addAction("Hold to talk");
        holdAction->setCheckable(true);
        typeAction = menu.addAction("Type into focused application");
        typeAction->setCheckable(true);
        menu.addSeparator();
        quitAction = menu.addAction("Quit usit");
        connect(showAction, &QAction::triggered, this, &Tray::toggleWindow);
        connect(settingsAction, &QAction::triggered, this, &Tray::showSettings);
        connect(listenAction, &QAction::triggered, this, [this] { this->bridge.send("toggle_pause"); update(); });
        connect(holdAction, &QAction::triggered, this, [this] { this->bridge.send("toggle_ptt"); update(); });
        connect(typeAction, &QAction::triggered, this, [this] { this->bridge.send("toggle_injection"); update(); });
        connect(quitAction, &QAction::triggered, this, [this] { this->bridge.send("quit"); QCoreApplication::quit(); });
        icon.setIcon(QIcon(":/assets/usit_tux_relaxed_icon_tile.png"));
        icon.setContextMenu(&menu);
        connect(&icon, &QSystemTrayIcon::activated, this, [this](auto reason) {
            if (reason == QSystemTrayIcon::Trigger) toggleWindow();
        });
        connect(&bridge, &Bridge::stateChanged, this, &Tray::update);
        // If the tray host disappears, keep the passive Controls button reachable.
        connect(&availability, &QTimer::timeout, this, [this] {
            if (window && !window->isVisible() && !QSystemTrayIcon::isSystemTrayAvailable()) showWindow();
        });
        availability.start(2000);
        update();
        icon.show();
    }
    void attach(QQuickWindow *display, QQuickWindow *controls) {
        window = display;
        settings = controls;
        connect(window, &QWindow::visibleChanged, this, &Tray::update);
    }
    Q_INVOKABLE bool hideToTray() {
        if (!window || !QSystemTrayIcon::isSystemTrayAvailable()) return false;
        window->hide();
        return true;
    }
    void showWindow() {
        if (window) window->show(); // Never activate the passive transcript surface.
    }
    void toggleWindow() {
        if (window && window->isVisible()) hideToTray();
        else showWindow();
    }
    Q_INVOKABLE void showSettings() {
        if (!settings) return;
        settings->showNormal();
        settings->raise();
        settings->requestActivate(); // Only an explicit Settings action takes focus.
    }
    Q_INVOKABLE void editPttShortcut() {
        QDialog dialog;
        dialog.setWindowTitle("usit — Hold key");
        dialog.setWindowModality(Qt::ApplicationModal);
        dialog.winId();
        if (settings) dialog.windowHandle()->setTransientParent(settings);
        auto *layout = new QVBoxLayout(&dialog);
        layout->addWidget(new QLabel("Press one key or key combination to hold while speaking."));
        auto *editor = new QKeySequenceEdit(QKeySequence::fromString(
            bridge.state().value("ptt_shortcut").toString(), QKeySequence::PortableText));
        editor->setMaximumSequenceLength(1);
        layout->addWidget(editor);
        auto *buttons = new QDialogButtonBox(QDialogButtonBox::Save | QDialogButtonBox::Cancel);
        layout->addWidget(buttons);
        connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
        connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
        editor->setFocus();
        if (dialog.exec() == QDialog::Accepted && !editor->keySequence().isEmpty())
            bridge.send("set_ptt_shortcut", editor->keySequence().toString(QKeySequence::PortableText));
    }
    void update() {
        const auto state = bridge.state();
        const bool ready = state.value("transcription_available").toBool();
        const bool paused = state.value("paused").toBool();
        const bool typing = state.value("injection_enabled").toBool();
        const bool ptt = state.value("push_to_talk").toBool();
        const bool held = state.value("ptt_held").toBool();
        listenAction->setChecked(!paused);
        listenAction->setEnabled(!state.isEmpty() && !ptt);
        holdAction->setChecked(ptt);
        typeAction->setChecked(typing);
        typeAction->setEnabled(ready);
        showAction->setText(window && window->isVisible() ? "Hide transcript" : "Show transcript");
        const QString mode = state.isEmpty() ? "Connecting" :
            !state.value("ptt_error").toString().isEmpty() ? state.value("ptt_error").toString() :
            ptt ? (held ? QString("Dictating") : QString("Hold %1 to dictate").arg(state.value("ptt_shortcut").toString())) :
            paused ? "Paused" : ready ? "Listening" : "Model unavailable";
        const auto tooltip = QString("usit — %1\n%2 · Typing %3")
            .arg(mode, state.value("model_label").toString(), typing ? "on" : "off");
        if (icon.toolTip() != tooltip) icon.setToolTip(tooltip);
    }
    void quit() { quitAction->trigger(); }
    bool testActions() {
        if (icon.icon().isNull()) return false;
        if (listenAction->isChecked() || !typeAction->isEnabled() || typeAction->isChecked()) return false;
        listenAction->trigger();
        typeAction->trigger();
        holdAction->trigger();
        return true;
    }
private:
    Bridge &bridge;
    QQuickWindow *window = nullptr, *settings = nullptr;
    QMenu menu;
    QSystemTrayIcon icon;
    QTimer availability;
    QAction *showAction, *listenAction, *holdAction, *typeAction, *quitAction;
};

// In hold-to-talk mode the transcript follows dictation: it appears when the key
// goes down and leaves shortly after the phrase has been typed. Untyped text stays
// visible so it can still be read.
class DictationVisibility : public QObject {
public:
    DictationVisibility(Bridge &bridge, std::function<void()> show, std::function<void()> hide,
                        int delayMs = 1500)
        : bridge(bridge), show(std::move(show)), hide(std::move(hide)) {
        timer.setSingleShot(true);
        timer.setInterval(delayMs);
        connect(&timer, &QTimer::timeout, this, [this] {
            const auto state = this->bridge.state();
            if (state.value("push_to_talk").toBool() && !state.value("ptt_held").toBool()) this->hide();
        });
        connect(&bridge, &Bridge::stateChanged, this, &DictationVisibility::update);
    }
private:
    void update() {
        const auto state = bridge.state();
        const bool ptt = state.value("push_to_talk").toBool();
        const bool held = ptt && state.value("ptt_held").toBool();
        const qint64 typed = state.value("injected").toLongLong();
        if (held && !wasHeld) {
            timer.stop();
            show();
        } else if (ptt && !held && injected >= 0 && typed > injected) {
            timer.start();
        }
        wasHeld = held;
        injected = typed;
    }
    Bridge &bridge;
    std::function<void()> show, hide;
    QTimer timer;
    bool wasHeld = false;
    qint64 injected = -1;
};

static LayerShellQt::Window *configureDisplay(QQuickWindow *window) {
    // Attach before the first show() so Qt never maps this as a focusable xdg window.
    if (QGuiApplication::platformName().startsWith("wayland")) {
        auto *layer = LayerShellQt::Window::get(window);
        layer->setScope("usit-dictation");
        layer->setLayer(LayerShellQt::Window::LayerOverlay);
        layer->setAnchors(LayerShellQt::Window::AnchorBottom);
        layer->setMargins(QMargins(16, 16, 16, 24));
        layer->setExclusiveZone(0);
        layer->setKeyboardInteractivity(LayerShellQt::Window::KeyboardInteractivityNone);
        layer->setActivateOnShow(false);
        return layer;
    }
    // Offscreen tests and X11 retain the same non-focusable window flags.
    if (auto *screen = window->screen()) {
        const auto geometry = screen->availableGeometry();
        window->setPosition(geometry.center().x() - window->width() / 2,
                            geometry.bottom() - window->height() - 24);
    }
    return nullptr;
}

int main(int argc, char **argv) {
    std::signal(SIGPIPE, SIG_IGN); // Pipe loss must still release/unregister a held key.
    QApplication app(argc, argv);
    app.setQuitOnLastWindowClosed(false);
    app.setApplicationName("usit");
    app.setDesktopFileName("usit");
    const auto args = app.arguments();
    Bridge bridge;
    const bool hotkeyTest = args.contains("--hotkey-self-test") || args.contains("--hotkey-disconnect-test");
    if (args.contains("--hotkey-only") || hotkeyTest) {
        GlobalHotkey hotkey(bridge, hotkeyTest);
        if (args.contains("--hotkey-self-test")) {
            QTimer::singleShot(150, &app, [&] { hotkey.selfTest(); app.quit(); });
        } else if (args.contains("--hotkey-disconnect-test")) {
            QTimer::singleShot(150, &app, [&] { hotkey.holdForDisconnectTest(); });
        }
        return app.exec();
    }
    app.setWindowIcon(QIcon(":/assets/usit_tux_relaxed_icon_tile.png"));
    QQuickWindow::setDefaultAlphaBuffer(true);
    Tray tray(bridge);
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("usitBridge", &bridge);
    engine.rootContext()->setContextProperty("usitTray", &tray);
    engine.load(QUrl(QStringLiteral("qrc:/Main.qml")));
    engine.load(QUrl(QStringLiteral("qrc:/Settings.qml")));
    if (engine.rootObjects().size() != 2) return 1;
    auto *display = qobject_cast<QQuickWindow *>(engine.rootObjects()[0]);
    auto *settings = qobject_cast<QQuickWindow *>(engine.rootObjects()[1]);
    if (!display || !settings) return 1;
    auto *layer = configureDisplay(display);
    tray.attach(display, settings);
    display->show();
    // A later launch of usit asks this session to show itself instead of starting.
    qint64 activation = -1;
    QObject::connect(&bridge, &Bridge::stateChanged, &app, [&] {
        const qint64 next = bridge.state().value("activation").toLongLong();
        if (activation >= 0 && next > activation) tray.showWindow();
        activation = next;
    });
    const bool autoHideTest = args.contains("--auto-hide-test");
    // Without a tray the hidden transcript would strand the Controls button.
    DictationVisibility dictation(bridge, [&] { tray.showWindow(); },
        [&] { if (autoHideTest) display->hide(); else tray.hideToTray(); },
        autoHideTest ? 100 : 1500);
    if (autoHideTest) {
        // Snapshots: idle, held (shows a hidden display), released and typed (hides it).
        display->hide();
        QObject::connect(&bridge, &Bridge::stateChanged, &app, [&] {
            const auto state = bridge.state();
            if (state.value("injected").toLongLong() == 0) return;
            QTimer::singleShot(400, &app, [&] { app.exit(display->isVisible() ? 2 : 0); });
        });
        QObject::connect(&bridge, &Bridge::stateChanged, &app, [&] {
            if (bridge.state().value("ptt_held").toBool() && !display->isVisible()) app.exit(3);
        });
        QTimer::singleShot(5000, &app, [&] { app.exit(4); });
    }
    if (args.contains("--activation-test")) {
        // The first snapshot hides the display; a later activation must restore it.
        QObject::connect(&bridge, &Bridge::stateChanged, &app, [&] {
            if (bridge.state().value("activation").toLongLong() == 0) display->hide();
            else app.exit(display->isVisible() ? 0 : 2);
        });
        QTimer::singleShot(5000, &app, [&] { app.exit(2); });
    }
    const int tag = args.indexOf("--tag");
    if (tag >= 0 && tag + 1 < args.size()) display->setTitle(QStringLiteral("usit — ") + args[tag + 1]);
    if (args.contains("--self-test")) {
        QTimer::singleShot(500, &app, [&] {
            if (!(display->flags() & Qt::WindowDoesNotAcceptFocus) || settings->isVisible()) { app.exit(2); return; }
            if (display->color().alpha() != 0) { app.exit(2); return; }
            if (layer && (layer->keyboardInteractivity() != LayerShellQt::Window::KeyboardInteractivityNone || layer->activateOnShow())) {
                app.exit(2); return;
            }
            QVariant displayResult;
            QMetaObject::invokeMethod(display, "selfTest", Q_RETURN_ARG(QVariant, displayResult));
            if (!displayResult.toBool()) { app.exit(2); return; }
            // Only clicking Controls opens the separate focusable window.
            auto *open = display->findChild<QQuickItem *>("openSettings");
            QTest::mouseClick(display, Qt::LeftButton, Qt::NoModifier,
                              open->mapToScene(QPointF(open->width() / 2, open->height() / 2)).toPoint());
            if (!settings->isVisible() || (settings->flags() & Qt::WindowDoesNotAcceptFocus)) { app.exit(2); return; }
            QTest::mouseClick(settings, Qt::LeftButton, Qt::NoModifier, QPoint(160, 40));
            QTest::keyClick(settings, Qt::Key_Return);
            auto *selector = settings->findChild<QQuickItem *>("modelSelector");
            if (!selector) { app.exit(2); return; }
            const auto location = selector->mapToScene(QPointF(selector->width() / 2, 15));
            QTest::mouseClick(settings, Qt::LeftButton, Qt::NoModifier, location.toPoint());
            if (!selector->property("expanded").toBool() || selector->property("highlightedIndex").toInt() != 1) { app.exit(2); return; }
            QTest::keyClick(settings, Qt::Key_End);
            QTest::keyClick(settings, Qt::Key_Return);
            if (selector->property("expanded").toBool()) { app.exit(2); return; }
            QVariant result;
            const bool invoked = QMetaObject::invokeMethod(settings, "selfTest", Q_RETURN_ARG(QVariant, result));
            settings->close();
            if (settings->isVisible() || !display->isVisible()) { app.exit(2); return; }
            display->hide();
            tray.showWindow();
            if (!display->isVisible() || settings->isVisible()) { app.exit(2); return; }
            app.exit(invoked && result.toBool() && tray.testActions() ? 0 : 2);
        });
    }
    if (args.contains("--close-test") || args.contains("--tray-self-test")) {
        QTimer::singleShot(500, &app, [&] {
            const bool liveTray = args.contains("--tray-self-test");
            if (QSystemTrayIcon::isSystemTrayAvailable() != liveTray) { app.exit(2); return; }
            display->close();
            if (!liveTray) return;
            if (display->isVisible()) { app.exit(2); return; }
            QTimer::singleShot(100, &app, [&] {
                tray.toggleWindow();
                if (!display->isVisible() || settings->isVisible() || !tray.testActions()) { app.exit(2); return; }
                tray.quit();
            });
        });
    }
    const int screenshot = args.indexOf("--screenshot");
    if (screenshot >= 0 && screenshot + 1 < args.size()) {
        QTimer::singleShot(350, &app, [&] {
            if (!display->grabWindow().save(args[screenshot + 1])) app.exit(3);
        });
    }
    return app.exec();
}
#include "main.moc"
