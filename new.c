#include "ai.h"
#include "compress.h"
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <libgen.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/prctl.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <termios.h>
#include <time.h>
#include <unistd.h>

pid_t background_pids[100];
int background_count = 0;

void start_ai_orchestrator() {
  if (access("/tmp/aish_chat.sock", F_OK) != -1) {
    int test_sock = socket(AF_UNIX, SOCK_STREAM, 0);
    struct sockaddr_un addr;
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, "/tmp/aish_chat.sock", sizeof(addr.sun_path) - 1);

    if (connect(test_sock, (struct sockaddr *)&addr, sizeof(addr)) == 0) {
      close(test_sock);
      return;
    } else {
      printf("🧹 Found dead socket. Cleaning it up...\n");
      close(test_sock);
      unlink("/tmp/aish_chat.sock");
    }
  }

  printf("🚀 Booting AI Orchestrator daemon...\n");
  pid_t pid = fork();
  if (pid < 0) {
    perror("Failed to fork AI orchestrator");
    return;
  }

  if (pid == 0) {
    prctl(PR_SET_PDEATHSIG, SIGTERM);

    int log_fd =
        open("/tmp/aish_orchestrator.log", O_WRONLY | O_CREAT | O_APPEND, 0644);
    if (log_fd >= 0) {
      dup2(log_fd, STDOUT_FILENO);
      dup2(log_fd, STDERR_FILENO);
      close(log_fd);
    }

    int null_fd = open("/dev/null", O_RDONLY);
    if (null_fd >= 0) {
      dup2(null_fd, STDIN_FILENO);
      close(null_fd);
    }

    char exe_path[512];
    snprintf(exe_path, sizeof(exe_path),
             "%s/coding/My_shell/target/release/ai_orchestrator",
             getenv("HOME"));

    execl(exe_path, "ai_orchestrator", NULL);
    perror("Failed to exec ai_orchestrator");
    exit(1);
  }

  usleep(500000);
}

void add_background_pid(pid_t pid) {
  background_pids[background_count++] = pid;
}

volatile sig_atomic_t flag = 0;
volatile pid_t done_pid = 0;

// FIX: Only reap explicitly registered background processes so foreground
// commands don't freeze!
void sigchld_handler(int sig) {
  (void)sig;
  int saved_errno = errno;
  for (int i = 0; i < background_count; i++) {
    if (background_pids[i] > 0) {
      pid_t done = waitpid(background_pids[i], NULL, WNOHANG);
      if (done > 0) {
        background_pids[i] = 0; // Clear it out
        flag = 1;
      }
    }
  }
  errno = saved_errno;
}

#define LSH_RL_BUFSIZE 1024
#define history_size 100
#define ALPHABET_SIZE 128

char *history[history_size] = {NULL};
int history_count = 0;

typedef struct {
  char command[256];
  int count;
} CommandFreq;

CommandFreq freq_table[1000];
int freq_count = 0;
int last_exit_status = 0;

int compare_freq(const void *a, const void *b) {
  char *cmd_a = *(char **)a;
  char *cmd_b = *(char **)b;
  int freq_a = 0, freq_b = 0;
  for (int i = 0; i < freq_count; i++) {
    if (strcmp(freq_table[i].command, cmd_a) == 0)
      freq_a = freq_table[i].count;
    if (strcmp(freq_table[i].command, cmd_b) == 0)
      freq_b = freq_table[i].count;
  }
  if (freq_b != freq_a)
    return freq_b - freq_a;
  return strlen(cmd_a) - strlen(cmd_b);
}

void update_freq(char *cmd) {
  for (int i = 0; i < freq_count; i++) {
    if (strcmp(freq_table[i].command, cmd) == 0) {
      freq_table[i].count++;
      return;
    }
  }
  if (freq_count < 1000) {
    strncpy(freq_table[freq_count].command, cmd, 255);
    freq_table[freq_count].count = 1;
    freq_count++;
  }
}

void load_freq() {
  char path[256];
  snprintf(path, sizeof(path), "%s/.aish_freq", getenv("HOME"));
  FILE *f = fopen(path, "r");
  if (!f) {
    char *defaults[] = {"ls",  "cd",   "git",  "gcc",     "cargo", "grep",
                        "cat", "nvim", "make", "python3", "exit",  NULL};
    for (int i = 0; defaults[i] != NULL; i++) {
      strncpy(freq_table[freq_count].command, defaults[i], 255);
      freq_table[freq_count].count = 1;
      freq_count++;
    }
    return;
  }

  while (freq_count < 1000) {
    if (fscanf(f, "%255s %d", freq_table[freq_count].command,
               &freq_table[freq_count].count) != 2)
      break;
    freq_count++;
  }
  fclose(f);
}

