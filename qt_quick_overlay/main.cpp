#include <QGuiApplication>
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

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    app.setApplicationName("usit");
    Bridge bridge;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("usitBridge", &bridge);
    engine.load(QUrl(QStringLiteral("qrc:/Main.qml")));
    if (engine.rootObjects().isEmpty()) return 1;
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
            app.exit(invoked && result.toBool() ? 0 : 2);
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
