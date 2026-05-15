# aish — Line by Line Explained Notes
> Every line explained so I understand WHY, not just WHAT.
> Syntax I can look up. Understanding I need to keep.

---

## 1. Signal Handlers

```c
volatile sig_atomic_t got_sigint = 0;
```
- `volatile` — tells the compiler "don't cache this in a register, always read it fresh from memory". Without this, the compiler might optimise the variable away and your main loop never sees the change the signal handler made.
- `sig_atomic_t` — a special type guaranteed to be readable/writable in one CPU instruction. This matters because a signal can interrupt your code at ANY point mid-execution. If updating the variable took 2 instructions and signal fired between them, you'd get garbage. `sig_atomic_t` makes it safe.
- `got_sigint = 0` — starts as false. Handler sets it to 1. Main loop checks it.

```c
void sigint_handler(int sig) {
    got_sigint = 1;
}
```
- The parameter `int sig` is mandatory — the OS passes the signal number here. You don't have to use it but you must accept it.
- **Why only set a flag and nothing else?** Signal handlers interrupt your program at a random point. If you call `printf` or `malloc` inside a handler, and your program was already inside `printf` or `malloc` when the signal arrived, you get corruption. These functions are not "re-entrant safe". So the rule is: handler only sets a flag. Main loop does the real work.

```c
signal(SIGINT, sigint_handler);
```
- `signal()` — registers your handler with the OS. Now when Ctrl+C is pressed, OS calls your function instead of killing the process.
- `SIGINT` — the signal sent by Ctrl+C. Name means "interrupt signal".
- Second argument is your handler function. You pass the function name without `()` because you're passing the address of the function, not calling it.

---

## 2. Raw Mode Terminal

```c
struct termios orig_termios;
```
- `termios` is a struct the OS uses to store all terminal settings — echo, line buffering, special characters, etc.
- We store the original settings globally so we can restore them when the shell exits. If we don't restore, the terminal stays broken after our program ends.

```c
void enable_raw_mode() {
    tcgetattr(STDIN_FILENO, &orig_termios);
```
- `tcgetattr` — "terminal control get attributes". Reads current terminal settings into our struct.
- `STDIN_FILENO` — file descriptor 0, which is standard input (keyboard). We're asking about the terminal attached to keyboard input.
- `&orig_termios` — passing the address so the function can write into our struct.

```c
    struct termios raw = orig_termios;
```
- Copy the original settings into a new struct called `raw`. We modify `raw`, not `orig_termios`. This way we always have the originals safe.

