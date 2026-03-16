#include <QApplication>
#include <QCheckBox>
#include <QFile>
#include <QFrame>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QMainWindow>
#include <QPaintEvent>
#include <QPainter>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QScrollBar>
#include <QSlider>
#include <QSocketNotifier>
#include <QTextStream>
#include <QToolButton>
#include <QVBoxLayout>
#include <QVector>

#include <algorithm>
#include <cstdio>
#include <cmath>

class SpectrogramWidget : public QWidget {
public:
    explicit SpectrogramWidget(QWidget *parent = nullptr)
        : QWidget(parent) {
        setMinimumHeight(180);
    }

    void applySnapshot(const QJsonObject &snapshot) {
        const auto mode = snapshot.value("viz_mode").toString();
        waterfallMode_ = mode == "waterfall";

        QVector<float> nextSamples;
        const auto sampleArray = snapshot.value("samples").toArray();
        nextSamples.reserve(sampleArray.size());
        for (const auto &value : sampleArray) {
            nextSamples.push_back(static_cast<float>(value.toDouble()));
        }

        if (!nextSamples.isEmpty()) {
            samples_ = nextSamples;
            if (waterfallMode_) {
                history_.push_front(samples_);
                while (history_.size() > std::max(1, height() / 3)) {
                    history_.pop_back();
                }
            } else {
                history_.clear();
            }
            update();
        }
    }

protected:
    void paintEvent(QPaintEvent *) override {
        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing, true);

        painter.fillRect(rect(), QColor("#19110d"));

        QRectF viewport = rect().adjusted(12, 12, -12, -12);
        painter.setPen(QPen(QColor("#4a3427"), 1.0));
        painter.setBrush(QColor("#21160f"));
        painter.drawRoundedRect(viewport, 14, 14);

        QRectF content = viewport.adjusted(10, 10, -10, -10);
        painter.fillRect(content, QColor("#2a1b10"));

        if (samples_.isEmpty()) {
            return;
        }

        if (waterfallMode_) {
            paintWaterfall(painter, content);
        } else {
            paintBars(painter, content);
        }
    }

private:
    static QColor sampleColor(float value) {
        const float clamped = std::clamp(value, 0.0f, 1.0f);
        const int red = static_cast<int>(86 + clamped * 170);
        const int green = static_cast<int>(56 + clamped * 150);
        const int blue = static_cast<int>(24 + clamped * 48);
        return QColor(red, green, blue);
    }

    void paintBars(QPainter &painter, const QRectF &content) {
        const int count = std::max(1, static_cast<int>(samples_.size()));
        const qreal gap = 1.0;
        const qreal barWidth = std::max<qreal>(2.0, (content.width() - gap * (count - 1)) / count);

        painter.setPen(Qt::NoPen);
        for (int i = 0; i < count; ++i) {
            const float sample = std::clamp(samples_[i], 0.0f, 1.0f);
            const qreal barHeight = std::max<qreal>(4.0, sample * content.height());
            QRectF bar(
                content.left() + i * (barWidth + gap),
                content.bottom() - barHeight,
                barWidth,
                barHeight
            );
            painter.setBrush(sampleColor(sample));
            painter.drawRoundedRect(bar, 2.0, 2.0);
        }
    }

    void paintWaterfall(QPainter &painter, const QRectF &content) {
        const int rows = std::max(1, static_cast<int>(history_.size()));
        const int cols = std::max(1, static_cast<int>(samples_.size()));
        const qreal cellWidth = content.width() / cols;
        const qreal cellHeight = content.height() / rows;

        painter.setPen(Qt::NoPen);
        for (int row = 0; row < rows; ++row) {
            const auto &line = history_[row];
            for (int col = 0; col < cols && col < line.size(); ++col) {
                const float sample = std::clamp(line[col], 0.0f, 1.0f);
                painter.setBrush(sampleColor(sample));
                painter.drawRect(
                    QRectF(
                        content.left() + col * cellWidth,
                        content.top() + row * cellHeight,
                        std::ceil(cellWidth),
                        std::ceil(cellHeight)
                    )
                );
            }
        }
    }

    QVector<float> samples_;
    QVector<QVector<float>> history_;
    bool waterfallMode_ = true;
};