void save_freq() {
  char path[256];
  snprintf(path, sizeof(path), "%s/.aish_freq", getenv("HOME"));
  FILE *f = fopen(path, "w");
  if (!f)
    return;
  for (int i = 0; i < freq_count; i++) {
    fprintf(f, "%s %d\n", freq_table[i].command, freq_table[i].count);
  }
  fclose(f);
}

struct termios orig_termios;
void disable_raw_mode() { tcsetattr(STDIN_FILENO, TCSAFLUSH, &orig_termios); }
void enable_raw_mode() {
  tcgetattr(STDIN_FILENO, &orig_termios);
  atexit(disable_raw_mode);
  struct termios raw = orig_termios;
  raw.c_lflag &= ~(ECHO | ICANON | ISIG);
  tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw);
}

typedef struct TrieNode {
  struct TrieNode *children[128];
  int is_end;
} TrieNode;

int shell_mode = 0;
int lsh_cd(char **args);
int lsh_help(char **args);
int lsh_exit(char **args);
int lsh_history(char **args);
int lsh_mode(char **args);

char *builtin_str[] = {"cd", "help", "history", "exit", "mode"};
int (*builtin_func[])(char **) = {&lsh_cd, &lsh_help, &lsh_history, &lsh_exit,
                                  &lsh_mode};

int lsh_mode(char **args) {
  if (args[1] == NULL) {
    if (shell_mode == 2) {
      printf("current mode: chat\n");
      notify_rust_vram_state("entered_chat");
      return 1;
    }
    if (shell_mode == 1) {
      printf("current mode: nlp\n");
      notify_rust_vram_state("exited_chat");
      return 1;
    }
    if (shell_mode == 0) {
      printf("current mode: shell\n");
      notify_rust_vram_state("exited_chat");
      return 1;
    }
  }
  if (strcmp(args[1], "nlp") == 0 || strcmp(args[1], "NLP") == 0) {
    shell_mode = 1;
    printf("switched to NLP mode\n");
  } else if (strcmp(args[1], "shell") == 0 || strcmp(args[1], "Shell") == 0) {
    shell_mode = 0;
    printf("switched to shell mode\n");
  } else if (strcmp(args[1], "chat") == 0 || strcmp(args[1], "Chat") == 0) {
    shell_mode = 2;
    printf("switched to chat mode\n");
  } else {
    printf("unknown mode: %s\nAvailable modes : nlp shell chat\n", args[1]);
  }
  return 1;
}

TrieNode *trie_new_node() {
  TrieNode *node = calloc(1, sizeof(TrieNode));
  if (!node) {
    fprintf(stderr, "lsh: allocation error\n");
    exit(EXIT_FAILURE);
  }
  return node;
}

void trie_insert(TrieNode *root, char *word) {
  TrieNode *current = root;
  for (int i = 0; word[i] != '\0'; i++) {
    int idx = (unsigned char)word[i];
    if (current->children[idx] == NULL)
      current->children[idx] = trie_new_node();
    current = current->children[idx];
  }
  current->is_end = 1;
}

void trie_collect(TrieNode *node, char *prefix, char **results, int *count) {
  if (*count >= 2048)
    return;
  if (node->is_end) {
    results[*count] = strdup(prefix);
    (*count)++;
  }
  for (int i = 0; i < 128; i++) {
    if (node->children[i] != NULL) {
      int len = strlen(prefix);
      char new_prefix[1024];
      strncpy(new_prefix, prefix, 1024);
      new_prefix[len] = (char)i;
      new_prefix[len + 1] = '\0';
      trie_collect(node->children[i], new_prefix, results, count);
    }
  }
}

void trie_search(TrieNode *root, char *prefix, char **results, int *count) {
  TrieNode *current = root;
  for (int i = 0; prefix[i] != '\0'; i++) {
    int idx = (unsigned)prefix[i];
    if (current->children[idx] == NULL)
      return;
    current = current->children[idx];
  }
  trie_collect(current, prefix, results, count);
}

int lsh_is_background(char **args) {
  int i = 0;
  while (args[i] != NULL)
    i++;
  if (i > 0 && strcmp(args[i - 1], "&") == 0) {
    args[i - 1] = NULL;
    return 1;
  }
  return 0;
}