```c
    raw.c_lflag &= ~(ECHO | ICANON);
```
- `c_lflag` — "local flags", controls local terminal behaviour.
- `ECHO` — the flag that makes terminal automatically print what you type. We turn it OFF because we want to control what gets printed ourselves (that's how we show ghost text).
- `ICANON` — "canonical mode", means terminal waits for Enter before sending input. We turn it OFF so we get each keypress immediately without waiting for Enter.
- `&= ~(...)` — bitwise operation to turn specific bits OFF without touching other bits. `~` flips all bits, `&=` keeps only bits that are 1 in both. This is the standard C idiom for "clear these specific flags".

```c
    tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw);
```
- `tcsetattr` — "terminal control set attributes". Applies our modified settings.
- `TCSAFLUSH` — "when to apply": after flushing (discarding) any pending input. This ensures we start clean.

```c
void disable_raw_mode() {
    tcsetattr(STDIN_FILENO, TCSAFLUSH, &orig_termios);
}
```
- Exact same call but passing back the original settings. Restores terminal to how it was before we touched it.
- This is called in `atexit()` so it runs automatically when the shell exits, even if it crashes.

---

## 3. ANSI Escape Codes

These are special sequences that terminals understand as commands, not text to print.

```
\033
```
- This is the ESC character (decimal 27, octal 033). Every ANSI sequence starts with this. The terminal sees ESC and knows "a command follows".

```
\033[
```
- ESC followed by `[` starts a "CSI sequence" (Control Sequence Introducer). Most visual commands use this prefix.

```c
"\033[34m"   // blue text
"\033[32m"   // green text
"\033[31m"   // red text
"\033[33m"   // yellow text
"\033[0m"    // reset — turn off ALL formatting
"\033[1m"    // bold
"\033[2m"    // dim — used for ghost text because it looks faded
```
- The number before `m` is the colour/style code. `m` means "set graphics mode". Just memorise: 0=reset, 1=bold, 2=dim, 31-37=colours.

```c
"\033[%dD"   // move cursor LEFT by %d characters
"\033[%dC"   // move cursor RIGHT by %d characters
```
- `D` = move left (think: D for Decreasing column number)
- `C` = move right (think: C for... just memorise it honestly)
- Used after printing ghost text to move cursor back so next keypress appears at the right position.

```c
"\033[s"     // SAVE cursor position
"\033[u"     // RESTORE cursor position (go back to saved position)
```
- Used when drawing the dropdown — save where cursor is, draw dropdown below, restore to where user is typing.

```c
"\r\033[K"   // carriage return + clear to end of line
```
- `\r` — carriage return, moves cursor to START of current line (not a new line).
- `\033[K` — "erase from cursor to end of line". Together they wipe the current line completely so we can reprint it fresh. Used every time buffer changes.

---

## 4. Fork + Exec

```c
pid_t pid = fork();
```
- `fork()` creates an exact copy of the current process. Both parent and child continue from this line.
- Returns different values: in parent it returns the child's PID (a positive number). In child it returns 0. On failure returns -1.
- `pid_t` is just a type for process IDs — basically an int.

```c
if (pid == 0) {
```
- We're inside the CHILD process. The child is a copy of the shell, and we're about to replace it with the actual command.

```c
    execvp(args[0], args);
```
- `execvp` — "execute with path search, passing vector of arguments".
- Replaces the ENTIRE current process with the program named in `args[0]`. The child process stops being a shell and becomes `ls` or `gcc` or whatever.
- `args[0]` — the program name.
- `args` — the full argument array including args[0]. execvp expects this format.
- The `p` in `execvp` means it searches PATH automatically. So you can write `"gcc"` not `"/usr/bin/gcc"`.

```c
    perror("lsh");
    exit(EXIT_FAILURE);
```
- **These lines only run if `execvp` failed.** On success, execvp never returns — the process is replaced. If we reach here, something went wrong (command not found, no permission, etc.).
- `perror` prints the error with the system's description appended automatically.
- `exit(EXIT_FAILURE)` — child must exit here. If we don't, the child would continue as a second shell. That's very bad.

```c
} else if (pid < 0) {
    perror("lsh");
```
- `fork()` itself failed. Rare but possible (too many processes running). Just report the error.

```c
} else {
    waitpid(pid, &status, 0);
}
```
- We're in the PARENT (the shell).
- `waitpid` — parent blocks here until the child process finishes. Without this, the shell would immediately print a new prompt while the child is still running.
- First arg: which child to wait for (the pid we got from fork).
- Second arg: pointer to int where exit status gets written. We can check this later.
- Third arg: flags. 0 means "block until child exits" (simple wait). `WNOHANG` would mean "check but don't block" (used in SIGCHLD handler).

---

## 5. Pipes

```c
int pipefd[2];
pipe(pipefd);
```
- `pipe()` creates a communication channel — data written to one end comes out the other.
- `pipefd[1]` — write end. Write data here.
- `pipefd[0]` — read end. Read data from here.
- Think of it as a physical pipe: stuff goes in one end, comes out the other.

```c
// In left command's child:
dup2(pipefd[1], STDOUT_FILENO);
close(pipefd[0]);
```
- `dup2(old, new)` — makes file descriptor `new` point to the same place as `old`. After this, writing to `STDOUT_FILENO` (fd 1) is the same as writing to `pipefd[1]` (pipe write end). So when the command does `printf`, it goes into the pipe.
- `close(pipefd[0])` — child doesn't need the read end. Close it. **This is important** — if you don't close unused ends, the reader on the other side never gets EOF and hangs forever.

```c
// In right command's child:
dup2(pipefd[0], STDIN_FILENO);
close(pipefd[1]);
```
- Makes `STDIN_FILENO` (fd 0) read from the pipe. Now when the command reads from stdin, it gets what the left command wrote.
- Close the write end — right command doesn't write to pipe.

```c
// In parent after forking both:
close(pipefd[0]);
close(pipefd[1]);
```
- Parent also must close BOTH ends. Parent doesn't use the pipe at all.
- **Why close in parent?** The read end stays open as long as ANY process has it open. If parent keeps write end open, reader never gets EOF even after left command exits. Always close all pipe ends in every process that doesn't use them.

---

## 6. getcwd and HOME Shortening

```c
char cwd[1024];
getcwd(cwd, sizeof(cwd));
```
- `getcwd` — "get current working directory". Writes the path into our buffer.
- First arg: buffer to write into.
- Second arg: max size to write. Prevents overflow.
- Returns NULL if path is longer than buffer (very deep directories). Should handle this case.

```c
char *home = getenv("HOME");
```
- `getenv` — reads an environment variable. Returns `char*` to the value, or `NULL` if not set.
- **Always check for NULL before using.** `getenv("HOME")` is almost always set, but defensive code checks anyway.

```c
if (strncmp(cwd, home, strlen(home)) == 0) {
    printf("~%s", cwd + strlen(home));
} else {
    printf("%s", cwd);
}
```
- `strncmp` — compare first N characters of two strings. We compare only `strlen(home)` characters — checking if cwd STARTS WITH the home path.
- Returns 0 if equal.
- `cwd + strlen(home)` — pointer arithmetic. We skip past the HOME part of the path. If cwd is `/home/aswin/projects` and home is `/home/aswin`, then `cwd + strlen(home)` points at `/projects`. We prepend `~` to get `~/projects`.

---

## 7. Trie — How Mine Works

```c
typedef struct TrieNode {
    struct TrieNode *children[128];
    int is_end;
} TrieNode;
```
- Each node has 128 child pointers — one for each ASCII character (index = ASCII value of character).
- `is_end` — marks if a complete word ends at this node.
- Most children will be NULL (most characters don't follow a given character).

```
Inserting "cd":
root → children['c'] → children['d'] → is_end = 1

Inserting "cat":
root → children['c'] → children['a'] → children['t'] → is_end = 1
                    ↑
            shared with "cd" — same 'c' node
```

```c
void trie_insert(TrieNode *root, char *word) {
    TrieNode *current = root;
    for (int i = 0; word[i] != '\0'; i++) {
        int idx = (unsigned char)word[i];
```
- `(unsigned char)` — cast to unsigned before using as array index. Some chars have negative values when treated as signed. This prevents negative array indexing (which is undefined behaviour and causes crashes).

```c
        if (current->children[idx] == NULL) {
            current->children[idx] = trie_new_node();
        }
        current = current->children[idx];
    }
    current->is_end = 1;
}
```
- Walk down the trie, creating nodes that don't exist yet.
- After the loop, mark the current node as a word ending.

**Search:** walk to the node matching the last character of the prefix. Then collect all words below that node. Those are all completions.

---

## 8. Frequency System

```c
typedef struct {
    char command[256];
    int count;
} CommandFreq;

CommandFreq freq_table[1000];
int freq_count = 0;
```
- Simple flat array of structs. Not the most efficient but plenty fast for 1000 commands.
- `freq_count` tracks how many entries are actually used.

```c
void update_freq(char *cmd) {
    for (int i = 0; i < freq_count; i++) {
        if (strcmp(freq_table[i].command, cmd) == 0) {
            freq_table[i].count++;
            return;   // found it, done
        }
    }
    // not found — add new entry
    strncpy(freq_table[freq_count].command, cmd, 255);
    freq_table[freq_count].count = 1;
    freq_count++;
}
```
- Linear search through the table. Fine for 1000 entries.
- `strncpy(dest, src, n)` — copy at most n characters. Prevents overflow. Note: doesn't guarantee null terminator if src is longer than n — always set `[n] = '\0'` manually to be safe.

```c
int compare_freq(const void *a, const void *b) {
    char *cmd_a = *(char**)a;
    char *cmd_b = *(char**)b;
```
- `qsort` passes pointers to the elements. Since our array is `char**`, each element is a `char*`. So what qsort gives us is a `char**`. We dereference to get the actual string.
- `const void *` — qsort's generic signature. We cast to the type we know.

```c
    return freq_b - freq_a;  // higher freq first (descending)
}
```
- If freq_b > freq_a, returns positive → b comes first → descending order.
- Opposite of normal sort (which returns a - b for ascending).

---

## 9. History — Simple Linear

```c
char *history[100] = {NULL};
int history_count = 0;
```
- Array of 100 string pointers. All start as NULL.
- `history_count` tracks how many commands have been stored.

```c
// storing a new command:
if (history_count < history_size) {
    history[history_count] = strdup(line);
    history_count++;
}
```
- `strdup` — allocates new memory and copies the string. We need this because `line` gets freed after execution. Without strdup, we'd store a pointer to freed memory.
- Stop at 100. Simple. No circular buffer needed for personal use.

```c
// displaying:
for (int i = 0; i < history_count; i++) {
    printf("%d) %s\n", i + 1, history[i]);
}
```
- Oldest first (index 0), newest last. Exactly what you'd expect.

```c
// up arrow navigation:
if (history_index > 0) {
    history_index--;
    strncpy(buffer, history[history_index], bufsize);
}

// down arrow:
if (history_index < history_count) {
    history_index++;
    if (history_index == history_count) {
        strncpy(buffer, saved_buffer, bufsize); // restore what was being typed
    } else {
        strncpy(buffer, history[history_index], bufsize);
    }
}
```
- `saved_buffer` stores what the user was typing before they pressed up. So pressing down all the way restores their original input.

---

## 10. lsh_split_line — Tokenizer

```c
char **lsh_split_line(char *line) {
```
- Returns `char**` — an array of strings (each string is one token/word).

```c
    while (line[i] != '\0') {
        char c = line[i];

        if (c == '"') {
            in_quotes = !in_quotes;   // toggle quote mode
        }
        else if (c == ' ' && !in_quotes) {
```
- Space splits tokens ONLY when not inside quotes. This handles `echo "hello world"` as one argument.

```c
            if (token_pos > 0) {
                token[token_pos] = '\0';
                tokens[position++] = strdup(token);
                token_pos = 0;
            }
        }
        else {
            token[token_pos++] = c;   // add character to current token
        }
    }
```
- Build current token character by character. When we hit a space (outside quotes), flush the token to the array and start fresh.

**What's missing (to fix):** operators like `>`, `<`, `|` need to be their own tokens even without spaces. `ls>file` should become `["ls", ">", "file"]`. Fix: in the else branch, check if `c` is an operator character. If yes, flush current token, add operator as its own token.

---

## 11. Memory — Things That Bite

```c
// strdup allocates — you must free it later
char *s = strdup("hello");
// ... use s ...
free(s);   // if you forget this = memory leak

// realloc can return NULL on failure
char *new_buf = realloc(buffer, new_size);
if (!new_buf) {
    // handle error — buffer is still valid here
    // if you do buffer = realloc(...) directly and it fails,
    // you lose the original pointer = memory leak + crash
}
buffer = new_buf;   // only assign after checking

// strncpy does NOT null terminate if source >= n
strncpy(dest, src, 255);
dest[255] = '\0';   // always add this manually
```

---

## 12. Things I Always Forget

| What | Why |
|------|-----|
| `dup2(old, new)` — first arg is SOURCE | Think: "copy old into new" |
| Close ALL unused pipe ends | Reader hangs if any writer fd stays open anywhere |
| Signal handler: set flag ONLY | printf/malloc inside handler = corruption |
| `execvp` never returns on success | Error handling goes right after the call |
| `WNOHANG` in waitpid = non-blocking | Used in SIGCHLD handler, not in foreground wait |
| `strncpy` doesn't null terminate | Always add `[n] = '\0'` after |
| Check `getenv` for NULL | PATH, HOME can theoretically be unset |
| `realloc` return value check | Assign to temp, check, then assign to real pointer |
| `strdup` needs `free` later | Every strdup = a malloc you own |

---

## 13. Compile and Debug

```bash
# compile with all warnings — fix every warning before moving on
gcc new.c -o aish -Wall -Wextra

# check for memory leaks and invalid reads
valgrind --leak-check=full --track-origins=yes ./aish

# see what system calls your shell makes (advanced debugging)
strace ./aish
```

**Warnings are errors waiting to happen.** `-Wall -Wextra` catches most of them. Never ignore a warning and move on — it will become a bug later.

---

## 14. How to Think About Debugging

When something breaks, ask in this order:

1. **What did I just change?** 90% of bugs are in the last thing you touched.
2. **What is the actual value vs expected value?** Add a `printf` to see it.
3. **Is memory involved?** If yes, run valgrind before guessing.
4. **Am I reading freed memory?** Classic shell bug — storing pointer to line after `free(line)`.
5. **Signal involved?** Check if global flags are being set/cleared correctly.

---
*Add new things here as I learn them. This is a living document.*
