#include "bridge.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QRectF>
#include <QtCore/QTimer>
#include <QtGui/QColor>
#include <QtGui/QFont>
#include <QtGui/QKeyEvent>
#include <QtGui/QLinearGradient>
#include <QtOpenGL/QOpenGLPaintDevice>
#include <QtGui/QPainter>
#include <QtGui/QOpenGLFunctions>
#include <QtOpenGLWidgets/QOpenGLWidget>
#include <QtWidgets/QApplication>
#include <QtWidgets/QVBoxLayout>
#include <QtWidgets/QWidget>
#include <QShortcut>

#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstring>
#include <mutex>
#include <string>

namespace {

std::mutex g_state_mutex;
std::string g_status = "usit qt visualizer idle";
std::string g_committed;
std::string g_partial;
UsitQtFrameSnapshot g_frame = {};
std::atomic<bool> g_quit_requested{false};

class GlMeterWidget : public QOpenGLWidget, protected QOpenGLFunctions {
public:
    GlMeterWidget() {
        setMinimumHeight(240);
        setMinimumWidth(560);
    }

protected:
    void initializeGL() override {
        initializeOpenGLFunctions();
        glDisable(GL_DEPTH_TEST);
        glDisable(GL_CULL_FACE);
    }

    void paintGL() override {
        UsitQtFrameSnapshot frame = {};
        std::string status;
        std::string committed;
        std::string partial;
        UsitQtControlSnapshot controls = {};
        usit_qt_get_control_snapshot(&controls);
        {
            const std::lock_guard<std::mutex> guard(g_state_mutex);
            frame = g_frame;
            status = g_status;
            committed = g_committed;
            partial = g_partial;
        }

        glViewport(0, 0, width(), height());
        glClearColor(0.06f, 0.04f, 0.035f, 1.0f);
        glClear(GL_COLOR_BUFFER_BIT);

        QOpenGLPaintDevice device(size() * devicePixelRatioF());
        device.setDevicePixelRatio(devicePixelRatioF());

        QPainter painter(&device);
        painter.setRenderHint(QPainter::Antialiasing, true);
        painter.setRenderHint(QPainter::TextAntialiasing, true);

        painter.fillRect(rect(), QColor("#100d0a"));

        const QRectF shell = rect().adjusted(18, 18, -18, -18);
        painter.setPen(QPen(QColor("#4d392d"), 1.0));
        painter.setBrush(QColor("#18120e"));
        painter.drawRoundedRect(shell, 18.0, 18.0);

        const QRectF canvas = shell.adjusted(18, 18, -18, -86);
        const bool controls_open = controls.panel_open != 0;
        const QRectF chart = controls_open
            ? canvas.adjusted(0, 0, -240, 0)
            : canvas;
        const QRectF controls_panel(
            chart.right() + 16.0,
            canvas.top(),
            std::max<qreal>(0.0, canvas.right() - chart.right() - 16.0),
            canvas.height());
        painter.setPen(QPen(QColor("#6d5341"), 1.0));
        painter.setBrush(QColor("#120d09"));
        painter.drawRoundedRect(chart, 16.0, 16.0);
        if (controls_open && controls_panel.width() > 80.0) {
            painter.setPen(QPen(QColor("#745844"), 1.0));
            painter.setBrush(QColor("#17100c"));
            painter.drawRoundedRect(controls_panel, 16.0, 16.0);
        }

        const qreal baseline = chart.bottom() - 18.0;
        const qreal usable_height = chart.height() - 36.0;
        const qreal bar_gap = 2.0;
        const qreal bar_width = std::max<qreal>(
            2.0,
            (chart.width() - (USIT_QT_BIN_COUNT - 1) * bar_gap - 20.0) / USIT_QT_BIN_COUNT);
        for (size_t index = 0; index < USIT_QT_BIN_COUNT; ++index) {
            const qreal magnitude = std::clamp<qreal>(frame.bins[index], 0.0, 1.0);
            const qreal bar_height = std::max<qreal>(8.0, usable_height * magnitude);
            const qreal x = chart.left() + 10.0 + index * (bar_width + bar_gap);
            const QRectF bar(
                x,
                baseline - bar_height,
                std::max<qreal>(1.0, bar_width),
                bar_height);
            QLinearGradient fill(bar.topLeft(), bar.bottomLeft());
            fill.setColorAt(0.0, QColor("#f2d77a"));
            fill.setColorAt(0.45, QColor("#cb9342"));
            fill.setColorAt(1.0, QColor("#56332a"));
            painter.setPen(Qt::NoPen);
            painter.setBrush(fill);
            painter.drawRoundedRect(bar, 2.5, 2.5);
        }

        const QRectF meter(
            chart.left() + 14.0,
            shell.bottom() - 54.0,
            chart.width() - 28.0,
            18.0);
        painter.setPen(Qt::NoPen);
        painter.setBrush(QColor("#2d231c"));
        painter.drawRoundedRect(meter, 9.0, 9.0);

        const QRectF fill(
            meter.left() + 3.0,
            meter.top() + 3.0,
            std::max<qreal>(
                12.0,
                (meter.width() - 6.0) * std::clamp<qreal>(frame.level, 0.0, 1.0)),
            meter.height() - 6.0);
        QLinearGradient fill_gradient(fill.topLeft(), fill.topRight());
        fill_gradient.setColorAt(0.0, QColor("#8c553d"));
        fill_gradient.setColorAt(0.4, QColor("#d39a4c"));
        fill_gradient.setColorAt(1.0, QColor("#efe3a8"));
        painter.setBrush(fill_gradient);
        painter.drawRoundedRect(fill, 7.0, 7.0);

        const qreal peak_x = meter.left() + 3.0
            + (meter.width() - 6.0) * std::clamp<qreal>(frame.peak, 0.0, 1.0);
        painter.setPen(QPen(QColor("#fff4c6"), 2.0));
        painter.drawLine(
            QPointF(peak_x, meter.top() + 2.0),
            QPointF(peak_x, meter.bottom() - 2.0));

        painter.setPen(QColor("#f0e2cd"));
        QFont title_font = painter.font();
        title_font.setPointSize(18);
        title_font.setBold(true);
        painter.setFont(title_font);
        painter.drawText(
            QRectF(shell.left() + 18, shell.top() + 8, shell.width() - 36, 30),
            Qt::AlignLeft | Qt::AlignVCenter,
            "usit");

        painter.setPen(QColor("#c8b08d"));
        QFont body_font = painter.font();
        body_font.setPointSize(10);
        body_font.setBold(false);
        painter.setFont(body_font);
        painter.drawText(
            QRectF(shell.left() + 18, shell.bottom() - 58, shell.width() - 36, 18),
            Qt::AlignLeft | Qt::AlignVCenter,
            QString("heard: %1").arg(QString::fromUtf8(committed.c_str())));
        painter.setPen(QColor("#dcc3a4"));
        painter.drawText(
            QRectF(shell.left() + 18, shell.bottom() - 40, shell.width() - 36, 18),
            Qt::AlignLeft | Qt::AlignVCenter,
            QString("live: %1").arg(QString::fromUtf8(partial.c_str())));
        painter.setPen(QColor("#c8b08d"));
        painter.drawText(
            QRectF(shell.left() + 18, shell.bottom() - 22, shell.width() - 36, 18),
            Qt::AlignLeft | Qt::AlignVCenter,
            QString::fromUtf8(status.c_str()));

        if (controls_open && controls_panel.width() > 80.0) {
            drawControlPanel(painter, controls_panel, controls);
        }

        painter.end();
    }

