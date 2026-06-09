
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
        char message[4096];
        snprintf(message,sizeof(message),"chat:%s",input);
        
        int fin= open("/tmp/aish_in",O_WRONLY | O_NONBLOCK);
        write(fin,message,strlen(message));
        close(fin);

        int fout=open("/tmp/aish_out",O_RDONLY);
        char *response = malloc(4096);
        int n = read(fout,response,4095);
        if(n>0) response[n]='\0';
        close(fout);
        

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