class OverlayWindow : public QMainWindow {
public:
    OverlayWindow() {
        setWindowTitle("usit Qt Overlay");
        resize(1100, 460);
        setMinimumSize(920, 360);

        auto *root = new QWidget(this);
        setCentralWidget(root);

        root->setStyleSheet(R"(
            QWidget {
                background-color: #140c0a;
                color: #eadfcf;
                font-family: "Noto Sans", "DejaVu Sans", sans-serif;
                font-size: 14px;
            }
            QFrame#shell, QFrame#drawer, QFrame#header, QFrame#transcriptCard {
                background-color: #1b120d;
                border: 1px solid #3a2a20;
                border-radius: 18px;
            }
            QFrame#drawer {
                background-color: #1a120d;
            }
            QLabel#title {
                font-size: 24px;
                font-weight: 700;
                color: #f2e7d4;
            }
            QLabel#summary {
                color: #baab98;
                font-size: 12px;
            }
            QLabel#transcript {
                font-size: 15px;
                color: #f3ebdc;
            }
            QLabel#partial {
                color: #bea98b;
            }
            QPushButton#record {
                background-color: #8c3b28;
                color: #f6ead6;
                border-radius: 18px;
                border: 2px solid #bb5a42;
                padding: 8px 14px;
                font-weight: 700;
            }
            QPushButton#record:pressed {
                background-color: #743120;
            }
            QToolButton#drawerToggle, QPushButton#smallButton {
                background-color: #2a1a12;
                border: 1px solid #4a3427;
                border-radius: 14px;
                padding: 8px 12px;
                color: #f1e5d3;
            }
            QCheckBox {
                spacing: 10px;
                font-size: 15px;
            }
            QCheckBox::indicator {
                width: 42px;
                height: 24px;
                border-radius: 12px;
                background: #47332f;
                border: 1px solid #6a4d40;
            }
            QCheckBox::indicator:checked {
                background: #617f43;
                border: 1px solid #8ba761;
            }
            QCheckBox::indicator:unchecked {
                background: #4b3532;
                border: 1px solid #6a4d40;
            }
            QSlider::groove:horizontal {
                border: 1px solid #5a4132;
                height: 8px;
                border-radius: 4px;
                background: #261912;
            }
            QSlider::handle:horizontal {
                background: #d39f53;
                border: 1px solid #f0c97a;
                width: 18px;
                margin: -6px 0;
                border-radius: 9px;
            }
            QPlainTextEdit {
                background-color: transparent;
                border: none;
                color: #d6c8b7;
                selection-background-color: #5f4635;
                font-size: 13px;
            }
        )");

        auto *layout = new QHBoxLayout(root);
        layout->setContentsMargins(18, 18, 18, 18);
        layout->setSpacing(16);

        auto *left = new QWidget(root);
        auto *leftLayout = new QVBoxLayout(left);
        leftLayout->setContentsMargins(0, 0, 0, 0);
        leftLayout->setSpacing(14);
        layout->addWidget(left, 1);

        auto *header = new QFrame(left);
        header->setObjectName("header");
        auto *headerLayout = new QHBoxLayout(header);
        headerLayout->setContentsMargins(18, 10, 18, 10);
        headerLayout->setSpacing(14);
        leftLayout->addWidget(header);

        auto *title = new QLabel("usit", header);
        title->setObjectName("title");
        headerLayout->addWidget(title);

        listeningToggle_ = new QCheckBox("Listening", header);
        injectionToggle_ = new QCheckBox("Injecting", header);
        waterfallToggle_ = new QCheckBox("Waterfall", header);
        for (auto *toggle : {listeningToggle_, injectionToggle_, waterfallToggle_}) {
            headerLayout->addWidget(toggle);
        }

        headerLayout->addStretch(1);

        auto *record = new QPushButton("Stop", header);
        record->setObjectName("record");
        connect(record, &QPushButton::clicked, this, [this] {
            sendCommand("toggle_pause");
        });
        headerLayout->addWidget(record);

        drawerToggle_ = new QToolButton(header);
        drawerToggle_->setObjectName("drawerToggle");
        drawerToggle_->setText("Control");
        drawerToggle_->setCheckable(true);
        drawerToggle_->setChecked(true);
        connect(drawerToggle_, &QToolButton::toggled, this, [this](bool open) {
            drawer_->setVisible(open);
        });
        headerLayout->addWidget(drawerToggle_);

        auto *shell = new QFrame(left);
        shell->setObjectName("shell");
        auto *shellLayout = new QVBoxLayout(shell);
        shellLayout->setContentsMargins(16, 16, 16, 16);
        shellLayout->setSpacing(14);
        leftLayout->addWidget(shell, 1);

        summaryLabel_ = new QLabel("display · src default · no model", shell);
        summaryLabel_->setObjectName("summary");
        shellLayout->addWidget(summaryLabel_);

        spectrogram_ = new SpectrogramWidget(shell);
        shellLayout->addWidget(spectrogram_, 1);

        auto *transcriptCard = new QFrame(shell);
        transcriptCard->setObjectName("transcriptCard");
        auto *transcriptLayout = new QVBoxLayout(transcriptCard);
        transcriptLayout->setContentsMargins(16, 12, 16, 12);
        transcriptLayout->setSpacing(6);
        shellLayout->addWidget(transcriptCard);

        transcriptLabel_ = new QLabel(transcriptCard);
        transcriptLabel_->setObjectName("transcript");
        transcriptLabel_->setWordWrap(true);
        transcriptLayout->addWidget(transcriptLabel_);

        partialLabel_ = new QLabel(transcriptCard);
        partialLabel_->setObjectName("partial");
        partialLabel_->setWordWrap(true);
        transcriptLayout->addWidget(partialLabel_);

        drawer_ = new QFrame(root);
        drawer_->setObjectName("drawer");
        drawer_->setFixedWidth(280);
        auto *drawerLayout = new QVBoxLayout(drawer_);
        drawerLayout->setContentsMargins(18, 16, 18, 16);
        drawerLayout->setSpacing(12);
        layout->addWidget(drawer_);

        auto *drawerTitleRow = new QHBoxLayout();
        auto *drawerTitle = new QLabel("Control Panel", drawer_);
        drawerTitle->setObjectName("title");
        drawerTitle->setStyleSheet("font-size: 18px;");
        drawerTitleRow->addWidget(drawerTitle);
        drawerTitleRow->addStretch(1);
        auto *closeButton = new QPushButton("Close", drawer_);
        closeButton->setObjectName("smallButton");
        connect(closeButton, &QPushButton::clicked, this, [this] {
            drawerToggle_->setChecked(false);
        });
        drawerTitleRow->addWidget(closeButton);
        drawerLayout->addLayout(drawerTitleRow);

        pausedToggle_ = new QCheckBox("Paused", drawer_);
        agcToggle_ = new QCheckBox("Auto gain", drawer_);
        drawerListeningToggle_ = new QCheckBox("Listening", drawer_);
        drawerInjectionToggle_ = new QCheckBox("Injection", drawer_);
        drawerWaterfallToggle_ = new QCheckBox("Waterfall", drawer_);
        autoSaveToggle_ = new QCheckBox("Auto-save", drawer_);

        sourceButton_ = new QPushButton("Source: Default", drawer_);
        sourceButton_->setObjectName("smallButton");
        modelButton_ = new QPushButton("Model: none", drawer_);
        modelButton_->setObjectName("smallButton");

        gainSlider_ = new QSlider(Qt::Horizontal, drawer_);
        gainSlider_->setRange(50, 200);
        gainSlider_->setValue(100);

        drawerLayout->addWidget(pausedToggle_);
        drawerLayout->addWidget(drawerListeningToggle_);
        drawerLayout->addWidget(drawerInjectionToggle_);
        drawerLayout->addWidget(drawerWaterfallToggle_);
        drawerLayout->addWidget(agcToggle_);
        drawerLayout->addWidget(autoSaveToggle_);
        drawerLayout->addWidget(sourceButton_);
        drawerLayout->addWidget(modelButton_);
        drawerLayout->addWidget(new QLabel("Software gain", drawer_));
        drawerLayout->addWidget(gainSlider_);

        helpBox_ = new QPlainTextEdit(drawer_);
        helpBox_->setReadOnly(true);
        helpBox_->setPlainText("Trusted helper shell. Rust remains the source of truth; this window owns layout and interaction.");
        helpBox_->setFixedHeight(112);
        drawerLayout->addWidget(helpBox_, 1);

        auto linkToggle = [this](QCheckBox *box, const QString &command) {
            connect(box, &QCheckBox::toggled, this, [this, command](bool) {
                if (updating_) {
                    return;
                }
                sendCommand(command);
            });
        };

        linkToggle(listeningToggle_, "toggle_pause");
        linkToggle(drawerListeningToggle_, "toggle_pause");
        linkToggle(injectionToggle_, "toggle_injection");
        linkToggle(drawerInjectionToggle_, "toggle_injection");
        linkToggle(waterfallToggle_, "toggle_viz");
        linkToggle(drawerWaterfallToggle_, "toggle_viz");
        linkToggle(pausedToggle_, "toggle_pause");
        linkToggle(agcToggle_, "toggle_agc");
        linkToggle(autoSaveToggle_, "toggle_auto_save");

        connect(sourceButton_, &QPushButton::clicked, this, [this] { sendCommand("cycle_device"); });
        connect(modelButton_, &QPushButton::clicked, this, [this] { sendCommand("cycle_model"); });
        connect(gainSlider_, &QSlider::sliderReleased, this, [this] {
            if (updating_) {
                return;
            }
            sendCommand("set_gain", QJsonObject{{"value", gainSlider_->value() / 100.0}});
        });

        if (!stdin_.open(stdin, QIODevice::ReadOnly | QIODevice::Text)) {
            qFatal("failed to open stdin for Qt overlay bridge");
        }
        notifier_ = new QSocketNotifier(fileno(stdin), QSocketNotifier::Read, this);
        connect(notifier_, &QSocketNotifier::activated, this, [this] { consumeInput(); });

        transcriptLabel_->setText("Waiting for transcript…");
        partialLabel_->clear();
    }

