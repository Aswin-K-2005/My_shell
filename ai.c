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

static int execute_tool_command(const char *cmd, char *output_buf,
                                size_t max_len) {
  char full_cmd[1024];
  // Redirect stderr to stdout so the AI gets error context if a tool fails
  snprintf(full_cmd, sizeof(full_cmd), "%s 2>&1", cmd);

  FILE *fp = popen(full_cmd, "r");
  if (!fp) {
    snprintf(output_buf, max_len, "Error: Failed to execute tool command.");
    return -1;
  }

  size_t bytes_read = fread(output_buf, 1, max_len - 1, fp);
  output_buf[bytes_read] = '\0';
  pclose(fp);

  if (bytes_read == 0) {
    strncpy(output_buf, "(Command executed successfully with no output)",
            max_len);
  }
  return 0;
}

// Autonomous Multi-Turn Agent Loop for Chat Mode
void ask_chat(const char *initial_prompt) {
  char current_prompt[8192];
  strncpy(current_prompt, initial_prompt, sizeof(current_prompt) - 1);
  current_prompt[sizeof(current_prompt) - 1] = '\0';

  int max_turns = 10; // Safety limit to prevent infinite tool loops
  int turn = 0;

  printf("\033[32mAish:\033[0m ");
  fflush(stdout);

  while (turn < max_turns) {
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

    // Send payload + delimiter to Rust orchestrator
    char payload[9216];
    snprintf(payload, sizeof(payload), "%s__MSG_END__", current_prompt);
    write(sock, payload, strlen(payload));

    // Read streaming chunks live from socket
    char response_buf[16384] = "";
    char chunk[512];
    ssize_t bytes_read;

    while ((bytes_read = read(sock, chunk, sizeof(chunk) - 1)) > 0) {
      chunk[bytes_read] = '\0';

      // Check for ending delimiter
      char *end_pos = strstr(chunk, "__END__");
      if (end_pos) {
        *end_pos = '\0';
        strcat(response_buf, chunk);
        printf("%s", chunk);
        fflush(stdout);
        break;
      }

      strcat(response_buf, chunk);
      printf("%s", chunk);
      fflush(stdout);
    }
    close(sock);

    // Look for tool calls inside ``` codeblocks or <tool_call> tags
    // Look for STRICT tool calls only (to avoid accidentally executing C/Rust
    // code blocks)
    char *tool_start = NULL;
    char *tool_end = NULL;

    if ((tool_start = strstr(response_buf, "```command\n")) != NULL) {
      tool_start += 11;
      tool_end = strstr(tool_start, "```");
    } else if ((tool_start = strstr(response_buf, "```sh\n")) != NULL) {
      tool_start += 6;
      tool_end = strstr(tool_start, "```");
    } else if ((tool_start = strstr(response_buf, "```bash\n")) != NULL) {
      tool_start += 8;
      tool_end = strstr(tool_start, "```");
    } else if ((tool_start = strstr(response_buf, "<tool_call>")) != NULL) {
      tool_start += 11;
      tool_end = strstr(tool_start, "</tool_call>");
    }

    if (tool_start && tool_end && tool_end > tool_start) {
      size_t cmd_len = tool_end - tool_start;

      char tool_cmd[1024];
      if (cmd_len >= sizeof(tool_cmd))
        cmd_len = sizeof(tool_cmd) - 1;
      strncpy(tool_cmd, tool_start, cmd_len);
      tool_cmd[cmd_len] = '\0';

      // Trim leading/trailing whitespace & newlines
      char *trimmed_cmd = tool_cmd;
      while (*trimmed_cmd == ' ' || *trimmed_cmd == '\n' ||
             *trimmed_cmd == '\r')
        trimmed_cmd++;
      char *end = trimmed_cmd + strlen(trimmed_cmd) - 1;
      while (end > trimmed_cmd &&
             (*end == ' ' || *end == '\n' || *end == '\r')) {
        *end = '\0';
        end--;
      }

      if (strlen(trimmed_cmd) > 0) {
        // Visual indicator to user
        printf("\n\033[33m[🔨 Running tool: %s]\033[0m\n", trimmed_cmd);
        fflush(stdout);

        // Execute command and capture stdout/stderr
        char tool_output[4096];
        execute_tool_command(trimmed_cmd, tool_output, sizeof(tool_output));

        // Format next prompt turn with tool result
        snprintf(current_prompt, sizeof(current_prompt),
                 "Tool output for command "
                 "`%s`:\n<tool_response>\n%s\n</tool_response>\nPlease analyze "
                 "this output and answer the user's question.",
                 trimmed_cmd, tool_output);

        turn++;
        printf("\033[32mAish:\033[0m ");
        fflush(stdout);
      } else {
        break; // Empty tool block
      }
    } else {
      // No tool call detected, final answer complete
      break;
    }
  }
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
