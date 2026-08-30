#ifndef AI_H
#define AI_H

#include <stddef.h>

void json_escape(const char *src, char *dest, size_t dest_size);
void send_telemetry_to_rust(const char *json_payload);
char *ask_nlp(char *input);
void ask_chat(const char *message);
void notify_rust_vram_state(const char *event_type);
// Change this line in ai.h:
void ask_ai(const char *command,
            const char *error); // <-- Make sure this is here!

#endif
