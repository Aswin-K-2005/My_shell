
#include <sys/wait.h>
#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <fcntl.h>

#define LSH_RL_BUFSIZE 1024
#define history_size 100
char *history[history_size]= {NULL};
int history_count =0;
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
    do {
      wpid = waitpid(pid, &status, WUNTRACED);
    } while (!WIFEXITED(status) && !WIFSIGNALED(status));
  }

  return 1;
}

/**
   @brief Execute shell built-in or launch program.
   @param args Null terminated list of arguments.
   @return 1 if the shell should continue running, 0 if it should terminate
 */
int lsh_execute(char **args)
{
  int i;

  if (args[0] == NULL) {
    // An empty command was entered.
    return 1;
  }

  for (i = 0; i < lsh_num_builtins(); i++) {
    if (strcmp(args[0], builtin_str[i]) == 0) {
      return (*builtin_func[i])(args);
    }
  }

  return lsh_launch(args);
}

/**
   @brief Read a line of input from stdin.
   @return The line from stdin.
 */
char *lsh_read_line(void)
{
  int bufsize = LSH_RL_BUFSIZE;
  int position = 0;
  char *buffer = malloc(sizeof(char) * bufsize);
  int c;

  if (!buffer) {
    fprintf(stderr, "lsh: allocation error\n");
    exit(EXIT_FAILURE);
  }

  while (1) {
    // Read a character
    c = getchar();

    // If we hit EOF, replace it with a null character and return.
    if (c == EOF || c == '\n') {
      buffer[position] = '\0';
      return buffer;
    } else {
      buffer[position] = c;
    }
    position++;

    // If we have exceeded the buffer, reallocate.
    if (position >= bufsize) {
      bufsize += LSH_RL_BUFSIZE;
      buffer = realloc(buffer, bufsize);
      if (!buffer) {
        fprintf(stderr, "lsh: allocation error\n");
        exit(EXIT_FAILURE);
      }
    }
  }
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
  char *token;

  if (!tokens) {
    fprintf(stderr, "lsh: allocation error\n");
    exit(EXIT_FAILURE);
  }

  token = strtok(line, LSH_TOK_DELIM);
  while (token != NULL) {
    tokens[position] = token;
    position++;

    if (position >= bufsize) {
      bufsize += LSH_TOK_BUFSIZE;
      tokens = realloc(tokens, bufsize * sizeof(char*));
      if (!tokens) {
        fprintf(stderr, "lsh: allocation error\n");
        exit(EXIT_FAILURE);
      }
    }

    token = strtok(NULL, LSH_TOK_DELIM);
  }
  tokens[position] = NULL;
  return tokens;
}

/**
   @brief Loop getting input and executing it.
 */
void lsh_loop(void)
{
  char *line;
  char **args;
  int status;

  do {
    printf("> ");
    line = lsh_read_line();
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
  // Load config files, if any.

  // Run command loop.
  lsh_loop();

  // Perform any shutdown/cleanup.

  return EXIT_SUCCESS;
}

