#ifndef AI_H
#define AI_H

#include <stddef.h>

void json_escape(const char *src, char *dest, size_t dest_size);
void send_telemetry_to_rust(const char *json_payload);
char *ask_nlp(char *input);
void ask_chat(const char *message);
void notify_rust_vram_state(const char *event_type);
void ask_ai(const char *command, const char *error);
void extract_fix_command(const char *reply, const char *failed_command,
                         char *out, size_t out_size);
void notify_command_finished(const char *command, int exit_code,
                             double seconds);

// The command the last auto-fix suggested, offered as ghost text at the next
// prompt. Empty when there's nothing to offer.
extern char ai_suggested_fix[1024];

#endif
