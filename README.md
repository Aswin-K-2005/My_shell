# aish 🐚
### Aswin's Intelligent Shell

A Unix shell built from scratch in C, supercharged by a highly optimized, local AI orchestrator written in Rust.

> Built by [Aswin K](https://github.com/Aswin-K-2005) — Legion 5i Pro + i9 + RTX 4070Ti doing the heavy lifting.

---

## What is aish?

Most people use a shell without understanding what happens when they press Enter. **aish** started as a learning project built from zero using raw C and POSIX system calls — no shortcuts, no libraries. 

It has since evolved into a **Cognitive AI Terminal**. Instead of relying on cloud APIs or slow Python wrappers, `aish` uses a native Rust orchestrator communicating via Unix sockets to run local LLMs (via `llama.cpp`) and a native RAG memory engine. It learns from your daily workflow, predicts fixes, and acts as a local Agentic assistant—all running entirely offline.

---

## Current Features

### 💻 Core Shell (C & POSIX)
- [x] Command execution & standard Builtins (`cd`, `help`, `history`, `exit`)
- [x] Output/Input redirection (`>`, `>>`, `<`) and Piping (`|`)
- [x] Background jobs (`&`) with job completion notifications
- [x] Raw mode + arrow key history
- [x] Ghost text & Tab autocomplete (Trie structure)
- [x] Logical operators (`&&` and `||`)

### 🧠 AI & Memory Engine (Rust)
- [x] **Local AI Inference**: Bare-metal `llama.cpp` integration (Vulkan accelerated).
- [x] **Lexical VRAM Hot-Swapping**: Keeps a fast 1.5B model pinned for instant auto-fixes, dynamically loads 7B/14B models for deep architectural reasoning.
- [x] **Semantic RAG Memory**: Uses `FastEmbed` and `LanceDB` to vector-search past shell errors and learn custom fixes over time.
- [x] **Short-Term Context**: Uses `Fjall` for ultra-fast IPC state tracking.
- [x] **Cognitive Routing**: Rust natively calculates heuristics to determine if it should provide an instant fix or trigger deep reasoning.
- [ ] **Agentic Tool Use**: (WIP) ReAct loop allowing the LLM to execute `grep`, `cat`, and file searches directly in the shell.

---

## Build and Run

### Prerequisites
- GCC / Clang
- Rust & Cargo (`rustc 1.75+`)
- Local GGUF Models (placed in `models/` directory)

### 1. Build the Rust AI Orchestrator
```bash
# Compile the memory engine and inference backend
cargo build --release

2. Compile the C Shell
Bash
# Compile the shell with AI socket extensions
make 

3. Run
Bash
# Start the shell (the orchestrator daemon will handle AI requests)
./aish

Architecture
aish uses a decoupled architecture. The C shell handles raw POSIX operations and UI, while the heavy AI lifting is pushed to an asynchronous Rust backend over IPC.

┌────────────────────────┐
                  │   Unix Socket Client   │ (aish C Shell)
                  └───────────┬────────────┘
                              │ raw_request
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   ai_orchestrator (Rust)                    │
│                                                             │
│ 1. Socket Listener   ──> Receives raw command payload       │
│ 2. Query Sanitizer   ──> Formats search query for LanceDB   │
│ 3. Cognitive Router  ──> Option 1 Prompt Branching          │
│ 4. Worker Thread     ──> Lexical Scope VRAM Hot-Swapper     │
│ 5. Llama-cpp Driver  ──> run_inference() (Vulkan GPU)       │
└──────────────┬──────────────────────────────┬───────────────┘
               │                              │
               ▼                              ▼
┌──────────────────────────────┐  ┌───────────────────────────┐
│       memory_engine          │  │       consolidator        │
│                              │  │                           │
│ • Runs FastEmbed vectorizer  │  │ • Background Tokio loop   │
│ • Queries LanceDB vectors    │  │ • Scans Fjall short-term  │
│ • Ranks & compresses rules   │  │ • Writes long-term rules  │
└──────────────────────────────┘  └───────────────────────────┘

Project Structure

My_shell/
├── ai.c / ai.h           # C Shell: AI Unix socket communication
├── new.c                 # C Shell: POSIX process & input management
├── ai_orchestrator/      # Rust: Tokio async server & llama.cpp bindings
├── memory_engine/        # Rust: FastEmbed, LanceDB, and Fjall IPC
├── shared_types/         # Rust: Shared data structures across workspaces
├── models/               # Local GGUF weights (git-ignored)
└── Cargo.toml            # Rust workspace definition



Roadmap 🚀
Phase 1 & 2 — Shell Polish (Completed)
Raw mode, autocomplete, piping, redirection, background jobs.

Phase 3 — Systems-Level AI Integration (Current)
[x] Replace Python/cloud APIs with native Rust/llama.cpp.

[x] Build semantic memory engine (LanceDB/FastEmbed).

[x] Implement VRAM resource management for SLMs.

[ ] Implement C-side popen() for Agentic Tool Execution <tool_call>.

Phase 4 — The Cognitive Loop
[ ] Upgrade memory schema to track success/failure confidence rates.

[ ] Enable the orchestrator to dynamically manage context windows for codebase ingestion.

[ ] Daemonize the orchestrator as a background OS service.

Concept,Where it's used
C / POSIX Systems,"fork(), execvp(), dup2(), waitpid(), SIGCHLD, termios"
Inter-Process Comm.,Unix Domain Sockets connecting C to Rust
Concurrency (Rust),"Tokio async/await, mpsc channels, multi-threading"
Memory Bounds,Lexical scoping to force VRAM allocation/deallocation
Machine Learning,"Quantized GGUF inference, Vector Embeddings, RAG Pipelines"
Constrained Decoding,Probability redistribution and LLM cognitive branching

References
Write a Shell in C — Stephen Brennan

Build Your Own Text Editor — kilo

Linux man pages: fork(2), execvp(3), waitpid(2), dup2(2), pipe(2), termios(3)

llama.cpp and tokio documentation.

aish is actively being developed as a daily-driver tool. Star it to follow the journey from a basic shell to an autonomous system.