    static void drawControlPanel(
        QPainter& painter,
        const QRectF& panel,
        const UsitQtControlSnapshot& controls) {
        painter.setPen(QColor("#f0e2cd"));
        QFont title_font = painter.font();
        title_font.setPointSize(13);
        title_font.setBold(true);
        painter.setFont(title_font);
        painter.drawText(
            QRectF(panel.left() + 16, panel.top() + 12, panel.width() - 32, 24),
            Qt::AlignLeft | Qt::AlignVCenter,
            "Control");

        QFont body_font = painter.font();
        body_font.setPointSize(10);
        body_font.setBold(false);
        painter.setFont(body_font);

        struct Row {
            const char* label;
            QString value;
        };
        const Row rows[] = {
            {"Capture pause", controls.paused ? "paused" : "listening"},
            {"Auto gain", controls.auto_gain_enabled ? "on" : "off"},
            {"Manual gain", QString::asprintf("%.1fx", controls.manual_gain)},
        };

        qreal row_y = panel.top() + 48.0;
        for (size_t index = 0; index < std::size(rows); ++index) {
            const bool selected = index == controls.selected_index;
            const QRectF row(panel.left() + 12.0, row_y, panel.width() - 24.0, 42.0);
            painter.setPen(QPen(selected ? QColor("#f2d77a") : QColor("#4d392d"), 1.0));
            painter.setBrush(selected ? QColor("#2a1e17") : QColor("#1b1410"));
            painter.drawRoundedRect(row, 11.0, 11.0);

            painter.setPen(selected ? QColor("#f7edd0") : QColor("#ccb08d"));
            painter.drawText(
                QRectF(row.left() + 12.0, row.top() + 5.0, row.width() - 24.0, 16.0),
                Qt::AlignLeft | Qt::AlignVCenter,
                rows[index].label);
            painter.drawText(
                QRectF(row.left() + 12.0, row.top() + 19.0, row.width() - 24.0, 18.0),
                Qt::AlignLeft | Qt::AlignVCenter,
                rows[index].value);

            row_y += 50.0;
        }

        painter.setPen(QColor("#a0886e"));
        const int current_gain_percent = static_cast<int>(std::round(controls.current_gain * 100.0));
        painter.drawText(
            QRectF(panel.left() + 16.0, panel.bottom() - 62.0, panel.width() - 32.0, 20.0),
            Qt::AlignLeft | Qt::AlignVCenter,
            QString::fromUtf8(controls.source_label));
        painter.drawText(
            QRectF(panel.left() + 16.0, panel.bottom() - 40.0, panel.width() - 32.0, 20.0),
            Qt::AlignLeft | Qt::AlignVCenter,
            QString("c controls · arrows move/adjust · enter toggles"));
        painter.drawText(
            QRectF(panel.left() + 16.0, panel.bottom() - 20.0, panel.width() - 32.0, 20.0),
            Qt::AlignLeft | Qt::AlignVCenter,
            QString("active gain %1%").arg(current_gain_percent));
    }
};

class ShellWidget : public QWidget {
public:
    ShellWidget() {
        setWindowTitle("usit");
        resize(780, 360);
        setMinimumSize(640, 300);
        setFocusPolicy(Qt::StrongFocus);

        auto* layout = new QVBoxLayout(this);
        layout->setContentsMargins(14, 14, 14, 14);
        layout->setSpacing(0);
        layout->addWidget(&meter_);

        auto* quit_shortcut = new QShortcut(QKeySequence(Qt::Key_Q), this);
        QObject::connect(quit_shortcut, &QShortcut::activated, this, []() {
            QCoreApplication::quit();
        });

        auto* esc_shortcut = new QShortcut(QKeySequence(Qt::Key_Escape), this);
        QObject::connect(esc_shortcut, &QShortcut::activated, this, []() {
            QCoreApplication::quit();
        });

        timer_.setInterval(33);
        QObject::connect(&timer_, &QTimer::timeout, this, [this]() {
            if (g_quit_requested.load()) {
                QCoreApplication::quit();
                return;
            }
            meter_.update();
        });
        timer_.start();
    }

private:
    void keyPressEvent(QKeyEvent* event) override {
        UsitQtControlSnapshot controls = {};
        usit_qt_get_control_snapshot(&controls);

        switch (event->key()) {
        case Qt::Key_Q:
            QCoreApplication::quit();
            return;
        case Qt::Key_Escape:
            if (controls.panel_open != 0) {
                usit_qt_toggle_controls();
                meter_.update();
            } else {
                QCoreApplication::quit();
            }
            return;
        case Qt::Key_C:
            usit_qt_toggle_controls();
            meter_.update();
            return;
        case Qt::Key_Up:
            if (controls.panel_open != 0) {
                usit_qt_focus_previous_control();
                meter_.update();
            }
            return;
        case Qt::Key_Down:
            if (controls.panel_open != 0) {
                usit_qt_focus_next_control();
                meter_.update();
            }
            return;
        case Qt::Key_Left:
            if (controls.panel_open != 0) {
                usit_qt_adjust_control(-1);
                meter_.update();
            }
            return;
        case Qt::Key_Right:
            if (controls.panel_open != 0) {
                usit_qt_adjust_control(1);
                meter_.update();
            }
            return;
        case Qt::Key_Return:
        case Qt::Key_Enter:
        case Qt::Key_Space:
            if (controls.panel_open != 0) {
                usit_qt_activate_control();
                meter_.update();
            }
            return;
        default:
            QWidget::keyPressEvent(event);
            return;
        }
    }

    GlMeterWidget meter_;
    QTimer timer_;
};

}  // namespace

extern "C" void usit_qt_set_status(const char* text) {
    const std::lock_guard<std::mutex> guard(g_state_mutex);
    g_status = text ? text : "";
}

extern "C" void usit_qt_set_transcript(const char* committed, const char* partial) {
    const std::lock_guard<std::mutex> guard(g_state_mutex);
    g_committed = committed ? committed : "";
    g_partial = partial ? partial : "";
}

extern "C" void usit_qt_publish_frame(const UsitQtFrameSnapshot* frame) {
    const std::lock_guard<std::mutex> guard(g_state_mutex);
    if (!frame) {
        std::memset(&g_frame, 0, sizeof(g_frame));
        return;
    }
    g_frame = *frame;
}

extern "C" void usit_qt_request_quit() {
    g_quit_requested.store(true);
}

extern "C" int usit_qt_run() {
    int argc = 1;
    char app_name[] = "usit";
    char* argv[] = {app_name, nullptr};

    QApplication app(argc, argv);
    ShellWidget widget;
    widget.show();
    return app.exec();
}
