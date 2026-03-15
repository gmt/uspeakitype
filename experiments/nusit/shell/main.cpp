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
#include <QProgressBar>
#include <QTextEdit>
#include <QVBoxLayout>
#include <QWidget>

class LevelMeter : public QWidget {
public:
    explicit LevelMeter(QWidget *parent = nullptr) : QWidget(parent) {
        setMinimumHeight(120);
    }

    void setLevel(float level) {
        level_ = std::clamp(level, 0.0f, 1.0f);
        update();
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing, true);
        painter.fillRect(rect(), QColor("#17100d"));

        QRectF frame = rect().adjusted(10, 10, -10, -10);
        painter.setPen(QPen(QColor("#4f382a"), 1.0));
        painter.setBrush(QColor("#211611"));
        painter.drawRoundedRect(frame, 14, 14);

        QRectF bar = frame.adjusted(12, 12, -12, -12);
        const qreal width = bar.width() * level_;
        QRectF fill(bar.left(), bar.top(), width, bar.height());

        QLinearGradient gradient(fill.topLeft(), fill.topRight());
        gradient.setColorAt(0.0, QColor("#7f3a26"));
        gradient.setColorAt(1.0, QColor("#dca653"));
        painter.setPen(Qt::NoPen);
        painter.setBrush(gradient);
        painter.drawRoundedRect(fill, 12, 12);
    }

private:
    float level_ = 0.0f;
};

class NusitWindow : public QMainWindow {
public:
    explicit NusitWindow(const QString &helperPath) {
        setWindowTitle("nusit");
        resize(760, 360);

        auto *root = new QWidget(this);
        auto *layout = new QVBoxLayout(root);
        layout->setContentsMargins(18, 18, 18, 18);
        layout->setSpacing(14);
        setCentralWidget(root);

        auto *header = new QHBoxLayout();
        auto *title = new QLabel("nusit", root);
        title->setStyleSheet("font-size: 28px; font-weight: 700; color: #f1e5d3;");
        header->addWidget(title);
        header->addStretch();

        status_ = new QLabel("starting helper…", root);
        status_->setStyleSheet("color: #d1c0ac;");
        header->addWidget(status_);
        layout->addLayout(header);

        meter_ = new LevelMeter(root);
        layout->addWidget(meter_);

        transcript_ = new QTextEdit(root);
        transcript_->setReadOnly(true);
        transcript_->setMinimumHeight(120);
        layout->addWidget(transcript_);

        auto *controls = new QHBoxLayout();
        auto *pauseButton = new QPushButton("Pause / Resume", root);
        auto *injectButton = new QPushButton("Toggle Injection", root);
        auto *quitButton = new QPushButton("Quit", root);
        controls->addWidget(pauseButton);
        controls->addWidget(injectButton);
        controls->addStretch();
        controls->addWidget(quitButton);
        layout->addLayout(controls);

        root->setStyleSheet(R"(
            QWidget {
                background: #140d0a;
                color: #eadfcf;
                font-family: "Noto Sans", "DejaVu Sans", sans-serif;
                font-size: 14px;
            }
            QTextEdit {
                background: #1d1511;
                border: 1px solid #4b3428;
                border-radius: 12px;
                padding: 10px;
            }
            QPushButton {
                background: #281a13;
                border: 1px solid #584031;
                border-radius: 12px;
                padding: 8px 14px;
                color: #f1e6d7;
            }
        )");

        helper_.setProgram(helperPath);
        connect(&helper_, &QProcess::readyReadStandardOutput, this, [this] { consumeStdout(); });
        connect(&helper_, &QProcess::readyReadStandardError, this, [this] {
            transcript_->append(QString::fromUtf8(helper_.readAllStandardError()));
        });
        connect(&helper_, &QProcess::errorOccurred, this, [this](QProcess::ProcessError) {
            status_->setText("helper failed to start");
        });
        connect(&helper_,
                qOverload<int, QProcess::ExitStatus>(&QProcess::finished),
                this,
                [this](int code, QProcess::ExitStatus) {
                    status_->setText(QString("helper exited (%1)").arg(code));
                });

        connect(pauseButton, &QPushButton::clicked, this, [this] { sendCommand("toggle_pause"); });
        connect(injectButton, &QPushButton::clicked, this, [this] {
            sendCommand("toggle_injection");
        });
        connect(quitButton, &QPushButton::clicked, this, [this] {
            sendCommand("quit");
            close();
        });

        helper_.start();
    }

    ~NusitWindow() override {
        if (helper_.state() != QProcess::NotRunning) {
            sendCommand("quit");
            helper_.waitForFinished(500);
        }
    }

private:
    void sendCommand(const QString &type) {
        if (helper_.state() != QProcess::Running) {
            return;
        }
        QJsonObject object;
        object.insert("type", type);
        helper_.write(QJsonDocument(object).toJson(QJsonDocument::Compact));
        helper_.write("\n");
        helper_.waitForBytesWritten(50);
    }

    void consumeStdout() {
        buffer_.append(helper_.readAllStandardOutput());
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
            meter_->setLevel(static_cast<float>(object.value("level").toDouble()));
            status_->setText(object.value("status").toString("unknown"));
            transcript_->setPlainText(object.value("transcript").toString());
        }
    }

    QProcess helper_;
    QByteArray buffer_;
    LevelMeter *meter_ = nullptr;
    QLabel *status_ = nullptr;
    QTextEdit *transcript_ = nullptr;
};

static QString defaultHelperPath() {
    const QString appDir = QCoreApplication::applicationDirPath();
    return QFileInfo(appDir + "/../../rust_helper/target/debug/nusit-rust-helper").absoluteFilePath();
}

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    const QString helperPath = argc > 1 ? QString::fromLocal8Bit(argv[1]) : defaultHelperPath();
    NusitWindow window(helperPath);
    window.show();
    return app.exec();
}
