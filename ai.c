#include "ai.h"
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

// Helper: Escape special JSON characters (" and \) to avoid breaking Rust
// serde_json
void json_escape(const char *src, char *dest, size_t dest_size) {
  size_t j = 0;
  for (size_t i = 0; src[i] != '\0' && j < dest_size - 2; i++) {
    if (src[i] == '"') {
      dest[j++] = '\\';
      dest[j++] = '"';
    } else if (src[i] == '\\') {
      dest[j++] = '\\';
      dest[j++] = '\\';
    } else if (src[i] == '\n') {
      dest[j++] = '\\';
      dest[j++] = 'n';
    } else {
      dest[j++] = src[i];
    }
  }
  dest[j] = '\0';
}
void notify_rust_vram_state(const char *event_type) {
  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0)
    return;

  struct sockaddr_un addr;
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, "/tmp/aish_chat.sock", sizeof(addr.sun_path) - 1);

  if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) == 0) {
    char payload[256];
    // Send the JSON trigger with our Process ID and the required __MSG_END__
    // delimiter
    snprintf(payload, sizeof(payload),
             "{\"event\": \"%s\", \"pid\": %d}__MSG_END__", event_type,
             getpid());
    write(sock, payload, strlen(payload));
  }
  close(sock);
}

// Non-blocking telemetry dispatcher to Rust
void send_telemetry_to_rust(const char *json_payload) {
  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0)
    return; // Fail silently if socket creation fails

  struct sockaddr_un addr;
  memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, "/tmp/aish.sock", sizeof(addr.sun_path) - 1);

  // Connect to the Rust daemon (/tmp/aish.sock)
  if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) == 0) {
    write(sock, json_payload, strlen(json_payload));
  }

  // Close connection so Rust's `read_to_end` gets an EOF signal
  close(sock);
}

void ask_chat(char *input) {
  char message[20480];
  strncpy(message, input, sizeof(message) - 1);

  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0) {
    perror("socket");
    return;
  }

  struct sockaddr_un addr;
  memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, "/tmp/aish_chat.sock", sizeof(addr.sun_path) - 1);

  if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
    printf("\033[31m[Error] Could not connect to Aish AI Orchestrator daemon "
           "(/tmp/aish_chat.sock). Is it running?\033[0m\n");
    close(sock);
    return;
  }

  // Send request payload to Rust
  write(sock, message, strlen(message));
  write(sock, "__MSG_END__", 11);

  char buf[512];
  int n;
  printf("\033[33mAish: \033[0m");
  fflush(stdout);

  // Stream tokens live from Rust GPU daemon
  while ((n = recv(sock, buf, sizeof(buf) - 1, 0)) > 0) {
    buf[n] = '\0';

    // Check for end marker
    char *end_pos = strstr(buf, "__END__");
    if (end_pos != NULL) {
      *end_pos = '\0'; // Truncate out the marker
      printf("%s", buf);
      fflush(stdout);
      break;
    }

    printf("%s", buf);
    fflush(stdout);
  }
  printf("\n");

  close(sock);
}

char *ask_nlp(char *input) {
  char cwd[2048];
  getcwd(cwd, sizeof(cwd));
  char message[4096];
  snprintf(message, sizeof(message), "nlp:[context: currently in %s] %s", cwd,
           input);

  int fin = open("/tmp/aish_in", O_WRONLY | O_NONBLOCK);
  if (fin < 0)
    return NULL;
  write(fin, message, strlen(message));
  close(fin);

  int fout = open("/tmp/aish_out", O_RDONLY);
  if (fout < 0)
    return NULL;
  char *response = malloc(4096);
  int n = read(fout, response, 4095);
  if (n > 0)
    response[n] = '\0';
  close(fout);
  return response;
}

void ask_ai(const char *command, const char *error) {
  // Clear context showing BOTH what was typed and what failed
  char message[8192];
  snprintf(message, sizeof(message),
           "The user typed the command: '%s'\n"
           "The shell returned the following error:\n%s\n\n"
           "Explain why this command failed and provide the exact command to "
           "fix it.",
           command, error);

  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0) {
    return;
  }

  struct sockaddr_un addr;
  memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, "/tmp/aish_chat.sock", sizeof(addr.sun_path) - 1);

  if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
    close(sock);
    return;
  }

  // Send request payload to Rust
  write(sock, message, strlen(message));
  write(sock, "__MSG_END__", 11);

  char buf[512];
  int n;
  printf("\033[33m[aish AI Auto-Fix]: \033[0m\n");
  fflush(stdout);

  // Stream tokens live from the Rust GPU daemon
  while ((n = recv(sock, buf, sizeof(buf) - 1, 0)) > 0) {
    buf[n] = '\0';

    char *end_pos = strstr(buf, "__END__");
    if (end_pos != NULL) {
      *end_pos = '\0';
      printf("%s", buf);
      fflush(stdout);
      break;
    }

    printf("%s", buf);
    fflush(stdout);
  }
  printf("\n");

  close(sock);
}
