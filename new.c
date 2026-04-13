
#include <signal.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <fcntl.h>
#include <errno.h>
#include <termios.h>
#include <dirent.h>
#include <sys/stat.h>


volatile sig_atomic_t flag=0;
volatile pid_t done_pid=0;

void sigchld_handler(int sig){
    int saved_errno = errno;
    pid_t done;
    while((done=waitpid(-1,NULL,WNOHANG))>0){
        done_pid=done;
        flag=1;   
    }
    errno = saved_errno;
}
#define LSH_RL_BUFSIZE 1024
#define history_size 100
#define ALPHABET_SIZE 128

char *history[history_size]= {NULL};
int history_count =0;

typedef struct {
    char command[256];
    int count;
} CommandFreq;

CommandFreq freq_table[1000];
int freq_count =0;

int compare_freq(const void *a, const void *b){
    char *cmd_a = *(char**)a;
    char *cmd_b = *(char**)b;
    int freq_a = 0, freq_b = 0;
    int i;
    for(i = 0; i < freq_count; i++){
        if(strcmp(freq_table[i].command, cmd_a) == 0) freq_a = freq_table[i].count;
        if(strcmp(freq_table[i].command, cmd_b) == 0) freq_b = freq_table[i].count;
    }
    if(freq_b != freq_a) return freq_b - freq_a;  // higher freq first
    return strlen(cmd_a) - strlen(cmd_b);          // then shorter
}


void update_freq(char *cmd){
    int i;
    for(i=0;i<freq_count;i++){
        if(strcmp(freq_table[i].command,cmd)==0){
            freq_table[i].count++;
            return;
        }
    }
    if(freq_count<1000){
        strncpy(freq_table[freq_count].command,cmd,255);
        freq_table[freq_count].count=1;
        freq_count++;
    }
}

void load_freq(){
     char path[256];
    snprintf(path, sizeof(path), "%s/.aish_freq", getenv("HOME"));
    FILE *f = fopen(path, "r");
    if(!f){
        char *defaults[]= {"ls","cd","git","gcc","grep","cat","nvim","make","python3","exit",NULL};
        int i;
        for(i=0;defaults[i]!=NULL;i++){
            strncpy(freq_table[freq_count].command,defaults[i],255);
            freq_table[freq_count].count =1;
            freq_count++;
            }
                return;
        }
            
    while(freq_count <1000){
        if(fscanf(f, "%255s %d",freq_table[freq_count].command,&freq_table[freq_count].count) != 2) break;
        freq_count++;
    }
    fclose(f);

}

void save_freq()
{
        char path[256];
        snprintf(path, sizeof(path), "%s/.aish_freq",getenv("HOME"));
        FILE *f=fopen(path,"w");
        if(!f) return;
        int i;
        for(i=0;i<freq_count;i++){
            fprintf(f,"%s %d\n",freq_table[i].command,freq_table[i].count);
        }
        fclose(f);
    

}  
struct termios orig_termios;

void disable_raw_mode(){
    tcsetattr(STDIN_FILENO,TCSAFLUSH, &orig_termios);
}

void enable_raw_mode(){
    tcgetattr(STDIN_FILENO, &orig_termios);
    atexit(disable_raw_mode);

    struct termios raw = orig_termios;
    raw.c_lflag &= ~(ECHO | ICANON);
    tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw);
}


typedef struct TrieNode{
    struct TrieNode *children[128];
    int is_end;
}TrieNode;




/*
  Function Declarations for builtin shell commands:
 */
int lsh_cd(char **args);
int lsh_help(char **args);
int lsh_exit(char **args);
int lsh_history(char **args);
/*
  List of builtin commands, followed by their corresponding functions.
 */
char *builtin_str[] = {
    "cd",
    "help",
    "history",
    "exit"
};

int (*builtin_func[]) (char **) = {
    &lsh_cd,
    &lsh_help,
    &lsh_history,
    &lsh_exit
};

