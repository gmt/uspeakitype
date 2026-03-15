#include <QApplication>
#include <QCoreApplication>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QMainWindow>
#include <QPainter>
#include <QProcess>
#include <QPushButton>
#include <QTextEdit>
#include <QTimer>
#include <QVBoxLayout>

#include <algorithm>
#include <cmath>

class LocalMeter : public QWidget {
public:
    explicit LocalMeter(QWidget *parent = nullptr) : QWidget(parent) {
        setMinimumHeight(140);
    }

    void setLevel(float level) {
        level_ = std::clamp(level, 0.0f, 1.0f);
        update();
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing, true);
        painter.fillRect(rect(), QColor("#101010"));

        QRectF frame = rect().adjusted(10, 10, -10, -10);
        painter.setPen(QPen(QColor("#2e4b39"), 1.0));
        painter.setBrush(QColor("#152019"));
        painter.drawRoundedRect(frame, 12, 12);

        QRectF bar = frame.adjusted(10, 10, -10, -10);
        const qreal filled = bar.width() * level_;
        painter.setPen(Qt::NoPen);
        painter.setBrush(QColor("#76b66f"));
        painter.drawRoundedRect(QRectF(bar.left(), bar.top(), filled, bar.height()), 10, 10);
    }

private:
    float level_ = 0.0f;
};

class NucitWindow : public QMainWindow {
public:
    explicit NucitWindow(const QString &workerPath) {
        setWindowTitle("nucit");
        resize(820, 420);

        auto *root = new QWidget(this);
        auto *layout = new QVBoxLayout(root);
        layout->setContentsMargins(18, 18, 18, 18);
        layout->setSpacing(14);
        setCentralWidget(root);

        auto *title = new QLabel("nucit", root);
        title->setStyleSheet("font-size: 28px; font-weight: 700; color: #e7efe5;");
        layout->addWidget(title);

        status_ = new QLabel("C++ owns the local meter loop", root);
        status_->setStyleSheet("color: #c0d0c3;");
        layout->addWidget(status_);

        meter_ = new LocalMeter(root);
        layout->addWidget(meter_);

        workerNotes_ = new QTextEdit(root);
        workerNotes_->setReadOnly(true);
        layout->addWidget(workerNotes_, 1);

        auto *controls = new QHBoxLayout();
        auto *pauseButton = new QPushButton("Pause Worker", root);
        auto *quitButton = new QPushButton("Quit", root);
        controls->addWidget(pauseButton);
        controls->addStretch();
        controls->addWidget(quitButton);
        layout->addLayout(controls);

        root->setStyleSheet(R"(
            QWidget {
                background: #0d120f;
                color: #e8efe5;
                font-family: "Noto Sans", "DejaVu Sans", sans-serif;
                font-size: 14px;
            }
            QTextEdit {
                background: #151b17;
                border: 1px solid #2f4735;
                border-radius: 12px;
                padding: 10px;
            }
            QPushButton {
                background: #1d2a20;
                border: 1px solid #3f6148;
                border-radius: 12px;
                padding: 8px 14px;
            }
        )");

        worker_.setProgram(workerPath);
        connect(&worker_, &QProcess::readyReadStandardOutput, this, [this] { consumeWorker(); });
        connect(&worker_, &QProcess::readyReadStandardError, this, [this] {
            workerNotes_->append(QString::fromUtf8(worker_.readAllStandardError()));
        });
        worker_.start();

        connect(pauseButton, &QPushButton::clicked, this, [this] { sendCommand("toggle_pause"); });
        connect(quitButton, &QPushButton::clicked, this, [this] {
            sendCommand("quit");
            close();
        });

        timer_.setInterval(33);
        connect(&timer_, &QTimer::timeout, this, [this] { tickLocalAudio(); });
        timer_.start();
    }

    ~NucitWindow() override {
        timer_.stop();
        if (worker_.state() != QProcess::NotRunning) {
            sendCommand("quit");
            worker_.waitForFinished(500);
        }
    }

private:
    void tickLocalAudio() {
        phase_ += 0.11;
        const float level = std::clamp(0.18f + std::abs(std::sin(phase_)) * 0.78f, 0.0f, 1.0f);
        meter_->setLevel(level);
        status_->setText(QString("local C++ frame level: %1").arg(level, 0, 'f', 2));

        QJsonObject object;
        object.insert("type", "audio_frame");
        object.insert("level", level);
        sendObject(object);
    }

    void sendCommand(const QString &type) {
        QJsonObject object;
        object.insert("type", type);
        sendObject(object);
    }

    void sendObject(const QJsonObject &object) {
        if (worker_.state() != QProcess::Running) {
            return;
        }
        worker_.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        worker_.write("\n");
        worker_.waitForBytesWritten(20);
    }

    void consumeWorker() {
        buffer_.append(worker_.readAllStandardOutput());
        while (true) {
            const int newline = buffer_.indexOf('\n');
            if (newline < 0) {
                break;
            }

            const QByteArray line = buffer_.left(newline).trimmed();
            buffer_.remove(0, newline + 1);
            if (line.isEmpty()) {
                continue;
            }

            const auto doc = QJsonDocument::fromJson(line);
            if (!doc.isObject()) {
                continue;
            }

            const auto object = doc.object();
            const auto frames = object.value("frames_seen").toInt();
            const auto analysis = object.value("analysis").toString();
            const auto advice = object.value("advice").toString();
            workerNotes_->setPlainText(
                QString("frames seen: %1\n\n%2\n\n%3").arg(frames).arg(analysis, advice));
        }
    }

    QProcess worker_;
    QByteArray buffer_;
    QTimer timer_;
    float phase_ = 0.0f;
    LocalMeter *meter_ = nullptr;
    QLabel *status_ = nullptr;
    QTextEdit *workerNotes_ = nullptr;
};

static QString defaultWorkerPath() {
    const QString appDir = QCoreApplication::applicationDirPath();
    return QFileInfo(appDir + "/../../rust_worker/target/debug/nucit-rust-worker").absoluteFilePath();
}

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    const QString workerPath = argc > 1 ? QString::fromLocal8Bit(argv[1]) : defaultWorkerPath();
    NucitWindow window(workerPath);
    window.show();
    return app.exec();
}
