#include <QApplication>
#include <QCheckBox>
#include <QFrame>
#include <QHBoxLayout>
#include <QLabel>
#include <QMainWindow>
#include <QProgressBar>
#include <QPushButton>
#include <QSlider>
#include <QTimer>
#include <QVBoxLayout>

#include <algorithm>
#include <cerrno>
#include <cstring>
#include <fcntl.h>
#include <string>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

namespace {

constexpr const char *kDefaultShmName = "/usit-shm-demo";
constexpr std::size_t kRegionSize = 576;
constexpr quint32 kVersion = 1;
constexpr quint32 kCommandNone = 0;
constexpr quint32 kCommandTogglePause = 1;
constexpr quint32 kCommandToggleInjection = 2;
constexpr quint32 kCommandSetGain = 3;
constexpr quint32 kCommandQuit = 4;

struct BridgeLayout {
    char magic[8];
    quint32 version;
    quint32 reserved0;
    quint64 snapshot_seq;
    quint64 command_seq;
    quint64 last_applied_command_seq;
    float level;
    float peak;
    float gain;
    quint8 paused;
    quint8 injection_enabled;
    quint8 quit_requested;
    quint8 reserved1;
    quint32 pending_command;
    float pending_value;
    char committed[128];
    char partial[128];
    char source_label[64];
    char model_label[64];
    char error_label[128];
};

static_assert(sizeof(BridgeLayout) == kRegionSize, "bridge layout drifted");

QString readText(const char *buffer, std::size_t len) {
    const auto end = std::find(buffer, buffer + len, '\0');
    return QString::fromUtf8(buffer, static_cast<int>(end - buffer));
}

class SharedRegion {
public:
    explicit SharedRegion(const QString &name) {
        const QByteArray rawName = name.toUtf8();
        fd_ = shm_open(rawName.constData(), O_CREAT | O_RDWR, 0600);
        if (fd_ < 0) {
            qFatal("shm_open failed: %s", std::strerror(errno));
        }

        if (ftruncate(fd_, static_cast<off_t>(kRegionSize)) != 0) {
            qFatal("ftruncate failed: %s", std::strerror(errno));
        }

        void *ptr = mmap(nullptr, kRegionSize, PROT_READ | PROT_WRITE, MAP_SHARED, fd_, 0);
        if (ptr == MAP_FAILED) {
            qFatal("mmap failed: %s", std::strerror(errno));
        }

        bridge_ = static_cast<BridgeLayout *>(ptr);
    }

    ~SharedRegion() {
        if (bridge_ != nullptr) {
            munmap(bridge_, kRegionSize);
        }
        if (fd_ >= 0) {
            close(fd_);
        }
    }