int compare_length(const void *a, const void *b){
    return strlen(*(char**)a) - strlen(*(char**)b);
}

TrieNode *trie_new_node(){
    TrieNode *node=calloc(1, sizeof(TrieNode));
    if(!node){
        fprintf(stderr, "lsh: allocation error\n");
        exit(EXIT_FAILURE);
    }
    return node;
}
void trie_insert(TrieNode *root,char *word){
    TrieNode *current = root;
    int i;
    for(i=0;word[i]!='\0';i++){
        int idx = (unsigned char)word[i];
        if(current->children[idx]==NULL){
            current->children[idx]=trie_new_node();

        }
        current=current->children[idx];
    }
    current->is_end=1;
}

void trie_collect(TrieNode *node,char *prefix, char **results, int *count){
    if(*count >= 2048) return;
    if(node->is_end){
        results[*count]=strdup(prefix);
        (*count)++;
    }
    int i;
    for(i=0;i<128;i++){
        if(node->children[i]!=NULL){
            int len = strlen(prefix);
            char new_prefix[1024];
            strncpy(new_prefix,prefix,1024);
            new_prefix[len]=(char)i;
            new_prefix[len+1]='\0';
            trie_collect(node->children[i], new_prefix, results, count);
        }
    }
}



void trie_search(TrieNode *root, char *prefix, char **results, int *count){
    TrieNode *current=root;
    int i;
    for(i=0;prefix[i]!='\0';i++){
        int idx=(unsigned)prefix[i];
        if(current->children[idx]==NULL)
        {
            return;
        }
        current = current->children[idx];
    }
    trie_collect(current,prefix,results,count);
}

int lsh_is_background(char **args){
    int i=0;
    while(args[i]!=NULL){i++;}
    if(i>0 &&strcmp(args[i-1],"&")==0){
        args[i-1]=NULL;
        return 1;
    }
    return 0;
}

void load_commands(TrieNode *root){
    char *path = getenv("PATH");
    char *path_copy = strdup(path);
    char *dir = strtok(path_copy, ":");
    while(dir != NULL){
        DIR *d = opendir(dir);
        if(d){
            struct dirent *entry;
            while((entry = readdir(d)) != NULL){
                if(strcmp(entry->d_name, ".") == 0 ||
                   strcmp(entry->d_name, "..") == 0) continue;
                
                // debug — check if ls is found
                if(strcmp(entry->d_name, "ls") == 0){
                }
                
                struct stat st;
                char fullpath[1024];
                snprintf(fullpath, sizeof(fullpath), "%s/%s", dir, entry->d_name);
                if(stat(fullpath, &st) == 0){
                    if(st.st_mode & S_IXUSR){
                        if(strcmp(entry->d_name, "ls") == 0){
                        }
                        trie_insert(root, entry->d_name);
                    }
                } else {
                    if(strcmp(entry->d_name, "ls") == 0){
                        perror("stat");
                    }
                }
            }
            closedir(d);
        } else {
            printf("Could not open: %s\n", dir);
        }
        dir = strtok(NULL, ":");
    }
    free(path_copy);
}

int lsh_find_pipe(char **args){
    int i;
    for(i=0;args[i]!=NULL;i++){
        if(strcmp(args[i],"|")==0){
            return i;
        }
    }
    return -1;

}

