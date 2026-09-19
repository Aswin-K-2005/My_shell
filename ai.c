#include "ai.h"
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

// Helper: Escape special JSON characters
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
  memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, "/tmp/aish_chat.sock", sizeof(addr.sun_path) - 1);
  if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) == 0) {
    char payload[256];
    snprintf(payload, sizeof(payload),
             "{\"event\": \"%s\", \"pid\": %d}__MSG_END__", event_type,
             getpid());
    write(sock, payload, strlen(payload));
  }
  close(sock);
}

void send_telemetry_to_rust(const char *json_payload) {
  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0)
    return;
  struct sockaddr_un addr;
  memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, "/tmp/aish.sock", sizeof(addr.sun_path) - 1);
  if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) == 0) {
    write(sock, json_payload, strlen(json_payload));
  }
  close(sock);
}

void ask_chat(const char *initial_prompt) {
  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0) {
    printf("\n[Error] Failed to create socket\n");
    return;
  }

  struct sockaddr_un addr;
  memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, "/tmp/aish_chat.sock", sizeof(addr.sun_path) - 1);

  if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
    printf("\n[Error] Could not connect to daemon (/tmp/aish_chat.sock)\n");
    close(sock);
    return;
  }

  char payload[16384];
  snprintf(payload, sizeof(payload), "%s__MSG_END__", initial_prompt);
  write(sock, payload, strlen(payload));

  // High-smoothness rotating Braille spinner frames
  const char *frames[] = {"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"};
  const int num_frames = 10;

  struct pollfd pfd;
  pfd.fd = sock;
  pfd.events = POLLIN;

  int frame = 0;
  int cleared_thinking = 0;
  int is_speaking = 0;
  char chunk[512];

  while (1) {
    // 60ms animation tick rate for buttery smooth rotation
    int ret = poll(&pfd, 1, 60);

    if (ret == 0) {
      // Timeout: render animation tick while waiting for data
      if (!cleared_thinking) {
        if (is_speaking) {
          // Neon Purple Rotating Spinner
          printf("\r\033[2K\033[38;2;180;100;255m%s\033[0m "
                 "\033[90mSpeaking...\033[0m",
                 frames[frame % num_frames]);
        } else {
          // Cyan Rotating Spinner
          printf("\r\033[2K\033[36m%s\033[0m \033[90mThinking...\033[0m",
                 frames[frame % num_frames]);
        }
        fflush(stdout);
        frame++;
      }
    } else if (ret > 0) {
      ssize_t bytes_read = read(sock, chunk, sizeof(chunk) - 1);
      if (bytes_read <= 0)
        break;
      chunk[bytes_read] = '\0';

      // Intercept "__SPEAKING__" and remove it cleanly without cutting off
      // adjacent text
      char *speak_pos = strstr(chunk, "__SPEAKING__");
      if (speak_pos) {
        is_speaking = 1;
        size_t rest_len = strlen(speak_pos + 12);
        memmove(speak_pos, speak_pos + 12, rest_len + 1);
      }

      // Check for stream termination tag
      char *end_pos = strstr(chunk, "__END__");
      if (end_pos) {
        *end_pos = '\0';
      }

      // Render payload text once available
      if (strlen(chunk) > 0) {
        if (!cleared_thinking) {
          printf("\r\033[2K"); // Erase spinner line completely
          fflush(stdout);
          cleared_thinking = 1;
        }
        printf("%s", chunk);
        fflush(stdout);
      }

      if (end_pos)
        break;
    } else {
      break;
    }
  }

  close(sock);
  printf("\n");
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
  char message[8192];
  snprintf(message, sizeof(message),
           "The user typed the command: '%s'\n"
           "The shell returned the following error:\n%s\n\n"
           "Explain why this command failed and provide the exact command to "
           "fix it.",
           command, error);

  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0)
    return;

  struct sockaddr_un addr;
  memset(&addr, 0, sizeof(addr));
  addr.sun_family = AF_UNIX;
  strncpy(addr.sun_path, "/tmp/aish_chat.sock", sizeof(addr.sun_path) - 1);

  if (connect(sock, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
    close(sock);
    return;
  }

  write(sock, message, strlen(message));
  write(sock, "__MSG_END__", 11);

  char buf[512];
  int n;
  printf("\033[33m[aish AI Auto-Fix]: \033[0m\n");
  fflush(stdout);

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
