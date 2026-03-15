#pragma once

extern "C" {
int hellnuxit_run();
void hellnuxit_set_level(float level);
void hellnuxit_set_status(const char* text);
void hellnuxit_request_quit();
}
