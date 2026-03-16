#pragma once

#include <stddef.h>

#define USIT_QT_BIN_COUNT 96

struct UsitQtFrameSnapshot {
    float level;
    float peak;
    float bins[USIT_QT_BIN_COUNT];
};

extern "C" {
int usit_qt_run();
void usit_qt_set_status(const char* text);
void usit_qt_publish_frame(const UsitQtFrameSnapshot* frame);
void usit_qt_request_quit();
}
