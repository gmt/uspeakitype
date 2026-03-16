#pragma once

#include <stddef.h>

#define NUXGLIT_BIN_COUNT 96

struct NuxglitFrameSnapshot {
    float level;
    float peak;
    float bins[NUXGLIT_BIN_COUNT];
};

extern "C" {
int nuxglit_run();
void nuxglit_set_status(const char* text);
void nuxglit_publish_frame(const NuxglitFrameSnapshot* frame);
void nuxglit_request_quit();
}