void lsh_split_pipe(char **args,int pipe_index,char **left,char **right){
    int i;
    int pos=0;
    for(i=0;i<pipe_index;i++){
        left[i]=args[i];
    }
    left[i]=NULL;
    for(i=pipe_index+1;args[i]!=NULL;i++){
        right[pos++]=args[i];
    }
    right[pos]=NULL;
}
//refactored the redirections to a whole new fn
int lsh_handle_redirections(char **args) {
    int i = 0;
    int first_redir_index = -1;

    while (args[i] != NULL) {
        if (strcmp(args[i], ">") == 0 || strcmp(args[i], ">>") == 0 || strcmp(args[i], "<") == 0) {

            if (first_redir_index == -1) first_redir_index = i;

            if (args[i+1] == NULL) {
                fprintf(stderr, "lsh: No filename after %s\n", args[i]);
                return -1;
            }

            int fd;
            if (strcmp(args[i], ">") == 0) {
                fd = open(args[i+1], O_WRONLY | O_CREAT | O_TRUNC, 0644);
                if(fd<0){ perror("lsh"); exit(EXIT_FAILURE);}
                dup2(fd, STDOUT_FILENO);
                close(fd);
            } else if (strcmp(args[i], ">>") == 0) {
                fd = open(args[i+1], O_WRONLY | O_CREAT | O_APPEND, 0644);
                if(fd<0){ perror("lsh"); exit(EXIT_FAILURE);} 
                dup2(fd, STDOUT_FILENO);
                close(fd);
            } else if (strcmp(args[i], "<") == 0) {
                fd = open(args[i+1], O_RDONLY);
                if(fd<0){ perror("lsh"); exit(EXIT_FAILURE);} 
                dup2(fd, STDIN_FILENO);
                close(fd);
            }

        }
        i++;
    }

    if (first_redir_index != -1) {
        args[first_redir_index] = NULL;
    }

    return 0;
}




int lsh_num_builtins() {
    return sizeof(builtin_str) / sizeof(char *);
}
int lsh_history(char **args){
    int i;
    for(i=0;i<history_count && i<history_size;i++){
        printf("%d) %s\n",i + 1,history[i]);
    }
    return 1;
}



int lsh_cd(char **args)
{
    if (args[1] == NULL) {
        fprintf(stderr, "lsh: expected argument to \"cd\"\n");
    } else {
        if (chdir(args[1]) != 0) {
            perror("lsh");
        }
    }
    return 1;
}

/**
   @brief Builtin command: print help.
   @param args List of args.  Not examined.
   @return Always returns 1, to continue executing.
 */
int lsh_help(char **args)
{
    int i;
    printf("Aswin's Shell LSH\n");
    printf("Type program names and arguments, and hit enter.\n");
    printf("The following are built in:\n");

    for (i = 0; i < lsh_num_builtins(); i++) {
        printf("  %s\n", builtin_str[i]);
    }

    printf("Use the man command for information on other programs.\n");
    return 1;
}

int lsh_exit(char **args)
{
    return 0;
}

int lsh_launch(char **args)
{
    int pipe_index = lsh_find_pipe(args);
    if(pipe_index != -1){
        char **left = malloc(sizeof(char*) * LSH_RL_BUFSIZE);
        char **right = malloc(sizeof(char*) * LSH_RL_BUFSIZE);
        lsh_split_pipe(args, pipe_index, left, right);

        int pipefd[2];
        if(pipe(pipefd) == -1){
            perror("lsh");
            return 1;
        }

        pid_t pid1 = fork();
        if(pid1 < 0){ perror("lsh"); return 1; }
        if(pid1 == 0){
            close(pipefd[0]);
            dup2(pipefd[1], STDOUT_FILENO);
            lsh_handle_redirections(left);
            execvp(left[0], left);
            perror("lsh");
            exit(EXIT_FAILURE);
        }

        pid_t pid2 = fork();
        if(pid2 < 0){ perror("lsh"); return 1; }
        if(pid2 == 0){
            close(pipefd[1]);
            dup2(pipefd[0], STDIN_FILENO);
            close(pipefd[0]);
            lsh_handle_redirections(right);        

            execvp(right[0], right);     
            perror("lsh");
            exit(EXIT_FAILURE);
        }

        close(pipefd[0]);
        close(pipefd[1]);
        waitpid(pid1, NULL, 0);
        waitpid(pid2, NULL, 0);
        free(left);
        free(right);
        return 1;
    }
    int is_background=lsh_is_background(args);
    pid_t pid, wpid;
    int status;

    pid = fork();
    if (pid == 0) {
        lsh_handle_redirections(args);        
        if (execvp(args[0], args) == -1) {

            perror("lsh");
        }
        exit(EXIT_FAILURE);
    } else if (pid < 0) {
        // Error forking
        perror("lsh");
    } else {
        // Parent process
        if(is_background){printf("[background] pid: %d\n",pid);}
        else{
            do {
                wpid = waitpid(pid, &status, WUNTRACED);
            } while (!WIFEXITED(status) && !WIFSIGNALED(status));
        }
    }

    return 1;
}