void load_commands(TrieNode *root) {
  char *path = getenv("PATH");
  char *path_copy = strdup(path);
  char *dir = strtok(path_copy, ":");
  while (dir != NULL) {
    DIR *d = opendir(dir);
    if (d) {
      struct dirent *entry;
      while ((entry = readdir(d)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0)
          continue;
        struct stat st;
        char fullpath[1024];
        snprintf(fullpath, sizeof(fullpath), "%s/%s", dir, entry->d_name);
        if (stat(fullpath, &st) == 0 && (st.st_mode & S_IXUSR))
          trie_insert(root, entry->d_name);
      }
      closedir(d);
    }
    dir = strtok(NULL, ":");
  }
  free(path_copy);
}

int lsh_find_pipe(char **args) {
  for (int i = 0; args[i] != NULL; i++) {
    if (strcmp(args[i], "|") == 0)
      return i;
  }
  return -1;
}

void lsh_split_pipe(char **args, int pipe_index, char **left, char **right) {
  int i, pos = 0;
  for (i = 0; i < pipe_index; i++)
    left[i] = args[i];
  left[i] = NULL;
  for (i = pipe_index + 1; args[i] != NULL; i++)
    right[pos++] = args[i];
  right[pos] = NULL;
}

int lsh_handle_redirections(char **args) {
  int i = 0;
  while (args[i] != NULL) {
    if (strcmp(args[i], ">") == 0 || strcmp(args[i], ">>") == 0 ||
        strcmp(args[i], "<") == 0) {
      if (args[i + 1] == NULL) {
        fprintf(stderr, "lsh: No filename after %s\n", args[i]);
        return -1;
      }
      int fd;
      if (strcmp(args[i], ">") == 0) {
        fd = open(args[i + 1], O_WRONLY | O_CREAT | O_TRUNC, 0644);
        dup2(fd, STDOUT_FILENO);
        close(fd);
      } else if (strcmp(args[i], ">>") == 0) {
        fd = open(args[i + 1], O_WRONLY | O_CREAT | O_APPEND, 0644);
        dup2(fd, STDOUT_FILENO);
        close(fd);
      } else if (strcmp(args[i], "<") == 0) {
        fd = open(args[i + 1], O_RDONLY);
        dup2(fd, STDIN_FILENO);
        close(fd);
      }
      int j = i;
      while (args[j + 2] != NULL) {
        args[j] = args[j + 2];
        j++;
      }
      args[j] = NULL;
    } else
      i++;
  }
  return 0;
}

int lsh_num_builtins() { return sizeof(builtin_str) / sizeof(char *); }
int lsh_history(char **args) {
  (void)args;
  for (int i = 0; i < history_count && i < history_size; i++)
    printf("%d) %s\n", i + 1, history[i]);
  return 1;
}

int lsh_cd(char **args) {
  if (args[1] == NULL)
    fprintf(stderr, "lsh: expected argument to \"cd\"\n");
  else if (chdir(args[1]) != 0)
    perror("lsh");
  return 1;
}

int lsh_help(char **args) {
  (void)args;
  printf("Aswin's Shell LSH\n");
  for (int i = 0; i < lsh_num_builtins(); i++)
    printf("  %s\n", builtin_str[i]);
  return 1;
}

int lsh_exit(char **args) {
  (void)args;
  return 0;
}

typedef struct {
  int index;
  char *op;
} OpResult;

OpResult lsh_find_andor(char **args) {
  OpResult result;
  for (int i = 0; args[i] != NULL; i++) {
    if (strcmp(args[i], "&&") == 0) {
      result.index = i;
      result.op = "&&";
      return result;
    } else if (strcmp(args[i], "||") == 0) {
      result.index = i;
      result.op = "||";
      return result;
    }
  }
  result.index = -1;
  return result;
}

void lsh_split_andor(char **args, int index, char **left, char **right) {
  int i, pos = 0;
  for (i = 0; i < index; i++)
    left[i] = args[i];
  left[i] = NULL;
  for (i = index + 1; args[i] != NULL; i++)
    right[pos++] = args[i];
  right[pos] = NULL;
}

int lsh_run_get_status(char **args);

