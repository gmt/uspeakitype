#include "bridge.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QRectF>
#include <QtCore/QTimer>
#include <QtGui/QColor>
#include <QtGui/QFont>
#include <QtGui/QLinearGradient>
#include <QtOpenGL/QOpenGLPaintDevice>
#include <QtGui/QPainter>
#include <QtGui/QOpenGLFunctions>
#include <QtOpenGLWidgets/QOpenGLWidget>
#include <QtWidgets/QApplication>
#include <QtWidgets/QHBoxLayout>
#include <QtWidgets/QVBoxLayout>
#include <QtWidgets/QWidget>

#include <algorithm>
#include <atomic>
#include <cmath>
#include <mutex>
#include <string>
#include <vector>

namespace {

std::mutex g_state_mutex;
float g_level = 0.0f;
std::string g_status = "QOpenGLPaintDevice idle";
std::vector<float> g_bins(96, 0.0f);
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
        std::vector<float> bins;
        std::string status;
        float level = 0.0f;
        {
            const std::lock_guard<std::mutex> guard(g_state_mutex);
            bins = g_bins;
            status = g_status;
            level = std::clamp(g_level, 0.0f, 1.0f);
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
        painter.setPen(QPen(QColor("#6d5341"), 1.0));
        painter.setBrush(QColor("#120d09"));
        painter.drawRoundedRect(canvas, 16.0, 16.0);

        const qreal baseline = canvas.bottom() - 18.0;
        const qreal usable_height = canvas.height() - 36.0;
        const qreal bar_gap = 2.0;
        const qreal bar_width =
            std::max<qreal>(2.0, (canvas.width() - (bins.size() - 1) * bar_gap) / bins.size());
        for (size_t index = 0; index < bins.size(); ++index) {
            const qreal magnitude = std::clamp<qreal>(bins[index], 0.0, 1.0);
            const qreal bar_height = std::max<qreal>(8.0, usable_height * magnitude);
            const qreal x =
                canvas.left() + 10.0 + index * (bar_width + bar_gap);
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
            canvas.left() + 14.0,
            shell.bottom() - 54.0,
            shell.width() - 28.0,
            18.0);
        painter.setPen(Qt::NoPen);
        painter.setBrush(QColor("#2d231c"));
        painter.drawRoundedRect(meter, 9.0, 9.0);

        const QRectF fill(
            meter.left() + 3.0,
            meter.top() + 3.0,
            std::max<qreal>(12.0, (meter.width() - 6.0) * level),
            meter.height() - 6.0);
        QLinearGradient fill_gradient(fill.topLeft(), fill.topRight());
        fill_gradient.setColorAt(0.0, QColor("#8c553d"));
        fill_gradient.setColorAt(0.4, QColor("#d39a4c"));
        fill_gradient.setColorAt(1.0, QColor("#efe3a8"));
        painter.setBrush(fill_gradient);
        painter.drawRoundedRect(fill, 7.0, 7.0);

        painter.setPen(QColor("#f0e2cd"));
        QFont title_font = painter.font();
        title_font.setPointSize(18);
        title_font.setBold(true);
        painter.setFont(title_font);
        painter.drawText(
            QRectF(shell.left() + 18, shell.top() + 8, shell.width() - 36, 30),
            Qt::AlignLeft | Qt::AlignVCenter,
            "nuxglit");

        painter.setPen(QColor("#c8b08d"));
        QFont body_font = painter.font();
        body_font.setPointSize(10);
        body_font.setBold(false);
        painter.setFont(body_font);
        painter.drawText(
            QRectF(shell.left() + 18, shell.bottom() - 34, shell.width() - 36, 22),
            Qt::AlignLeft | Qt::AlignVCenter,
            QString::fromUtf8(status.c_str()));

        painter.end();
    }
};

class ShellWidget : public QWidget {
public:
    ShellWidget() {
        setWindowTitle("nuxglit");
        resize(780, 360);
        setMinimumSize(640, 300);

        auto* layout = new QVBoxLayout(this);
        layout->setContentsMargins(14, 14, 14, 14);
        layout->setSpacing(0);
        layout->addWidget(&meter_);

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
    GlMeterWidget meter_;
    QTimer timer_;
};

}  // namespace

extern "C" void nuxglit_set_level(float level) {
    const std::lock_guard<std::mutex> guard(g_state_mutex);
    g_level = level;
}

extern "C" void nuxglit_set_status(const char* text) {
    const std::lock_guard<std::mutex> guard(g_state_mutex);
    g_status = text ? text : "";
}

extern "C" void nuxglit_set_bins(const float* bins, size_t len) {
    const std::lock_guard<std::mutex> guard(g_state_mutex);
    g_bins.assign(bins, bins + len);
}

extern "C" void nuxglit_request_quit() {
    g_quit_requested.store(true);
}

extern "C" int nuxglit_run() {
    int argc = 1;
    char app_name[] = "nuxglit";
    char* argv[] = {app_name, nullptr};

    QApplication app(argc, argv);
    ShellWidget widget;
    widget.show();
    return app.exec();
}