int lsh_execute(char **args)
{
    int i;

    if (args[0] == NULL) {
        // An empty command was entered.
        return 1;
    }
     update_freq(args[0]);


    for (i = 0; i < lsh_num_builtins(); i++) {
        if (strcmp(args[0], builtin_str[i]) == 0) {
            return (*builtin_func[i])(args);
        }
    }
       return lsh_launch(args);
}



void show_ghost(char *prefix, TrieNode *root, char *ghost){
    char **results = malloc(sizeof(char*)*2048);
    int count = 0;
    trie_search(root, prefix, results,&count);
    if(count >1){
        qsort(results,count,sizeof(char*),compare_freq);
    }

    if(count > 0){
        char *suggestion =results[0];
        char *ghost_part = suggestion + strlen(prefix);
        if(strlen(ghost_part)==0){
            ghost[0]='\0';
        }
        else{
            strncpy(ghost, ghost_part,1023);

            printf("\033[2m%s\033[0m", ghost_part);
            printf("\033[%dD",(int)strlen(ghost_part));
            fflush(stdout);
        }
    }
    else{
        ghost[0] = '\0';
    }
    int j;
    for(j=0;j<count;j++) free(results[j]);
    free(results);

}






char *lsh_read_line_raw(TrieNode *root){
    enable_raw_mode();  
    int bufsize = LSH_RL_BUFSIZE;
    int position = 0;
    char *buffer = malloc(sizeof(char) * bufsize);
    char *saved_buffer = malloc(sizeof(char) * bufsize);
    if(!buffer || !saved_buffer){
        fprintf(stderr, "lsh: allocation error\n");
        exit(EXIT_FAILURE);
    }
    saved_buffer[0]='\0';
    buffer[0]='\0';
    int c;
    int  history_index=history_count;
    char ghost[1024];
    ghost[0]='\0';


    printf("> ");
    fflush(stdout);
    while (1) {
        // Read a character
        c = getchar();

        // If we hit EOF, replace it with a null character and return.
        if (c == EOF || c == '\n') {
            if(strlen(ghost) > 0){
                printf("\033[2m%*s\033[0m", (int)strlen(ghost), "");
                ghost[0] = '\0';
            }
            disable_raw_mode();
            printf("\n");
            buffer[position] = '\0';
            free(saved_buffer);
            return buffer;

        }
        else if(c==127){
            ghost[0] = '\0';
            if(position>0){
                position--;
                buffer[position]='\0';
                printf("\r\033[K> %s",buffer);
                fflush(stdout);
                if(position>0){
                    show_ghost(buffer, root, ghost);
                }
            }
        }
        else if(c==27){
            char seq[2];
            seq[0]=getchar();
            seq[1]=getchar();
            if(seq[0]=='['){
                if(seq[1] == 'A'){
                    if(history_index>0){
                        if(history_index == history_count){
                            strncpy(saved_buffer, buffer, bufsize);
                        }
                        history_index--;
                        strncpy(buffer, history[history_index],bufsize);
                        position = strlen(buffer);
                        printf("\r\033[K> %s", buffer);
                        fflush(stdout);
                    }
                }

                else if(seq[1] == 'B'){
                    if(history_index<history_count){
                        history_index++;
                        if(history_index==history_count){
                            strncpy(buffer, saved_buffer, bufsize);
                        }
                        else{
                            strncpy(buffer, history[history_index], bufsize);
                        }
                        position =strlen(buffer);
                        printf("\r\033[K> %s",buffer);
                        fflush(stdout);
                    }
                }

            }
        }
        else if(c == 9){
            if(strlen(ghost) > 0){
                // clear ghost display
                printf("\033[2m%*s\033[0m", (int)strlen(ghost), "");
                printf("\033[%dD", (int)strlen(ghost));

                // append ghost to buffer
                strncpy(buffer + position, ghost, bufsize - position);
                position += strlen(ghost);
                ghost[0] = '\0';

                // reprint accepted text
                // simpler — just reprint whole line
                printf("\r\033[K> %s", buffer);
                fflush(stdout);
            }
        }
        else {
            if(strlen(ghost)>0){
                printf("\033[2m%*s\033[0m", (int)strlen(ghost), "");
                printf("\033[%dD", (int)strlen(ghost));
                ghost[0] = '\0';
            }
            buffer[position] = c;
            position++;
            buffer[position] = '\0';
            printf("%c", c);
            fflush(stdout);

            // show new ghost
            show_ghost(buffer, root, ghost);
        }
        if (position >= bufsize) {
            bufsize += LSH_RL_BUFSIZE;
            buffer = realloc(buffer, bufsize);
            if (!buffer) {
                fprintf(stderr, "lsh: allocation error\n");
                exit(EXIT_FAILURE);
            }
        }

    }

    // If we have exceeded the buffer, reallocate.
}




