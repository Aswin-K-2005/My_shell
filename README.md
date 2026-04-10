# AIShell 🐚

A Unix shell built from scratch in C, with plans to evolve into a fully AI-powered intelligent shell.

> Built by [Aswin K](https://github.com/Aswin-K-2005) — learning systems programming one feature at a time.

---

## What it can do right now

- **Command execution** — runs any program on your system via `fork()` and `execvp()`
- **Output redirection** — `ls > out.txt` saves output to a file
- **Piping** — `ls | grep .c` chains commands together
- **Command history** — stores last 100 commands, type `history` to view
- **Builtins** — `cd`, `help`, `history`, `exit` handled natively
- **Dynamic input buffer** — handles commands of any length via `realloc`

---

## Built with

- **Language** — C
- **Core concepts** — `fork()`, `execvp()`, `waitpid()`, `dup2()`, `pipe()`, `open()`
- **No external libraries** — pure C and POSIX system calls

---

## Build and run

```bash
# clone the repo
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
# basic command
> ls -la

# output redirection
> ls > out.txt

# piping
> ls | grep .c

# view history
> history

# change directory
> cd /tmp

# exit
> exit
```

---

## How it works

```
you type a command
       │
       ▼
lsh_read_line()       reads input character by character
       │
       ▼
lsh_split_line()      tokenizes into argv[] array using strtok
       │
       ▼
lsh_execute()         checks if it's a builtin or external command
      / \
     /   \
builtin   external
  │         │
runs      fork()
directly    │
          child → execvp()
          parent → waitpid()
```

---

## Project structure

```
My_shell/
├── shell.c        # entire shell in one file
└── README.md
```

---

## Roadmap 🚀

This is just the beginning. Here's where this project is heading:

### Phase 2 — More shell features
- [ ] `>>` append redirection
- [ ] `<` input redirection
- [ ] Background jobs with `&`
- [ ] `&&` and `||` operators
- [ ] Tab autocomplete with Trie data structure
- [ ] Arrow key history navigation (raw mode)

### Phase 3 — AI integration
- [ ] Local LLM (CodeLlama 7B via ollama) running on device
- [ ] AI explains errors when commands fail — in plain English
- [ ] Natural language to shell command translation
- [ ] Context-aware command suggestions

### Phase 4 — Qt GUI (Warp-style)
- [ ] Standalone terminal window built with Qt6/C++
- [ ] Command blocks — each command and output grouped visually
- [ ] AI explanation panel alongside terminal
- [ ] Syntax highlighting in input

---

## Why I built this

Most people learn shell scripting without ever understanding what happens when they press Enter. Building a shell from scratch in C forces you to understand:

- How the OS creates processes (`fork`)
- How programs actually get executed (`execvp`)
- How pipes and file descriptors work at the kernel level
- How memory is managed manually without garbage collection

This project is part of a larger goal — building an AI-powered shell that not only runs commands but teaches you why things work (and why they break).

---

## What I learned so far

- `fork()` + `execvp()` — how every Unix command actually runs
- `dup2()` — how redirection works at the file descriptor level
- `strtok()` — string tokenization and why it modifies the original string
- Circular buffer with modulo — efficient history without dynamic allocation
- Function pointers — how to map command names to functions cleanly

---

## References

- [Write a Shell in C — Stephen Brennan](https://brennan.io/2015/01/16/write-a-shell-in-c/)
- [project-based-learning repo](https://github.com/practical-tutorials/project-based-learning)
- Linux man pages: `fork(2)`, `execvp(3)`, `waitpid(2)`, `dup2(2)`, `pipe(2)`

---

*This project is actively being developed. Star it if you want to follow the journey.*
