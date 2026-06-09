
#include "ai.h"
#include "compress.h"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/socket.h>
#include <sys/un.h>


void ask_chat(char *input){
    int sock = socket(AF_UNIX, SOCK_STREAM, 0);
    if(sock < 0){ perror("socket"); return; }
    
    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, "/tmp/aish_chat.sock", sizeof(addr.sun_path)-1);
    
    if(connect(sock, (struct sockaddr*)&addr, sizeof(addr)) < 0){
        perror("connect");
        close(sock);
        return;
    }
    
    write(sock, input, strlen(input));
    
    char buf[256];
    int n;
    printf("\033[33mAish: \033[0m");
    fflush(stdout);
    while((n = recv(sock, buf, sizeof(buf)-1, 0)) > 0){
        buf[n] = '\0';
        if(strcmp(buf, "__END__") == 0) break;
        printf("%s", buf);
        fflush(stdout);
    }
    printf("\n");
    
    close(sock);

}
char *ask_nlp(char *input){
    char cwd[2048];
    getcwd(cwd,sizeof(cwd));
    char message[4096];
    snprintf(message, sizeof(message), "nlp:[context: currently in %s] %s",cwd,input);
       // write to aish_in
    int fin = open("/tmp/aish_in", O_WRONLY | O_NONBLOCK);
    if(fin < 0) return NULL;
    write(fin, message, strlen(message));
    close(fin);
    
    // read response from aish_out
    int fout = open("/tmp/aish_out", O_RDONLY);
    if(fout < 0) return NULL;
    char *response = malloc(4096);
    int n = read(fout, response, 4095);
    if(n > 0) response[n] = '\0';
    close(fout);
    return response;
}