int lsh_launch(char **args) {
  int stderr_pipe[2];
  pipe(stderr_pipe);
  OpResult result = lsh_find_andor(args);

  if (result.index != -1) {
    char **left = malloc(sizeof(char *) * LSH_RL_BUFSIZE);
    char **right = malloc(sizeof(char *) * LSH_RL_BUFSIZE);
    lsh_split_andor(args, result.index, left, right);
    int status = lsh_run_get_status(left);
    if (strcmp(result.op, "&&") == 0) {
      if (status == 0)
        lsh_launch(right);
    } else {
      if (status != 0)
        lsh_launch(right);
    }
    free(right);
    free(left);
    close(stderr_pipe[0]);
    close(stderr_pipe[1]);
    return 1;
  }
  int pipe_index = lsh_find_pipe(args);
  if (pipe_index != -1) {
    char **left = malloc(sizeof(char *) * LSH_RL_BUFSIZE);
    char **right = malloc(sizeof(char *) * LSH_RL_BUFSIZE);
    lsh_split_pipe(args, pipe_index, left, right);
    int pipefd[2];
    if (pipe(pipefd) == -1) {
      perror("lsh");
      return 1;
    }

    pid_t pid1 = fork();
    if (pid1 == 0) {
      signal(SIGINT, SIG_DFL);
      close(pipefd[0]);
      dup2(pipefd[1], STDOUT_FILENO);
      close(pipefd[1]);
      lsh_handle_redirections(left);
      execvp(left[0], left);
      perror("lsh");
      exit(EXIT_FAILURE);
    }

    pid_t pid2 = fork();
    if (pid2 == 0) {
      signal(SIGINT, SIG_DFL);
      close(pipefd[1]);
      dup2(pipefd[0], STDIN_FILENO);
      close(pipefd[0]);
      lsh_launch(right);
      exit(last_exit_status);
    }

    close(pipefd[0]);
    close(pipefd[1]);
    waitpid(pid1, NULL, 0);
    waitpid(pid2, NULL, 0);
    free(left);
    free(right);
    close(stderr_pipe[0]);
    close(stderr_pipe[1]);
    return 1;
  }

  int is_background = lsh_is_background(args);
  pid_t pid = fork();
  int status;

  if (pid == 0) {
    signal(SIGINT, SIG_DFL);
    close(stderr_pipe[0]);
    dup2(stderr_pipe[1], STDERR_FILENO);
    close(stderr_pipe[1]);

    // FIX: Force colored output for tools that disable it when piped!
    setenv("CARGO_TERM_COLOR", "always", 1);
    setenv("CLICOLOR_FORCE", "1", 1);
    setenv("FORCE_COLOR", "1", 1);

    lsh_handle_redirections(args);
    if (execvp(args[0], args) == -1)
      perror("lsh");
    exit(EXIT_FAILURE);
  } else if (pid > 0) {
    close(stderr_pipe[1]);
    if (is_background) {
      add_background_pid(pid);
      close(stderr_pipe[0]);
    } else {
      char error_buf[8192] = "";
      int err_pos = 0;
      struct pollfd pfd;
      pfd.fd = stderr_pipe[0];
      pfd.events = POLLIN;

      while (1) {
        int ret = poll(&pfd, 1, 10);
        if (ret > 0 && (pfd.revents & POLLIN)) {
          char temp_buf[256];
          int n = read(stderr_pipe[0], temp_buf, sizeof(temp_buf) - 1);
          if (n > 0) {
            temp_buf[n] = '\0';
            printf("%s", temp_buf);
            fflush(stdout);
            if (err_pos + n < (int)sizeof(error_buf)) {
              strcpy(error_buf + err_pos, temp_buf);
              err_pos += n;
            }
          }
        }

        pid_t wpid = waitpid(pid, &status, WNOHANG | WUNTRACED);
        // FIX: Only break when waitpid confirms process is actually dead!
        if (wpid == pid || (wpid == -1 && errno == ECHILD)) {
          fcntl(stderr_pipe[0], F_SETFL, O_NONBLOCK);
          while (1) {
            char temp_buf[256];
            int n = read(stderr_pipe[0], temp_buf, sizeof(temp_buf) - 1);
            if (n <= 0)
              break;
            temp_buf[n] = '\0';
            printf("%s", temp_buf);
            fflush(stdout);
            if (err_pos + n < (int)sizeof(error_buf)) {
              strcpy(error_buf + err_pos, temp_buf);
              err_pos += n;
            }
          }
          break;
        }
      }

      last_exit_status = WEXITSTATUS(status);
      if (WIFSIGNALED(status))
        printf("\n");

      if (last_exit_status != 0 && err_pos > 0) {
        char full_command[1024] = "";
        for (int i = 0; args[i] != NULL; i++) {
          strcat(full_command, args[i]);
          if (args[i + 1] != NULL)
            strcat(full_command, " ");
        }
        ask_ai(full_command, error_buf);
      }
      close(stderr_pipe[0]);
      return 1;
    }
  }
  return 1;
}