    BridgeLayout *bridge() const { return bridge_; }

private:
    int fd_ = -1;
    BridgeLayout *bridge_ = nullptr;
};

class Window : public QMainWindow {
public:
    explicit Window(const QString &shmName, int autoQuitMs) : region_(shmName) {
        setWindowTitle("usit shared-memory shell");
        resize(860, 320);

        auto *root = new QWidget(this);
        setCentralWidget(root);
        root->setStyleSheet(R"(
            QWidget { background: #160f0b; color: #ecd9c5; font-family: "Noto Sans", sans-serif; }
            QFrame#card { background: #221610; border: 1px solid #4d3627; border-radius: 16px; }
            QPushButton { background: #342116; border: 1px solid #684a34; border-radius: 10px; padding: 8px 12px; }
            QPushButton:pressed { background: #28180f; }
            QCheckBox { spacing: 8px; }
            QSlider::groove:horizontal { background: #2a1b12; border: 1px solid #5a412f; height: 8px; border-radius: 4px; }
            QSlider::handle:horizontal { background: #d5a057; border: 1px solid #f0c77c; width: 18px; margin: -6px 0; border-radius: 9px; }
            QProgressBar { background: #1a120d; border: 1px solid #4c3525; border-radius: 8px; text-align: center; }
            QProgressBar::chunk { background: #b8792f; border-radius: 7px; }
            QLabel#title { font-size: 24px; font-weight: 700; }
            QLabel#muted { color: #bba58e; }
        )");

        auto *layout = new QHBoxLayout(root);
        layout->setContentsMargins(18, 18, 18, 18);
        layout->setSpacing(16);

        auto *leftCard = new QFrame(root);
        leftCard->setObjectName("card");
        auto *leftLayout = new QVBoxLayout(leftCard);
        leftLayout->setContentsMargins(18, 18, 18, 18);
        leftLayout->setSpacing(12);
        layout->addWidget(leftCard, 2);

        auto *title = new QLabel("shared-memory usit sketch", leftCard);
        title->setObjectName("title");
        leftLayout->addWidget(title);

        summary_ = new QLabel("waiting for helper", leftCard);
        summary_->setObjectName("muted");
        leftLayout->addWidget(summary_);

        auto *meterLabel = new QLabel("Level meter", leftCard);
        leftLayout->addWidget(meterLabel);
        level_ = new QProgressBar(leftCard);
        level_->setRange(0, 100);
        leftLayout->addWidget(level_);

        peak_ = new QLabel("Peak: 0.00", leftCard);
        peak_->setObjectName("muted");
        leftLayout->addWidget(peak_);

        committed_ = new QLabel(leftCard);
        committed_->setWordWrap(true);
        leftLayout->addWidget(committed_);

        partial_ = new QLabel(leftCard);
        partial_->setObjectName("muted");
        partial_->setWordWrap(true);
        leftLayout->addWidget(partial_);

        auto *rightCard = new QFrame(root);
        rightCard->setObjectName("card");
        auto *rightLayout = new QVBoxLayout(rightCard);
        rightLayout->setContentsMargins(18, 18, 18, 18);
        rightLayout->setSpacing(10);
        layout->addWidget(rightCard, 1);

        paused_ = new QCheckBox("Paused", rightCard);
        injection_ = new QCheckBox("Injection enabled", rightCard);
        rightLayout->addWidget(paused_);
        rightLayout->addWidget(injection_);

        gain_ = new QSlider(Qt::Horizontal, rightCard);
        gain_->setRange(50, 200);
        gain_->setValue(100);
        rightLayout->addWidget(new QLabel("Gain", rightCard));
        rightLayout->addWidget(gain_);

        source_ = new QLabel("Source: ?", rightCard);
        source_->setObjectName("muted");
        model_ = new QLabel("Model: ?", rightCard);
        model_->setObjectName("muted");
        error_ = new QLabel("", rightCard);
        error_->setWordWrap(true);
        rightLayout->addWidget(source_);
        rightLayout->addWidget(model_);
        rightLayout->addWidget(error_);
        rightLayout->addStretch(1);

        auto *buttons = new QHBoxLayout();
        auto *quitButton = new QPushButton("Quit helper", rightCard);
        auto *refreshButton = new QPushButton("Force read", rightCard);
        buttons->addWidget(refreshButton);
        buttons->addWidget(quitButton);
        rightLayout->addLayout(buttons);

        connect(paused_, &QCheckBox::clicked, this, [this] {
            writeCommand(kCommandTogglePause, 0.0f);
        });
        connect(injection_, &QCheckBox::clicked, this, [this] {
            writeCommand(kCommandToggleInjection, 0.0f);
        });
        connect(gain_, &QSlider::sliderReleased, this, [this] {
            writeCommand(kCommandSetGain, gain_->value() / 100.0f);
        });
        connect(quitButton, &QPushButton::clicked, this, [this] {
            writeCommand(kCommandQuit, 0.0f);
        });
        connect(refreshButton, &QPushButton::clicked, this, [this] {
            applySnapshot();
        });

        timer_ = new QTimer(this);
        timer_->setInterval(33);
        connect(timer_, &QTimer::timeout, this, [this] { applySnapshot(); });
        timer_->start();

        if (autoQuitMs > 0) {
            QTimer::singleShot(autoQuitMs, this, [this] {
                close();
            });
        }
    }

protected:
    void closeEvent(QCloseEvent *event) override {
        writeCommand(kCommandQuit, 0.0f);
        QMainWindow::closeEvent(event);
    }

private:
    void applySnapshot() {
        auto *bridge = region_.bridge();
        if (bridge->version != kVersion) {
            summary_->setText("version mismatch or helper not initialized yet");
            return;
        }

        if (bridge->snapshot_seq == lastSnapshotSeq_) {
            return;
        }
        lastSnapshotSeq_ = bridge->snapshot_seq;

        const bool paused = bridge->paused != 0;
        const bool injectionEnabled = bridge->injection_enabled != 0;

        syncing_ = true;
        paused_->setChecked(paused);
        injection_->setChecked(injectionEnabled);
        gain_->setValue(static_cast<int>(bridge->gain * 100.0f));
        syncing_ = false;

        level_->setValue(static_cast<int>(std::clamp(bridge->level, 0.0f, 1.0f) * 100.0f));
        peak_->setText(QString("Peak: %1").arg(bridge->peak, 0, 'f', 2));
        summary_->setText(QString("seq %1 · cmd %2/%3")
                              .arg(bridge->snapshot_seq)
                              .arg(bridge->last_applied_command_seq)
                              .arg(bridge->command_seq));
        committed_->setText(readText(bridge->committed, sizeof(bridge->committed)));
        partial_->setText(readText(bridge->partial, sizeof(bridge->partial)));
        source_->setText(readText(bridge->source_label, sizeof(bridge->source_label)));
        model_->setText(readText(bridge->model_label, sizeof(bridge->model_label)));
        error_->setText(readText(bridge->error_label, sizeof(bridge->error_label)));
    }

    void writeCommand(quint32 command, float value) {
        if (syncing_) {
            return;
        }

        auto *bridge = region_.bridge();
        bridge->pending_command = command;
        bridge->pending_value = value;
        bridge->command_seq = bridge->command_seq + 1;
    }

    SharedRegion region_;
    QTimer *timer_ = nullptr;
    quint64 lastSnapshotSeq_ = 0;
    bool syncing_ = false;

    QLabel *summary_ = nullptr;
    QProgressBar *level_ = nullptr;
    QLabel *peak_ = nullptr;
    QLabel *committed_ = nullptr;
    QLabel *partial_ = nullptr;
    QCheckBox *paused_ = nullptr;
    QCheckBox *injection_ = nullptr;
    QSlider *gain_ = nullptr;
    QLabel *source_ = nullptr;
    QLabel *model_ = nullptr;
    QLabel *error_ = nullptr;
};

} // namespace

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);

    QString shmName = QString::fromUtf8(kDefaultShmName);
    int autoQuitMs = 0;

    for (int i = 1; i < argc; ++i) {
        const QString arg = QString::fromUtf8(argv[i]);
        if (arg == "--shm-name" && i + 1 < argc) {
            shmName = QString::fromUtf8(argv[++i]);
        } else if (arg == "--auto-quit-ms" && i + 1 < argc) {
            autoQuitMs = QString::fromUtf8(argv[++i]).toInt();
        }
    }

    Window window(shmName, autoQuitMs);
    window.show();
    return app.exec();
}
