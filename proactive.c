#include "proactive.h"
#include "ai.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_TRANSITIONS 2000
#define CMD_LEN 256
// Seen at least this many times before we suggest it, so one-offs don't nag.
#define MIN_CONFIDENCE 2

typedef struct {
  char prev[CMD_LEN];
  char next[CMD_LEN];
  int count;
} Transition;

static Transition transitions[MAX_TRANSITIONS];
static int transition_count = 0;

static void transitions_path(char *out, size_t size) {
  const char *home = getenv("HOME");
  snprintf(out, size, "%s/.config/aish/transitions.tsv", home ? home : ".");
}

// File format: one "count<TAB>prev<TAB>next" per line.
void load_transitions(void) {
  char path[512];
  transitions_path(path, sizeof(path));
  FILE *f = fopen(path, "r");
  if (!f)
    return;
  char line[CMD_LEN * 2 + 32];
  while (transition_count < MAX_TRANSITIONS && fgets(line, sizeof(line), f)) {
    line[strcspn(line, "\n")] = '\0';
    char *count = strtok(line, "\t");
    char *prev = strtok(NULL, "\t");
    char *next = strtok(NULL, "\t");
    if (!count || !prev || !next)
      continue;
    Transition *t = &transitions[transition_count++];
    t->count = atoi(count);
    strncpy(t->prev, prev, CMD_LEN - 1);
    t->prev[CMD_LEN - 1] = '\0';
    strncpy(t->next, next, CMD_LEN - 1);
    t->next[CMD_LEN - 1] = '\0';
  }
  fclose(f);
}

void save_transitions(void) {
  char path[512];
  transitions_path(path, sizeof(path));
  FILE *f = fopen(path, "w");
  if (!f)
    return;
  for (int i = 0; i < transition_count; i++)
    fprintf(f, "%d\t%s\t%s\n", transitions[i].count, transitions[i].prev,
            transitions[i].next);
  fclose(f);
}

static int worth_learning(const char *cmd) {
  size_t len = strlen(cmd);
  if (len == 0 || len >= CMD_LEN)
    return 0;
  if (strchr(cmd, '\t') || strchr(cmd, '\n'))
    return 0;
  if (strcmp(cmd, "exit") == 0 || strncmp(cmd, "mode", 4) == 0)
    return 0;
  return 1;
}

void record_transition(const char *prev, const char *next) {
  if (!worth_learning(prev) || !worth_learning(next) || strcmp(prev, next) == 0)
    return;

  int weakest = 0;
  for (int i = 0; i < transition_count; i++) {
    if (strcmp(transitions[i].prev, prev) == 0 &&
        strcmp(transitions[i].next, next) == 0) {
      transitions[i].count++;
      return;
    }
    if (transitions[i].count < transitions[weakest].count)
      weakest = i;
  }

  // Full: forget the weakest habit to make room.
  int slot = transition_count < MAX_TRANSITIONS ? transition_count++ : weakest;
  strncpy(transitions[slot].prev, prev, CMD_LEN - 1);
  transitions[slot].prev[CMD_LEN - 1] = '\0';
  strncpy(transitions[slot].next, next, CMD_LEN - 1);
  transitions[slot].next[CMD_LEN - 1] = '\0';
  transitions[slot].count = 1;
}

const char *predict_next(const char *prev) {
  const Transition *best = NULL;
  for (int i = 0; i < transition_count; i++) {
    if (strcmp(transitions[i].prev, prev) != 0)
      continue;
    if (!best || transitions[i].count > best->count)
      best = &transitions[i];
  }
  if (best && best->count >= MIN_CONFIDENCE)
    return best->next;
  return NULL;
}

void proactive_suggestion(const char *last_command, char *out,
                          size_t out_size) {
  out[0] = '\0';
  // A fix is only offered once — if the user ignores it, it goes away.
  if (ai_suggested_fix[0] != '\0') {
    strncpy(out, ai_suggested_fix, out_size - 1);
    out[out_size - 1] = '\0';
    ai_suggested_fix[0] = '\0';
    return;
  }
  if (last_command == NULL || last_command[0] == '\0')
    return;
  const char *next = predict_next(last_command);
  if (next) {
    strncpy(out, next, out_size - 1);
    out[out_size - 1] = '\0';
  }
}

// Programs you sit inside on purpose; finishing them isn't news.
static const char *interactive_programs[] = {
    "vim",  "nvim", "vi",     "nano", "emacs", "less",    "more",  "man",
    "top",  "htop", "btop",   "ssh",  "tmux",  "screen",  "watch", "lazygit",
    "fzf",  "gdb",  "python", "python3", "node", "irb",  "psql",  "sqlite3",
    "bash", "zsh",  "sh",     "fish", "tail",  "journalctl", "ranger", NULL};

int should_announce(const char *command, double seconds) {
  const char *setting = getenv("AISH_NOTIFY_SECS");
  double threshold = setting ? atof(setting) : 15.0;
  if (threshold <= 0 || seconds < threshold)
    return 0;

  char program[CMD_LEN];
  size_t len = strcspn(command, " ");
  if (len == 0 || len >= sizeof(program))
    return 0;
  memcpy(program, command, len);
  program[len] = '\0';
  const char *base = strrchr(program, '/');
  base = base ? base + 1 : program;
  for (int i = 0; interactive_programs[i] != NULL; i++)
    if (strcmp(base, interactive_programs[i]) == 0)
      return 0;
  return 1;
}
