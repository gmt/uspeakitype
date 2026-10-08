#pragma once
#include <QObject>
#include <QCoreApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSocketNotifier>
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
        sendObject(command);
    }
    void sendObject(const QJsonObject &command) {
        const QByteArray line = QJsonDocument(command).toJson(QJsonDocument::Compact) + '\n';
        if (fwrite(line.constData(), 1, line.size(), stdout) != size_t(line.size()) || fflush(stdout))
            QCoreApplication::quit();
    }
signals:
    void stateChanged();
    void disconnected();
private:
    void readInput() {
        char bytes[16384];
        while (true) {
            const auto count = read(STDIN_FILENO, bytes, sizeof(bytes));
            if (count == 0) {
                notifier->setEnabled(false);
                emit disconnected();
                QCoreApplication::quit();
                return;
            }
            if (count < 0) {
                if (errno == EINTR) continue;
                if (errno != EAGAIN && errno != EWOULDBLOCK) {
                    emit disconnected();
                    QCoreApplication::quit();
                }
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