int lsh_run_get_status(char **args) {
  lsh_launch(args);
  return last_exit_status;
}
char **lsh_split_line(char *line);
void expand_args(char **args) {
  char *home = getenv("HOME");
  for (int i = 0; args[i] != NULL; i++) {
    if (args[i][0] == '~') {
      char expanded[1024];
      snprintf(expanded, sizeof(expanded), "%s%s", home, args[i] + 1);
      free(args[i]);
      args[i] = strdup(expanded);
    } else if (args[i][0] == '$') {
      char *value = getenv(args[i] + 1);
      if (value != NULL) {
        free(args[i]);
        args[i] = strdup(value);
      }
    }
  }
}

void save_memory(char **args) {
  char path[256];
  snprintf(path, sizeof(path), "%s/.config/aish/memory.json", getenv("HOME"));
  char cwd[1024];
  getcwd(cwd, sizeof(cwd));
  char command[4096];
  command[0] = '\0';
  for (int i = 0; args[i] != NULL; i++) {
    strcat(command, args[i]);
    if (args[i + 1] != NULL)
      strcat(command, " ");
  }

  char escaped_command[8192];
  json_escape(command, escaped_command, sizeof(escaped_command));

  time_t now = time(NULL);
  struct tm *t_utc = gmtime(&now);
  char iso_time[64];
  strftime(iso_time, sizeof(iso_time), "%Y-%m-%dT%H:%M:%SZ", t_utc);

  char json_payload[16384];
  snprintf(json_payload, sizeof(json_payload),
           "{\n  \"session_id\": \"%d\",\n  \"raw_command\": \"%s\",\n  "
           "\"working_directory\": \"%s\",\n  \"exit_code\": %d,\n  "
           "\"start_timestamp\": \"%s\",\n  \"execution_duration_ms\": 0\n}",
           getpid(), escaped_command, cwd, last_exit_status, iso_time);
  send_telemetry_to_rust(json_payload);
}

int lsh_execute(char **args) {
  if (args[0] == NULL)
    return 1;
  expand_args(args);
  update_freq(args[0]);
  for (int i = 0; i < lsh_num_builtins(); i++) {
    if (strcmp(args[0], builtin_str[i]) == 0) {
      int result = (*builtin_func[i])(args);
      save_memory(args);
      return result;
    }
  }
  if (shell_mode == 1) {
    char compressed[4096];
    compress_nlp(args, compressed);
    char *cmd = ask_nlp(compressed);
    if (cmd != NULL) {
      printf("Running: \033[32m%s\033[0m\nExecute? (y/n): ", cmd);
      fflush(stdout);
      char confirm[4];
      fgets(confirm, sizeof(confirm), stdin);
      if (confirm[0] == 'y') {
        char **nlp_args = lsh_split_line(cmd);
        shell_mode = 0;
        lsh_execute(nlp_args);
        shell_mode = 1;
        free(nlp_args);
      }
      free(cmd);
    }
    save_memory(args);
    return 1;
  } else if (shell_mode == 2) {
    char message[4096];
    message[0] = '\0';
    for (int i = 0; args[i] != NULL; i++) {
      strcat(message, args[i]);
      if (args[i + 1] != NULL)
        strcat(message, " ");
    }
    ask_chat(message);
    save_memory(args);
    return 1;
  }
  int result = lsh_launch(args);
  save_memory(args);
  return result;
}

void show_path_ghost(char *buffer, char *ghost) {
  char *last_space = strrchr(buffer, ' ');
  if (!last_space)
    return;
  char *prefix = last_space + 1;

  char dir_path[1024] = ".";
  char base_prefix[1024] = "";
  char *last_slash = strrchr(prefix, '/');

  if (last_slash) {
    int dir_len = last_slash - prefix;
    if (dir_len == 0)
      strcpy(dir_path, "/");
    else {
      strncpy(dir_path, prefix, dir_len);
      dir_path[dir_len] = '\0';
    }
    strcpy(base_prefix, last_slash + 1);
  } else {
    strcpy(base_prefix, prefix);
  }

  DIR *d = opendir(dir_path);
  if (d) {
    struct dirent *entry;
    while ((entry = readdir(d)) != NULL) {
      if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0)
        continue;
      if (strncmp(entry->d_name, base_prefix, strlen(base_prefix)) == 0) {
        char *ghost_part = entry->d_name + strlen(base_prefix);
        if (strlen(ghost_part) > 0) {
          strncpy(ghost, ghost_part, 1023);
          printf("\033[2m%s\033[0m", ghost_part);

          struct stat st;
          char full_path[2048];
          snprintf(full_path, sizeof(full_path), "%s/%s", dir_path,
                   entry->d_name);
          if (stat(full_path, &st) == 0 && S_ISDIR(st.st_mode)) {
            strcat(ghost, "/");
            printf("\033[2m/\033[0m");
          }

          printf("\033[%dD", (int)strlen(ghost));
          fflush(stdout);
          closedir(d);
          return;
        }
      }
    }
    closedir(d);
  }
  ghost[0] = '\0';
}

void show_ghost(char *prefix, TrieNode *root, char *ghost) {
  char **results = malloc(sizeof(char *) * 2048);
  int count = 0;
  trie_search(root, prefix, results, &count);
  if (count > 1)
    qsort(results, count, sizeof(char *), compare_freq);

  if (count > 0) {
    char *suggestion = results[0];
    char *ghost_part = suggestion + strlen(prefix);
    if (strlen(ghost_part) == 0)
      ghost[0] = '\0';
    else {
      strncpy(ghost, ghost_part, 1023);
      printf("\033[2m%s\033[0m", ghost_part);
      printf("\033[%dD", (int)strlen(ghost_part));
      fflush(stdout);
    }
  } else
    ghost[0] = '\0';
  for (int j = 0; j < count; j++)
    free(results[j]);
  free(results);
}

void print_prompt(double elapsed) {
  char cwd[1024];
  getcwd(cwd, sizeof(cwd));
  char *home = getenv("HOME");
  if (strncmp(cwd, home, strlen(home)) == 0)
    printf("~%s", cwd + strlen(home));
  else
    printf("%s", cwd);

  FILE *git = fopen(".git/HEAD", "r");
  if (git) {
    char branch[256];
    fgets(branch, sizeof(branch), git);
    fclose(git);
    char *prefix = "ref: refs/heads/";
    if (strncmp(branch, prefix, strlen(prefix)) == 0) {
      char *branchname = branch + strlen(prefix);
      branchname[strcspn(branchname, "\n")] = '\0';
      printf("  \033[32m%s\033[0m", branchname);
    }
  }

  time_t now = time(NULL);
  struct tm *t = localtime(&now);
  char timestr[16];
  strftime(timestr, sizeof(timestr), "%I:%M:%S %p", t);
  printf("  \033[34m%s\033[0m", timestr);
  if (elapsed > 0.1)
    printf("  \033[33m%.1fs\033[0m", elapsed);

  printf("\n\033[1;36maish\033[0m \033[1;32m❯\033[1;33m❯\033[1;31m❯\033[0m ");
  fflush(stdout);
}

char *lsh_read_line_raw(TrieNode *root, double elapsed) {
  enable_raw_mode();
  int bufsize = LSH_RL_BUFSIZE;
  int position = 0, cursor = 0, c;
  char *buffer = malloc(sizeof(char) * bufsize);
  char *saved_buffer = malloc(sizeof(char) * bufsize);
  saved_buffer[0] = '\0';
  buffer[0] = '\0';
  int history_index = history_count;
  char ghost[1024];
  ghost[0] = '\0';

  print_prompt(elapsed);
  while (1) {
    if (flag) {
      printf("\r\033[K[done] background job finished\n");
      printf(
          "\033[1;36maish\033[0m \033[1;32m❯\033[1;33m❯\033[1;31m❯\033[0m %s",
          buffer);
      int diff = position - cursor;
      if (diff > 0)
        printf("\033[%dD", diff);
      fflush(stdout);
      flag = 0;
      done_pid = 0;
    }
    c = getchar();
    if (c == EOF || c == '\n') {
      if (strlen(ghost) > 0) {
        printf("\033[2m%*s\033[0m", (int)strlen(ghost), "");
        ghost[0] = '\0';
      }
      disable_raw_mode();
      printf("\n");
      buffer[position] = '\0';
      free(saved_buffer);
      return buffer;
    } else if (c == 3) {
      printf("^C\n");
      position = 0;
      cursor = 0;
      buffer[0] = '\0';
      ghost[0] = '\0';
      print_prompt(elapsed);
    } else if (c == 127) {
      ghost[0] = '\0';
      if (cursor > 0 && position > 0) {
        cursor--;
        for (int j = cursor; j < position; j++)
          buffer[j] = buffer[j + 1];
        position--;
        buffer[position] = '\0';
        printf("\r\033[K\033[1;36maish\033[0m "
               "\033[1;32m❯\033[1;33m❯\033[1;31m❯\033[0m %s",
               buffer);
        if (position - cursor > 0)
          printf("\033[%dD", position - cursor);
        fflush(stdout);
        if (position == cursor && position > 0) {
          if (strchr(buffer, ' ') == NULL)
            show_ghost(buffer, root, ghost);
          else
            show_path_ghost(buffer, ghost);
        }
      }
    } else if (c == 27) {
      char seq[2];
      seq[0] = getchar();
      seq[1] = getchar();
      if (seq[0] == '[') {
        if (seq[1] == 'A') {
          if (history_index > 0) {
            if (history_index == history_count)
              strncpy(saved_buffer, buffer, bufsize);
            history_index--;
            strncpy(buffer, history[history_index], bufsize);
            position = strlen(buffer);
            cursor = position;
            printf("\r\033[K\033[1;36maish\033[0m "
                   "\033[1;32m❯\033[1;33m❯\033[1;31m❯\033[0m %s",
                   buffer);
            fflush(stdout);
          }
        } else if (seq[1] == 'B') {
          if (history_index < history_count) {
            history_index++;
            if (history_index == history_count)
              strncpy(buffer, saved_buffer, bufsize);
            else
              strncpy(buffer, history[history_index], bufsize);
            position = strlen(buffer);
            cursor = position;
            printf("\r\033[K\033[1;36maish\033[0m "
                   "\033[1;32m❯\033[1;33m❯\033[1;31m❯\033[0m %s",
                   buffer);
            fflush(stdout);
          }
        } else if (seq[1] == 'D') {
          if (strlen(ghost) > 0) {
            printf("\033[2m%*s\033[0m\033[%dD", (int)strlen(ghost), "",
                   (int)strlen(ghost));
            ghost[0] = '\0';
          }
          if (cursor != 0) {
            cursor--;
            printf("\033[1D");
          }
        } else if (seq[1] == 'C') {
          if (strlen(ghost) > 0) {
            cursor++;
            buffer[position++] = ghost[0];
            memmove(ghost, ghost + 1, strlen(ghost));
            buffer[position] = '\0';
            printf("\r\033[K\033[1;36maish\033[0m "
                   "\033[1;32m❯\033[1;33m❯\033[1;31m❯\033[0m %s",
                   buffer);
            if (strlen(ghost) > 0)
              printf("\033[2m%s\033[0m\033[%dD", ghost, (int)strlen(ghost));
            fflush(stdout);
          } else if (cursor != position) {
            cursor++;
            printf("\033[1C");
          }
        }
      }
    } else if (c == 9) {
      if (strlen(ghost) > 0) {
        printf("\033[2m%*s\033[0m\033[%dD", (int)strlen(ghost), "",
               (int)strlen(ghost));
        strncpy(buffer + position, ghost, bufsize - position);
        position += strlen(ghost);
        cursor = position;
        ghost[0] = '\0';
        printf("\r\033[K\033[1;36maish\033[0m "
               "\033[1;32m❯\033[1;33m❯\033[1;31m❯\033[0m %s",
               buffer);
        fflush(stdout);
      }
    } else {
      if (strlen(ghost) > 0) {
        printf("\033[2m%*s\033[0m\033[%dD", (int)strlen(ghost), "",
               (int)strlen(ghost));
        ghost[0] = '\0';
      }
      if (cursor < position) {
        for (int j = position; j > cursor; j--)
          buffer[j] = buffer[j - 1];
        buffer[cursor] = c;
        cursor++;
        position++;
        buffer[position] = '\0';
        printf("\r\033[K\033[1;36maish\033[0m "
               "\033[1;32m❯\033[1;33m❯\033[1;31m❯\033[0m %s",
               buffer);
        if (position - cursor > 0)
          printf("\033[%dD", position - cursor);
        fflush(stdout);
      } else {
        cursor++;
        buffer[position++] = c;
        buffer[position] = '\0';
        printf("%c", c);
        fflush(stdout);
        if (position == cursor) {
          if (strchr(buffer, ' ') == NULL)
            show_ghost(buffer, root, ghost);
          else
            show_path_ghost(buffer, ghost);
        }
      }
    }
    if (position >= bufsize) {
      bufsize += LSH_RL_BUFSIZE;
      buffer = realloc(buffer, bufsize);
    }
  }
}

#define LSH_TOK_BUFSIZE 64
int is_operator(char c, char **operators) {
  for (int i = 0; operators[i] != NULL; i++) {
    if (c == operators[i][0])
      return 1;
  }
  return 0;
}
char **lsh_split_line(char *line) {
  int bufsize = LSH_TOK_BUFSIZE, position = 0, token_pos = 0, in_quotes = 0,
      i = 0;
  char **tokens = malloc(bufsize * sizeof(char *));
  char *token = malloc(LSH_RL_BUFSIZE);
  char *operators[] = {">", "<", "|", "&", NULL};

  while (line[i] != '\0') {
    char c = line[i];
    if (c == '"')
      in_quotes = !in_quotes;
    else if (is_operator(c, operators) && !in_quotes) {
      if (token_pos > 0) {
        if (position >= bufsize) {
          bufsize += LSH_TOK_BUFSIZE;
          tokens = realloc(tokens, bufsize * sizeof(char *));
        }
        token[token_pos] = '\0';
        tokens[position++] = strdup(token);
        token_pos = 0;
      }
      char t = line[i + 1];
      if (t == c) {
        if (position >= bufsize) {
          bufsize += LSH_TOK_BUFSIZE;
          tokens = realloc(tokens, bufsize * sizeof(char *));
        }
        token[0] = c;
        token[1] = t;
        token[2] = '\0';
        tokens[position++] = strdup(token);
        token_pos = 0;
        i++;
      } else {
        if (position >= bufsize) {
          bufsize += LSH_TOK_BUFSIZE;
          tokens = realloc(tokens, bufsize * sizeof(char *));
        }
        token[0] = c;
        token[1] = '\0';
        tokens[position++] = strdup(token);
        token_pos = 0;
      }
    } else if (c == ' ' && !in_quotes) {
      if (token_pos > 0) {
        if (position >= bufsize) {
          bufsize += LSH_TOK_BUFSIZE;
          tokens = realloc(tokens, bufsize * sizeof(char *));
        }
        token[token_pos] = '\0';
        tokens[position++] = strdup(token);
        token_pos = 0;
      }
    } else
      token[token_pos++] = c;
    i++;
  }
  if (token_pos > 0) {
    if (position >= bufsize) {
      bufsize += LSH_TOK_BUFSIZE;
      tokens = realloc(tokens, bufsize * sizeof(char *));
    }
    token[token_pos] = '\0';
    tokens[position++] = strdup(token);
  }
  tokens[position] = NULL;
  free(token);
  return tokens;
}

void lsh_loop(TrieNode *root) {
  char *line, **args;
  int status;
  double elapsed = 0.0;
  do {
    line = lsh_read_line_raw(root, elapsed);
    if (strlen(line) > 0) {
      int slot = history_count % history_size;
      if (history[slot] != NULL)
        free(history[slot]);
      history[slot] = strdup(line);
      history_count++;
    }
    args = lsh_split_line(line);
    struct timespec start, end;
    clock_gettime(CLOCK_MONOTONIC, &start);
    status = lsh_execute(args);
    clock_gettime(CLOCK_MONOTONIC, &end);
    elapsed = (end.tv_sec - start.tv_sec) + (end.tv_nsec - start.tv_nsec) / 1e9;
    free(line);
    for (int i = 0; args[i] != NULL; i++)
      free(args[i]);
    free(args);
  } while (status);
}

int main(int argc, char **argv) {
  (void)argc;
  (void)argv;
  load_freq();
  atexit(save_freq);
  start_ai_orchestrator();
  TrieNode *root = trie_new_node();
  load_commands(root);
  for (int i = 0; i < lsh_num_builtins(); i++)
    trie_insert(root, builtin_str[i]);
  signal(SIGCHLD, sigchld_handler);
  signal(SIGINT, SIG_IGN);
  lsh_loop(root);
  notify_rust_vram_state("exited_chat");
  return EXIT_SUCCESS;
}
