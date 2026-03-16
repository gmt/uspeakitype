#pragma once

#include <stddef.h>

#define USIT_QT_BIN_COUNT 96

struct UsitQtFrameSnapshot {
    float level;
    float peak;
    float bins[USIT_QT_BIN_COUNT];
};

struct UsitQtControlSnapshot {
    unsigned char panel_open;
    unsigned int selected_index;
    unsigned char paused;
    unsigned char auto_gain_enabled;
    float manual_gain;
    float current_gain;
    char source_label[128];
};

extern "C" {
int usit_qt_run();
void usit_qt_set_status(const char* text);
void usit_qt_publish_frame(const UsitQtFrameSnapshot* frame);
void usit_qt_request_quit();
void usit_qt_toggle_controls();
void usit_qt_focus_next_control();
void usit_qt_focus_previous_control();
void usit_qt_activate_control();
void usit_qt_adjust_control(int direction);
void usit_qt_get_control_snapshot(UsitQtControlSnapshot* out);
}