private:
    void sendCommand(const QString &type, QJsonObject extra = {}) {
        extra.insert("type", type);
        const auto payload = QJsonDocument(extra).toJson(QJsonDocument::Compact);
        QTextStream out(stdout);
        out << payload << '\n';
        out.flush();
    }

    void consumeInput() {
        buffer_.append(stdin_.readAll());
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
            applySnapshot(doc.object());
        }
    }

    void applySnapshot(const QJsonObject &snapshot) {
        updating_ = true;

        const bool paused = snapshot.value("paused").toBool(false);
        const bool injectionEnabled = snapshot.value("injection_enabled").toBool(false);
        const bool waterfall = snapshot.value("viz_mode").toString() == "waterfall";
        const bool agc = snapshot.value("auto_gain_enabled").toBool(false);
        const bool autoSave = snapshot.value("auto_save").toBool(true);
        const qreal gain = snapshot.value("gain").toDouble(1.0);

        listeningToggle_->setChecked(!paused);
        drawerListeningToggle_->setChecked(!paused);
        pausedToggle_->setChecked(paused);
        injectionToggle_->setChecked(injectionEnabled);
        drawerInjectionToggle_->setChecked(injectionEnabled);
        waterfallToggle_->setChecked(waterfall);
        drawerWaterfallToggle_->setChecked(waterfall);
        agcToggle_->setChecked(agc);
        autoSaveToggle_->setChecked(autoSave);
        gainSlider_->setValue(static_cast<int>(gain * 100.0));

        summaryLabel_->setText(snapshot.value("helper_summary").toString("display · src default · no model"));
        sourceButton_->setText(snapshot.value("source_label").toString("Source: Default"));
        modelButton_->setText(snapshot.value("model_label").toString("Model: none"));
        transcriptLabel_->setText(snapshot.value("committed").toString());
        partialLabel_->setText(snapshot.value("partial").toString());

        QString help = snapshot.value("helper_mode").toString();
        const auto error = snapshot.value("error").toString();
        if (!error.isEmpty()) {
            help += "\n\nERROR: " + error;
        }
        helpBox_->setPlainText(help);

        spectrogram_->applySnapshot(snapshot);
        updating_ = false;
    }

    QFile stdin_;
    QByteArray buffer_;
    QSocketNotifier *notifier_ = nullptr;
    bool updating_ = false;

    SpectrogramWidget *spectrogram_ = nullptr;
    QLabel *summaryLabel_ = nullptr;
    QLabel *transcriptLabel_ = nullptr;
    QLabel *partialLabel_ = nullptr;
    QFrame *drawer_ = nullptr;
    QToolButton *drawerToggle_ = nullptr;
    QCheckBox *listeningToggle_ = nullptr;
    QCheckBox *injectionToggle_ = nullptr;
    QCheckBox *waterfallToggle_ = nullptr;
    QCheckBox *pausedToggle_ = nullptr;
    QCheckBox *drawerListeningToggle_ = nullptr;
    QCheckBox *drawerInjectionToggle_ = nullptr;
    QCheckBox *drawerWaterfallToggle_ = nullptr;
    QCheckBox *agcToggle_ = nullptr;
    QCheckBox *autoSaveToggle_ = nullptr;
    QPushButton *sourceButton_ = nullptr;
    QPushButton *modelButton_ = nullptr;
    QSlider *gainSlider_ = nullptr;
    QPlainTextEdit *helpBox_ = nullptr;
};

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
    OverlayWindow window;
    window.show();
    return app.exec();
}
