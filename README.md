# aish 🐚
### Aswin's Intelligent Shell

A Unix shell built from scratch in C, evolving into a fully AI-powered intelligent terminal.

> Built by [Aswin K](https://github.com/Aswin-K-2005) — Legion 5i Pro + i9 + RTX 4070Ti doing the heavy lifting.

---

## What is aish?

Most people use a shell without understanding what happens when they press Enter. aish is built from zero using raw C and POSIX system calls — no shortcuts, no libraries. Every feature you see was written line by line.

The end goal: a standalone AI-powered terminal (like Warp) with a local LLM that explains errors, suggests commands, and understands natural language — running entirely offline.

---

## Current features

| Feature | Status |
|---------|--------|
| Command execution | ✅ |
| Builtins — cd, help, history, exit | ✅ |
| Output redirection `>` | ✅ |
| Append redirection `>>` | ✅ |
| Input redirection `<` | ✅ |
| Piping `\|` | ✅ |
| Pipe + redirection combined | ✅ |
| Background jobs `&` | ✅ |
| Job completion notification | ✅ |
| Raw mode + arrow key history | ✅ |
| Tab autocomplete (Trie) | 🔨 in progress |
| AI error explanation | 🔜 |
| Natural language → command | 🔜 |
| Qt GUI (Warp-style) | 🔜 |

---

## Built with

- **Language** — C
- **Core syscalls** — `fork()`, `execvp()`, `waitpid()`, `dup2()`, `pipe()`, `open()`
- **Terminal** — `termios` raw mode, ANSI escape codes
- **Signals** — `SIGCHLD` for background job notifications
- **No external libraries** — pure C and POSIX

---

## Build and run

```bash
# clone
git clone https://github.com/Aswin-K-2005/My_shell.git
cd My_shell

# compile
gcc shell.c -o shell

# run
./shell
```

---

## Usage

```bash
# basic commands
> ls -la
> cd /tmp && pwd

# redirection
> ls > out.txt          # write to file
> echo hello >> out.txt # append to file
> sort < out.txt        # read from file

# piping
> ls | grep .c
> ls | grep .c > out.txt

# background jobs
> sleep 5 &
[background] pid: 1234
>                        # prompt returns immediately
[done] pid: 1234         # notified when job finishes

# history
> history               # view last 100 commands
# press ↑ ↓             # navigate history
```

---

## Architecture

```
you type a command
       │
       ▼
lsh_read_line_raw()     raw mode input — handles arrow keys,
                        backspace, history navigation
       │
       ▼
lsh_split_line()        tokenizes into argv[] using strtok
       │
       ▼
lsh_execute()           checks builtins or external command
      / \
     /   \
builtin   external
  │          │
runs       fork()
directly     ├── child → lsh_handle_redirections() → execvp()
             └── parent → waitpid() or background
```

---

## How key features work

### Piping
```
ls | grep .c

fork child 1 → dup2(pipefd[1], stdout) → execvp ls
fork child 2 → dup2(pipefd[0], stdin)  → execvp grep
parent → close both ends → waitpid both children
```

### Redirection
```
ls > out.txt

open("out.txt", O_WRONLY | O_CREAT | O_TRUNC)
dup2(fd, STDOUT_FILENO)
execvp ls → output goes to file instead of terminal
```

### Background jobs
```
sleep 5 &

fork() → parent does NOT waitpid → returns to prompt
SIGCHLD handler fires when child dies → sets flag
lsh_loop prints [done] notification between commands
```

### Raw mode
```
termios turns off ECHO and ICANON
every keypress arrives immediately
ESC [ A → up arrow → navigate history
ESC [ B → down arrow
127     → backspace → redraw line
```

---

## Project structure

```
My_shell/
├── shell.c       # entire shell — ~500 lines of C
└── README.md
```

---

## Roadmap 🚀

### Phase 2 — Shell polish (current)
- [x] Raw mode + arrow key history
- [ ] Tab autocomplete with Trie data structure
- [ ] `&&` and `||` operators
- [ ] Startup mode selector (work/chill/nothing)
  - work → opens last project + neovim + spotify
  - chill → opens YouTube in Brave

### Phase 3 — Local AI (next)
- [ ] ollama + CodeLlama 7B running locally
- [ ] Capture stderr from failed commands
- [ ] AI explains errors in plain English
- [ ] Natural language → shell command translation
- [ ] Context-aware command suggestions

### Phase 4 — Fine-tuning
- [ ] Collect shell command + error dataset
- [ ] Fine-tune with LoRA on RTX 4070Ti
- [ ] Quantize to 4-bit for fast CPU inference
- [ ] Ship model bundled with aish

### Phase 5 — Qt GUI
- [ ] Standalone terminal window (Qt6/C++)
- [ ] Command blocks — grouped input/output like Warp
- [ ] AI explanation panel alongside terminal
- [ ] Syntax highlighting in input
- [ ] Startup mode selector UI

---

## What I learned building this

| Concept | Where it's used |
|---------|----------------|
| `fork()` + `execvp()` | every external command |
| `dup2()` + file descriptors | redirection and pipes |
| `pipe()` | connecting two processes |
| `waitpid()` + `WNOHANG` | background jobs |
| `SIGCHLD` signal handling | job completion notification |
| `termios` raw mode | arrow keys, backspace |
| ANSI escape codes | line redrawing |
| Circular buffer + modulo | history without dynamic alloc |
| Function pointers | builtin command dispatch |
| `strtok` | command tokenization |
| `volatile sig_atomic_t` | signal-safe flag |
| DRY refactoring | unified redirection handler |

---

## References

- [Write a Shell in C — Stephen Brennan](https://brennan.io/2015/01/16/write-a-shell-in-c/)
- [Build Your Own Text Editor — kilo](http://viewsourcecode.org/snaptoken/kilo/)
- [project-based-learning repo](https://github.com/practical-tutorials/project-based-learning)
- Linux man pages: `fork(2)`, `execvp(3)`, `waitpid(2)`, `dup2(2)`, `pipe(2)`, `termios(3)`

---

*aish is actively being developed. Star it to follow the journey from shell → AI terminal.*`:
