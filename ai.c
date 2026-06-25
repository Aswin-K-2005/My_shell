
#include "ai.h"
#include "compress.h"
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

void ask_chat(char *input) {

  char file_content[16384] = {0};
  char detected_file[256] = {0};

  char temp[4096];
  strncpy(temp, input, sizeof(temp) - 1);

  char message[20480];
  if (strlen(file_content) > 0) {
    snprintf(message, sizeof(message), "FILE : %s\nContent:\n%s\n\nQuestion:%s",
             detected_file, file_content, input);

  } else {
    strncpy(message, input, sizeof(message) - 1);
  }

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
    perror("connect");
    close(sock);
    return;
  }
  write(sock, message, strlen(message));
  write(sock, "__MSG_END__", 11);
  char buf[256];
  int n;
  printf("\033[33mAish: \033[0m");
  fflush(stdout);
  while ((n = recv(sock, buf, sizeof(buf) - 1, 0)) > 0) {
    buf[n] = '\0';
    if (strcmp(buf, "__END__") == 0)
      break;
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
  // write to aish_in
  int fin = open("/tmp/aish_in", O_WRONLY | O_NONBLOCK);
  if (fin < 0)
    return NULL;
  write(fin, message, strlen(message));
  close(fin);

  // read response from aish_out
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
