#pragma once

#include <stddef.h>

extern "C" {
int nuxglit_run();
void nuxglit_set_level(float level);
void nuxglit_set_status(const char* text);
void nuxglit_set_bins(const float* bins, size_t len);
void nuxglit_request_quit();
}
