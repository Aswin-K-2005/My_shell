#include "compress.h"
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <libgen.h>      // for basename
#include <strings.h>     // for strcasecmp

// ─── Data ───────────────────────────────────────────
typedef struct {
    char word[64];
    int count;
} StopwordCandidate;

char *stopwords[500];
int stopword_count = 0;

StopwordCandidate candidates[1000];
int candidate_count = 0;

#define STOPWORD_THRESHOLD 3

// ─── Stopword functions ──────────────────────────────
void load_stopwords(){
    // load from ~/.config/aish/stopwords.txt
    char path[256];
    snprintf(path,sizeof(path),"%s/.config/aish/stopwords.txt",getenv("HOME"));
    FILE *f= fopen(path,"r");
    if(!f) return;
    char word[64];
    while(fscanf(f, "%63s",word)==1 && stopword_count <500){
        stopwords[stopword_count++]=strdup(word);
    }
    fclose(f);

}

int is_stopword(char *word){
    // check against stopwords array
    for(int i=0;i<stopword_count;i++){
        if(strcasecmp(stopwords[i], word)==0) return 1;
    }
    return 0;
}

void load_candidates(){
    // load from stopword_candidates file
}

void save_candidates(){
    // save candidates back to file
}

void add_candidate(char *word){
    (void)word;
    // increment count, promote if threshold hit
}

// ─── Compression functions ───────────────────────────
void compress_error(char *error,char *out){
        char temp[4096];
        strcpy(temp,error);
        
        char *start=strchr(temp,'\'');
        if(start){
            strncpy(out,error,start-error);
            out[start-error]='\0';
            start++;
            char *end=strchr(start,'\'');
            if(end){
            char *after=end+1;
                *end='\0';
                char *base=basename(start);
                strcat(out,base);
                strcat(out,"'");
                strcat(out,after);
            }    
    }
}



void compress_nlp(char **args, char *out){
    // strip stopwords, join remaining
    out[0] ='\0';
    for(int i=0;args[i]!=NULL;i++){
        if(!is_stopword(args[i])){
            if(strlen(out)>0) strcat(out, " ");
            strcat(out, args[i]);
        }
    }
}
