#include <QApplication>
#include <QAction>
#include <QMenu>
#include <QSystemTrayIcon>
#include <QImage>
#include <QJsonDocument>
#include <QJsonObject>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QQuickItem>
#include <QSocketNotifier>
#include <QTimer>
#include <QTest>
#include <QVariantMap>
#include <cerrno>
#include <cstdio>
#include <fcntl.h>
#include <unistd.h>

// The engine owns capture and recognition. This process only displays snapshots
// and sends user intent; stdout is reserved for the newline JSON protocol.
class Bridge : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap state READ state NOTIFY stateChanged)
public:
    explicit Bridge(QObject *parent = nullptr) : QObject(parent) {
        const int flags = fcntl(STDIN_FILENO, F_GETFL);
        if (flags < 0 || fcntl(STDIN_FILENO, F_SETFL, flags | O_NONBLOCK) < 0)
            qFatal("Unable to make snapshot input nonblocking");
        notifier = new QSocketNotifier(STDIN_FILENO, QSocketNotifier::Read, this);
        connect(notifier, &QSocketNotifier::activated, this, [this] { readInput(); });
    }
    QVariantMap state() const { return snapshot; }
    Q_INVOKABLE void send(const QString &type, const QVariant &value = QVariant()) {
        QJsonObject command{{"type", type}};
        if (value.isValid() && !value.isNull())
            command.insert("value", QJsonValue::fromVariant(value));
        const QByteArray line = QJsonDocument(command).toJson(QJsonDocument::Compact) + '\n';
        if (fwrite(line.constData(), 1, line.size(), stdout) != size_t(line.size()) || fflush(stdout))
            QCoreApplication::quit();
    }
signals:
    void stateChanged();
private:
    void readInput() {
        char bytes[16384];
        while (true) {
            const auto count = read(STDIN_FILENO, bytes, sizeof(bytes));
            if (count == 0) {
                notifier->setEnabled(false);
                QCoreApplication::quit();
                return;
            }
            if (count < 0) {
                if (errno == EINTR) continue;
                if (errno != EAGAIN && errno != EWOULDBLOCK) QCoreApplication::quit();
                return;
            }
            input.append(bytes, count);
            qsizetype newline;
            while ((newline = input.indexOf('\n')) >= 0) {
                const auto line = input.left(newline);
                input.remove(0, newline + 1);
                QJsonParseError error;
                const auto document = QJsonDocument::fromJson(line, &error);
                if (error.error != QJsonParseError::NoError || !document.isObject()) {
                    qWarning("Ignoring invalid snapshot JSON");
                    continue;
                }
                snapshot = document.object().toVariantMap();
                emit stateChanged();
            }
            if (input.size() > 4 * 1024 * 1024) {
                qWarning("Snapshot line exceeds 4 MiB");
                QCoreApplication::exit(2);
                return;
            }
        }
    }
    QVariantMap snapshot;
    QByteArray input;
    QSocketNotifier *notifier;
};

