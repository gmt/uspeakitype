#include "bridge.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QRectF>
#include <QtCore/QTimer>
#include <QtGui/QColor>
#include <QtGui/QFont>
#include <QtGui/QPainter>
#include <QtWidgets/QApplication>
#include <QtWidgets/QWidget>

#include <algorithm>
#include <atomic>
#include <cmath>
#include <mutex>
#include <string>

namespace {

std::mutex g_state_mutex;
float g_level = 0.0f;
std::string g_status = "manual C++ bridge idle";
std::atomic<bool> g_quit_requested{false};

class MeterWidget : public QWidget {
public:
    MeterWidget() {
        setWindowTitle("hellnuxit");
        resize(640, 240);
        setMinimumSize(480, 200);

        timer_.setInterval(33);
        QObject::connect(&timer_, &QTimer::timeout, this, [this]() {
            if (g_quit_requested.load()) {
                QCoreApplication::quit();
                return;
            }
            update();
        });
        timer_.start();
    }

protected:
    void paintEvent(QPaintEvent*) override {
        std::string status;
        float level = 0.0f;
        {
            const std::lock_guard<std::mutex> guard(g_state_mutex);
            level = std::clamp(g_level, 0.0f, 1.0f);
            status = g_status;
        }

        QPainter painter(this);
        painter.setRenderHint(QPainter::Antialiasing, true);
        painter.fillRect(rect(), QColor("#100d0a"));

        const QRectF shell = rect().adjusted(18, 18, -18, -18);
        painter.setBrush(QColor("#1c1510"));
        painter.setPen(QPen(QColor("#4f3a2b"), 1.0));
        painter.drawRoundedRect(shell, 18.0, 18.0);

        painter.setPen(QColor("#f0e1ce"));
        QFont title_font = painter.font();
        title_font.setPointSize(18);
        title_font.setBold(true);
        painter.setFont(title_font);
        painter.drawText(QRectF(shell.left() + 20, shell.top() + 18, 220, 32), "hellnuxit");

        const QRectF meter_frame(shell.left() + 20, shell.top() + 64, shell.width() - 40, 90);
        painter.setBrush(QColor("#261c14"));
        painter.setPen(QPen(QColor("#5e4634"), 1.0));
        painter.drawRoundedRect(meter_frame, 14.0, 14.0);

        const qreal usable_width = meter_frame.width() - 24.0;
        const qreal fill_width = std::max<qreal>(16.0, usable_width * level);
        const QRectF meter_fill(
            meter_frame.left() + 12.0,
            meter_frame.top() + 12.0,
            fill_width,
            meter_frame.height() - 24.0);
        painter.setPen(Qt::NoPen);
        painter.setBrush(QColor("#d2984f"));
        painter.drawRoundedRect(meter_fill, 10.0, 10.0);

        painter.setPen(QColor("#e7d8c4"));
        QFont status_font = painter.font();
        status_font.setPointSize(11);
        status_font.setBold(false);
        painter.setFont(status_font);
        painter.drawText(
            QRectF(shell.left() + 20, shell.top() + 172, shell.width() - 40, 40),
            Qt::TextWordWrap,
            QString::fromUtf8(status.c_str()));
    }

private:
    QTimer timer_;
};

}  // namespace

extern "C" void hellnuxit_set_level(float level) {
    const std::lock_guard<std::mutex> guard(g_state_mutex);
    g_level = level;
}

extern "C" void hellnuxit_set_status(const char* text) {
    const std::lock_guard<std::mutex> guard(g_state_mutex);
    g_status = text ? text : "";
}

extern "C" void hellnuxit_request_quit() {
    g_quit_requested.store(true);
}

extern "C" int hellnuxit_run() {
    int argc = 1;
    char app_name[] = "hellnuxit";
    char* argv[] = {app_name, nullptr};

    QApplication app(argc, argv);
    MeterWidget widget;
    widget.show();
    return app.exec();
}
