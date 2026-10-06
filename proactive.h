#ifndef PROACTIVE_H
#define PROACTIVE_H

#include <stddef.h>

// Learns "after X you usually run Y" from your own history and offers Y as
// ghost text at an empty prompt.
void load_transitions(void);
void save_transitions(void);
void record_transition(const char *prev, const char *next);
const char *predict_next(const char *prev);

// What to show at an empty prompt: a pending auto-fix first, otherwise the
// learned next command. Writes "" when there's nothing worth offering.
void proactive_suggestion(const char *last_command, char *out, size_t out_size);

// Should finishing this command after `seconds` be announced out loud?
int should_announce(const char *command, double seconds);

#endif