// The tray is a second view of engine state; actions use the same commands as QML.
class Tray : public QObject {
    Q_OBJECT
public:
    explicit Tray(Bridge &bridge) : bridge(bridge) {
        showAction = menu.addAction("Hide window");
        menu.addSeparator();
        listenAction = menu.addAction("Listening");
        listenAction->setCheckable(true);
        typeAction = menu.addAction("Type into focused application");
        typeAction->setCheckable(true);
        menu.addSeparator();
        quitAction = menu.addAction("Quit usit");
        connect(showAction, &QAction::triggered, this, &Tray::toggleWindow);
        connect(listenAction, &QAction::triggered, this, [this] {
            this->bridge.send("toggle_pause");
            update();
        });
        connect(typeAction, &QAction::triggered, this, [this] {
            this->bridge.send("toggle_injection");
            update();
        });
        connect(quitAction, &QAction::triggered, this, [this] {
            this->bridge.send("quit");
            QCoreApplication::quit();
        });
        icon.setIcon(QIcon(":/assets/usit_tux_relaxed_icon_tile.png"));
        icon.setContextMenu(&menu);
        connect(&icon, &QSystemTrayIcon::activated, this, [this](auto reason) {
            if (reason == QSystemTrayIcon::Trigger) toggleWindow();
        });
        connect(&bridge, &Bridge::stateChanged, this, &Tray::update);
        // Restore access to controls if the tray host disappears while hidden.
        connect(&availability, &QTimer::timeout, this, [this] {
            if (window && !window->isVisible() && !QSystemTrayIcon::isSystemTrayAvailable())
                showWindow();
        });
        availability.start(2000);
        update();
        icon.show();
    }
    void attach(QQuickWindow *target) {
        window = target;
        connect(window, &QWindow::visibleChanged, this, &Tray::update);
    }
    Q_INVOKABLE bool hideToTray() {
        if (!window || !QSystemTrayIcon::isSystemTrayAvailable()) return false;
        window->hide();
        return true;
    }
    void showWindow() {
        if (!window) return;
        window->showNormal();
        window->raise();
        window->requestActivate();
    }
    void toggleWindow() {
        if (window && window->isVisible()) hideToTray();
        else showWindow();
    }
    void update() {
        const auto state = bridge.state();
        const bool ready = state.value("transcription_available").toBool();
        const bool paused = state.value("paused").toBool();
        const bool typing = state.value("injection_enabled").toBool();
        listenAction->setChecked(!paused);
        listenAction->setEnabled(!state.isEmpty());
        typeAction->setChecked(typing);
        typeAction->setEnabled(ready);
        showAction->setText(window && window->isVisible() ? "Hide window" : "Show window");
        const auto mode = state.isEmpty() ? "Connecting" : paused ? "Paused" : ready ? "Listening" : "Model unavailable";
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
        return true;
    }
private:
    Bridge &bridge;
    QQuickWindow *window = nullptr;
    QMenu menu;
    QSystemTrayIcon icon;
    QTimer availability;
    QAction *showAction, *listenAction, *typeAction, *quitAction;
};

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    app.setQuitOnLastWindowClosed(false);
    app.setWindowIcon(QIcon(":/assets/usit_tux_relaxed_icon_tile.png"));
    app.setApplicationName("usit");
    app.setDesktopFileName("usit");
    Bridge bridge;
    Tray tray(bridge);
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("usitBridge", &bridge);
    engine.rootContext()->setContextProperty("usitTray", &tray);
    engine.load(QUrl(QStringLiteral("qrc:/Main.qml")));
    if (engine.rootObjects().isEmpty()) return 1;
    tray.attach(qobject_cast<QQuickWindow *>(engine.rootObjects().first()));
    const auto args = app.arguments();
    const int tag = args.indexOf("--tag");
    if (tag >= 0 && tag + 1 < args.size()) {
        if (auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first()))
            window->setTitle(QStringLiteral("usit — ") + args[tag + 1]);
    }
    if (args.contains("--self-test")) {
        QTimer::singleShot(500, &app, [&] {
            auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
            if (!window) { app.exit(2); return; }
            // Exercise the real MouseArea and focused keyboard handler without
            // accessing the user's desktop or synthesizing host input.
            QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, QPoint(160, 40));
            QTest::keyClick(window, Qt::Key_Return);
            auto *selector = window->findChild<QQuickItem *>("modelSelector");
            if (!selector) { app.exit(2); return; }
            const auto location = selector->mapToScene(QPointF(selector->width() / 2, 15));
            QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, location.toPoint());
            if (!selector->property("expanded").toBool()) { app.exit(2); return; }
            if (selector->property("highlightedIndex").toInt() != 1) { app.exit(2); return; }
            // Browsing all choices must not activate intermediate models.
            QTest::keyClick(window, Qt::Key_End);
            QTest::keyClick(window, Qt::Key_Return);
            if (selector->property("expanded").toBool()) { app.exit(2); return; }
            QVariant result;
            const bool invoked = QMetaObject::invokeMethod(engine.rootObjects().first(), "selfTest", Q_RETURN_ARG(QVariant, result));
            app.exit(invoked && result.toBool() && tray.testActions() ? 0 : 2);
        });
    }
    if (args.contains("--close-test") || args.contains("--tray-self-test")) {
        QTimer::singleShot(500, &app, [&] {
            const bool liveTray = args.contains("--tray-self-test");
            if (QSystemTrayIcon::isSystemTrayAvailable() != liveTray) { app.exit(2); return; }
            auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
            window->close();
            if (!liveTray) return; // No tray: onClosing sends quit and exits.
            if (window->isVisible()) { app.exit(2); return; }
            QTimer::singleShot(100, &app, [&, window] {
                tray.toggleWindow();
                if (!window->isVisible() || !tray.testActions()) { app.exit(2); return; }
                tray.quit();
            });
        });
    }
    const int screenshot = args.indexOf("--screenshot");
    if (screenshot >= 0 && screenshot + 1 < args.size()) {
        QTimer::singleShot(350, &app, [&] {
            auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
            if (!window || !window->grabWindow().save(args[screenshot + 1])) app.exit(3);
        });
    }
    return app.exec();
}
#include "main.moc"