#define LSH_TOK_BUFSIZE 64
#define LSH_TOK_DELIM " \t\r\n\a"
/**
   @brief Split a line into tokens (very naively).
   @param line The line.
   @return Null-terminated array of tokens.
 */
char **lsh_split_line(char *line)
{
    int bufsize = LSH_TOK_BUFSIZE, position = 0;
    char **tokens = malloc(bufsize * sizeof(char*));
    char *token =  malloc(LSH_RL_BUFSIZE);
    int token_pos=0;
    int in_quotes =0;
    int i=0;
    if (!tokens || !token) {
        fprintf(stderr, "lsh: allocation error\n");
        exit(EXIT_FAILURE);
    }

    while (line[i] != '\0') {
        char c = line[i];    
        if( c=='"'){
            in_quotes=!in_quotes;

        }
        else if(c==' ' && !in_quotes){
            if(token_pos > 0){
                token[token_pos]='\0';
                tokens[position++] = strdup(token);
                token_pos = 0;
                if(position >= bufsize){
                    bufsize+=LSH_TOK_BUFSIZE;
                    tokens = realloc(tokens,bufsize*sizeof(char*));
                }
            }
        }
        else{
            token[token_pos++] = c;
        }
        i++;
    }
    if(token_pos > 0){
        token[token_pos] = '\0';
        tokens[position++]=strdup(token);
    }
    tokens[position]=NULL;
    free(token);
    return tokens;
}
/**
   @brief Loop getting input and executing it.
 */
void lsh_loop(TrieNode *root)
{
    char *line;
    char **args;
    int status;

    do {

        line = lsh_read_line_raw(root);
        if (flag){
            printf("\n[done] pid: %d\n",(int)done_pid);
            flag=0;
            done_pid=0;
        }
        if(strlen(line)>0){
            int slot = history_count % history_size;
            if(history[slot]!=NULL){
                free(history[slot]);
            }
            history[slot]=strdup(line);
            history_count++;
        }
        args = lsh_split_line(line);
        status = lsh_execute(args);

        free(line);
        free(args);
    } while (status);
}

/**
   @brief Main entry point.
   @param argc Argument count.
   @param argv Argument vector.:wq
   @return status code
 */
int main(int argc, char **argv)
{
    load_freq();
    atexit(save_freq);
    // Load config files, if any.
    TrieNode *root = trie_new_node();
    load_commands(root);
    int i;
    for(i = 0; i < lsh_num_builtins(); i++){
        trie_insert(root, builtin_str[i]);
    }
    
    signal(SIGCHLD,sigchld_handler);    
    // Run command loop.
    lsh_loop(root);

    // Perform any shutdown/cleanup.

    return EXIT_SUCCESS;
}

