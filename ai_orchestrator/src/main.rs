mod consolidator;
mod memory_manager;
mod memory_types;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use memory_engine::{ipc, SemanticMemory, VectorStore, WorkingMemoryStore};
use memory_manager::MemoryManager;

const CHAT_SOCKET_PATH: &str = "/tmp/aish_chat.sock";

#[derive(Debug)]
enum RequestType {
    FastFix(String),
    StandardChat(String),
    DeepThink(String),
    ChatModeEnter(i32),
    ChatModeExit(i32),
}

struct InferenceTask {
    request: RequestType,
    token_tx: mpsc::Sender<String>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // 1. HARDWARE LOCK: Force Vulkan to use the NVIDIA RTX 4070 Ti (Bypass Intel iGPU)
    //

    // 2. DISABLE CPU FALLBACK: Tell Vulkan to use raw VRAM and never use the CPU

    println!("  Booting aish AI Orchestrator (Daily Driver Mode)...");

    // Initialize Memory Engine components
    let semantic = Arc::new(SemanticMemory::new().expect("Failed to initialize FastEmbed"));
    let lancedb = Arc::new(
        VectorStore::new(".lancedb_data")
            .await
            .expect("Failed to open LanceDB"),
    );
    let fjall = Arc::new(WorkingMemoryStore::new(".fjall_data_ipc")?);

    let memory_manager = Arc::new(MemoryManager::new(
        Arc::clone(&lancedb),
        Arc::clone(&semantic),
    ));

    // IPC Telemetry Server
    let fjall_ipc = Arc::clone(&fjall);
    let lancedb_ipc = Arc::clone(&lancedb);
    let semantic_ipc = Arc::clone(&semantic);
    tokio::spawn(async move {
        if let Err(e) = ipc::start_ipc(fjall_ipc, lancedb_ipc, semantic_ipc).await {
            println!("IPC Telemetry server error: {}", e);
        }
    });

    // Episodic Consolidator
    let fjall_consol = Arc::clone(&fjall);
    let lancedb_consol = Arc::clone(&lancedb);
    let semantic_consol = Arc::clone(&semantic);
    tokio::spawn(async move {
        consolidator::start_consolidator(fjall_consol, lancedb_consol, semantic_consol).await;
    });

    let backend = Arc::new(LlamaBackend::init().expect("Failed to initialize LlamaBackend"));

    // Set up Absolute Paths dynamically
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let models_dir = format!("{}/coding/My_shell/models", home_dir);

    let path_1_5b = PathBuf::from(format!(
        "{}/Qwen2.5-Coder-1.5B-Instruct-Q4_K_M.gguf",
        models_dir
    ));
    let path_7b = PathBuf::from(format!(
        "{}/Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf",
        models_dir
    ));
    let path_14b = PathBuf::from(format!(
        "{}/Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf",
        models_dir
    ));

    let (task_tx, mut task_rx) = mpsc::channel::<InferenceTask>(32);
    let backend_worker = Arc::clone(&backend);

    // =========================================================================
    //   STATEFUL VRAM MANAGER (WORKER THREAD)
    // =========================================================================
    std::thread::spawn(move || {
        let mut active_chat_shells: HashSet<i32> = HashSet::new();

        // VRAM Storage: ONLY store the Models! (Weights take 99% of the load time)
        // Contexts (KV Caches) are created instantly on-demand, bypassing the Borrow Checker!
        let mut model_1_5b: Option<LlamaModel> = None;
        let mut model_7b: Option<LlamaModel> = None;

        println!("  [VRAM] Pinning 1.5B Fast Model (~1.2GB) for instant Auto-Fixes...");
        if path_1_5b.exists() {
            let params = LlamaModelParams::default().with_n_gpu_layers(99);
            model_1_5b = LlamaModel::load_from_file(&backend_worker, &path_1_5b, &params).ok();
        } else {
            println!("  [File] 1.5B Model not found at {:?}", path_1_5b);
        }

        while let Some(task) = task_rx.blocking_recv() {
            match task.request {
                RequestType::ChatModeEnter(pid) => {
                    active_chat_shells.insert(pid);
                    println!(
                        "  [State] PID {} entered Chat Mode. Active chats: {}",
                        pid,
                        active_chat_shells.len()
                    );

                    if model_7b.is_none() {
                        println!("  [VRAM] Loading 7B Chat model into memory...");
                        if path_7b.exists() {
                            let params = LlamaModelParams::default().with_n_gpu_layers(99);
                            model_7b =
                                LlamaModel::load_from_file(&backend_worker, &path_7b, &params).ok();
                        } else {
                            println!("  [File] 7B missing at {:?}", path_7b);
                        }
                    }
                }

                RequestType::ChatModeExit(pid) => {
                    active_chat_shells.remove(&pid);
                    println!(
                        "  [State] PID {} exited Chat Mode. Active chats: {}",
                        pid,
                        active_chat_shells.len()
                    );

                    if active_chat_shells.is_empty() && model_7b.is_some() {
                        println!("  [VRAM] 0 active chats. Evicting 7B model. VRAM freed for OS.");
                        model_7b = None;
                    }
                }

                RequestType::FastFix(ref prompt) => {
                    if let Some(m) = model_1_5b.as_ref() {
                        let ctx_params = LlamaContextParams::default()
                            .with_n_ctx(std::num::NonZeroU32::new(2048));
                        if let Ok(mut c) = m.new_context(&backend_worker, ctx_params) {
                            run_inference(&mut c, m, prompt, &task.token_tx);
                        }
                    } else {
                        let _ = task
                            .token_tx
                            .blocking_send(" [System Error: No Auto-Fix model]".to_string());
                    }
                }

                RequestType::StandardChat(ref prompt) => {
                    if model_7b.is_none() && path_7b.exists() {
                        println!("  [VRAM Warning] Lazy-loading 7B model (State Miss)...");
                        let params = LlamaModelParams::default().with_n_gpu_layers(99);
                        model_7b =
                            LlamaModel::load_from_file(&backend_worker, &path_7b, &params).ok();
                    }

                    if let Some(m) = model_7b.as_ref() {
                        let ctx_params = LlamaContextParams::default()
                            .with_n_ctx(std::num::NonZeroU32::new(4096));
                        if let Ok(mut c) = m.new_context(&backend_worker, ctx_params) {
                            run_inference(&mut c, m, prompt, &task.token_tx);
                        }
                    } else if let Some(m) = model_1_5b.as_ref() {
                        println!("  [VRAM] 7B missing! Falling back to 1.5B.");
                        let ctx_params = LlamaContextParams::default()
                            .with_n_ctx(std::num::NonZeroU32::new(2048));
                        if let Ok(mut c) = m.new_context(&backend_worker, ctx_params) {
                            run_inference(&mut c, m, prompt, &task.token_tx);
                        }
                    }
                }

                RequestType::DeepThink(ref prompt) => {
                    println!(
                        "  [VRAM] Deep Reasoning requested! Dropping SLMs to maximize space..."
                    );
                    model_1_5b = None;
                    model_7b = None;

                    if path_14b.exists() {
                        println!("  [VRAM] Loading 14B Model...");
                        let params = LlamaModelParams::default().with_n_gpu_layers(28);
                        if let Ok(m) =
                            LlamaModel::load_from_file(&backend_worker, &path_14b, &params)
                        {
                            let ctx_params = LlamaContextParams::default()
                                .with_n_ctx(std::num::NonZeroU32::new(4096));
                            if let Ok(mut c) = m.new_context(&backend_worker, ctx_params) {
                                run_inference(&mut c, &m, prompt, &task.token_tx);
                            }
                        }
                        println!("  [VRAM] Deep task complete. Unloading 14B model.");
                    }

                    println!("  [VRAM] Recovering 1.5B Idle State...");
                    if path_1_5b.exists() {
                        let params = LlamaModelParams::default().with_n_gpu_layers(99);
                        model_1_5b =
                            LlamaModel::load_from_file(&backend_worker, &path_1_5b, &params).ok();
                    }

                    if !active_chat_shells.is_empty() {
                        println!("  [VRAM] Recovering 7B Chat State...");
                        if path_7b.exists() {
                            let params = LlamaModelParams::default().with_n_gpu_layers(99);
                            model_7b =
                                LlamaModel::load_from_file(&backend_worker, &path_7b, &params).ok();
                        }
                    }
                }
            }
        }
    });

    // =========================================================================
    //   UNIX CHAT SOCKET SERVER & TRIAGE ROUTER
    // =========================================================================
    if std::path::Path::new(CHAT_SOCKET_PATH).exists() {
        std::fs::remove_file(CHAT_SOCKET_PATH)?;
    }
    let listener = UnixListener::bind(CHAT_SOCKET_PATH)?;
    println!(
        "  Unix Chat Socket Server listening on {}",
        CHAT_SOCKET_PATH
    );

    loop {
        let (mut stream, _) = listener.accept().await?;
        let task_tx = task_tx.clone();
        let memory_manager = Arc::clone(&memory_manager);

        tokio::spawn(async move {
            let mut buffer = Vec::new();
            let mut read_buf = [0u8; 1024];
            while let Ok(bytes_read) = stream.read(&mut read_buf).await {
                if bytes_read == 0 {
                    break;
                }
                buffer.extend_from_slice(&read_buf[..bytes_read]);
                if buffer.ends_with(b"__MSG_END__") {
                    break;
                }
            }

            let raw_request = String::from_utf8_lossy(&buffer)
                .trim_end_matches("__MSG_END__")
                .trim()
                .to_string();

            if raw_request.is_empty() {
                return;
            }

            // 1. Intercept JSON State Triggers
            if raw_request.contains("\"event\": \"entered_chat\"") {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw_request) {
                    if let Some(pid) = json["pid"].as_i64() {
                        let (token_tx, _) = mpsc::channel(1); // Dummy channel
                        let _ = task_tx
                            .send(InferenceTask {
                                request: RequestType::ChatModeEnter(pid as i32),
                                token_tx,
                            })
                            .await;
                        return;
                    }
                }
            }

            if raw_request.contains("\"event\": \"exited_chat\"") {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw_request) {
                    if let Some(pid) = json["pid"].as_i64() {
                        let (token_tx, _) = mpsc::channel(1);
                        let _ = task_tx
                            .send(InferenceTask {
                                request: RequestType::ChatModeExit(pid as i32),
                                token_tx,
                            })
                            .await;
                        return;
                    }
                }
            }

            // 2. Standard Routing
            let current_project_path = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "/home/aswinkss/coding/My_shell".to_string());

            let is_auto_fix = raw_request.contains("The user typed the command:");
            let is_deep_think =
                raw_request.to_lowercase().starts_with("think:") || raw_request.contains("--deep");

            let search_query = if is_auto_fix {
                let extracted_cmd = raw_request
                    .lines()
                    .find(|line| line.contains("The user typed the command:"))
                    .map(|line| {
                        line.replace("The user typed the command:", "")
                            .trim()
                            .to_string()
                    })
                    .unwrap_or_else(|| raw_request.clone());
                format!("what is the fix for the command {}", extracted_cmd)
            } else {
                raw_request.clone()
            };

            let retrieved_context = memory_manager
                .retrieve_and_compress_context(&search_query, &current_project_path)
                .await
                .unwrap_or_default();

            let mega_prompt = if is_auto_fix {
                if !retrieved_context.is_empty() {
                    let system_role = "You are a strict CLI auto-fix assistant. A verified fix for this exact command was retrieved from memory. Output the exact fix from the memory.";
                    format!(
                        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n[VERIFIED MEMORY FIX]:\n{}\n\nCURRENT FAILED COMMAND:\n{}<|im_end|>\n<|im_start|>assistant\nHere is the verified fix:\n```sh\n",
                        system_role, retrieved_context, raw_request
                    )
                } else {
                    let system_role = "You are an expert Linux CLI auto-fix assistant. Analyze the failed command and error message. IGNORE shell prefixes like 'lsh:' or 'bash:'. Provide a brief explanation and the fix command inside a ```sh codeblock.";
                    format!(
                        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\nCURRENT FAILED COMMAND TO FIX:\n{}<|im_end|>\n<|im_start|>assistant\n",
                        system_role, raw_request
                    )
                }
            } else if is_deep_think {
                let system_role = "You are Aish, an elite AI terminal agent. \
                You have direct access to the user's filesystem. \
                CRITICAL RULE: If the user asks about a file, a function, or code, DO NOT GUESS. \
                You MUST execute a terminal command to find out. \
                To execute a command, output exactly: <tool_call>command</tool_call>\n\
                Example 1: <tool_call>grep -n \"lsh_split_pipe\" new.c</tool_call>\n\
                Example 2: <tool_call>sed -n '10,20p' ai.c</tool_call>\n\
                Stop talking and wait for the tool response immediately after issuing a tool call.";
                let context_block = if !retrieved_context.is_empty() {
                    format!(
                        "\n\n[RETRIEVED LOCAL MEMORIES & KNOWN RULES]:\n{}",
                        retrieved_context
                    )
                } else {
                    String::new()
                };
                format!("<|im_start|>system\n{}{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n", system_role, context_block, raw_request)
            } else {
                let system_role = "You are Aish, an elite AI terminal agent. \
                You have direct access to the user's filesystem. \
                CRITICAL RULES: \
                1. If asked how code works, DO NOT GUESS. \
                2. You MUST use multi-step reasoning. \
                3. Before using a tool, you MUST explain your plan inside a <thought> block. \
                4. NEVER use interactive commands like `less`, `more`, `vim`, or `nano`. \
                5. Output the command inside a ```command codeblock. \n\
                \n\
                EXAMPLE FORMAT:\n\
                <thought>\n\
                I need to find where lsh_split_pipe is defined in new.c. I will use grep to get the line number first, then I will read the code.\n\
                </thought>\n\
                ```command\n\
                grep -n \"lsh_split_pipe\" new.c\n\
                ```\n\
                \n\
                Stop talking immediately after the codeblock and wait for the tool response.";
                let context_block = if !retrieved_context.is_empty() {
                    format!(
                        "\n\n[RETRIEVED LOCAL MEMORIES & KNOWN RULES]:\n{}",
                        retrieved_context
                    )
                } else {
                    String::new()
                };
                format!("<|im_start|>system\n{}{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n", system_role, context_block, raw_request)
            };

            let request_enum = if is_auto_fix {
                RequestType::FastFix(mega_prompt)
            } else if is_deep_think {
                RequestType::DeepThink(mega_prompt)
            } else {
                RequestType::StandardChat(mega_prompt)
            };

            let (token_tx, mut token_rx) = mpsc::channel::<String>(32);
            let task = InferenceTask {
                request: request_enum,
                token_tx,
            };

            let mut full_response = String::new();
            if task_tx.send(task).await.is_ok() {
                while let Some(chunk) = token_rx.recv().await {
                    let _ = stream.write_all(chunk.as_bytes()).await;
                    full_response.push_str(&chunk);
                }
                let _ = stream.write_all(b"__END__").await;
                let _ = stream.flush().await;
            }

            if is_auto_fix {
                if let Some(code) = extract_code_block(&full_response) {
                    println!("  [Orchestrator] Captured AI Suggested Fix: '{}'", code);
                }
            }
        });
    }
}

fn extract_code_block(text: &str) -> Option<String> {
    if let Some(start) = text.find("```") {
        let after_start = &text[start + 3..];
        let code_start = after_start.find('\n').map(|i| i + 1).unwrap_or(0);
        let code_body = &after_start[code_start..];
        if let Some(end) = code_body.find("```") {
            return Some(code_body[..end].trim().to_string());
        }
    }
    None
}

fn run_inference(
    ctx: &mut llama_cpp_2::context::LlamaContext,
    model: &LlamaModel,
    prompt: &str,
    token_tx: &mpsc::Sender<String>,
) {
    ctx.clear_kv_cache();
    let tokens = match model.str_to_token(prompt, llama_cpp_2::model::AddBos::Never) {
        Ok(t) => t,
        Err(e) => {
            println!("  [Inference Error] Tokenization failed: {}", e);
            return;
        }
    };

    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(2048, 1);
    for (i, &token) in tokens.iter().enumerate() {
        let is_last = i == tokens.len() - 1;
        let _ = batch.add(token, i as i32, &[0], is_last);
    }

    if let Err(e) = ctx.decode(&mut batch) {
        println!("  [Inference Error] VRAM Decode failed: {}", e);
        return;
    }

    let mut n_cur = tokens.len() as i32;
    let n_max = n_cur + 2048;
    let mut sampler = llama_cpp_2::sampling::LlamaSampler::chain_simple([
        llama_cpp_2::sampling::LlamaSampler::temp(0.7),
        llama_cpp_2::sampling::LlamaSampler::dist(1337),
    ]);

    let mut generated_something = false;
    while n_cur < n_max {
        let new_token_id = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(new_token_id);
        if model.is_eog_token(new_token_id) {
            break;
        }

        #[allow(deprecated)]
        if let Ok(piece) = model.token_to_str(new_token_id, llama_cpp_2::model::Special::Tokenize) {
            if token_tx.blocking_send(piece).is_err() {
                break;
            }
            generated_something = true;
        }

        batch.clear();
        let _ = batch.add(new_token_id, n_cur, &[0], true);
        n_cur += 1;
        if ctx.decode(&mut batch).is_err() {
            break;
        }
    }

    if !generated_something {
        println!("  [Inference Warning] Model instantly returned EOG.");
        let _ = token_tx.blocking_send(" Hello! I'm here. How can I help you today?".to_string());
    }
}

fn run_inference_silent(
    ctx: &mut llama_cpp_2::context::LlamaContext,
    model: &LlamaModel,
    prompt: &str,
) -> String {
    ctx.clear_kv_cache();
    let mut tokens = match model.str_to_token(prompt, llama_cpp_2::model::AddBos::Never) {
        Ok(t) => t,
        Err(_) => return String::new(),
    };

    // Safety limit: Truncate to avoid overflowing the 4096 context window
    if tokens.len() > 3800 {
        tokens.truncate(3800);
    }

    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(4096, 1);
    for (i, &token) in tokens.iter().enumerate() {
        let is_last = i == tokens.len() - 1;
        let _ = batch.add(token, i as i32, &[0], is_last);
    }

    if ctx.decode(&mut batch).is_err() {
        return String::new();
    }

    let mut n_cur = tokens.len() as i32;
    let n_max = n_cur + 250; // Keep the summary short and punchy!
    let mut sampler = llama_cpp_2::sampling::LlamaSampler::chain_simple([
        llama_cpp_2::sampling::LlamaSampler::temp(0.3), // Low temp for factual reading
    ]);

    let mut result = String::new();
    while n_cur < n_max {
        let new_token_id = sampler.sample(ctx, batch.n_tokens() - 1);
        sampler.accept(new_token_id);
        if model.is_eog_token(new_token_id) {
            break;
        }

        #[allow(deprecated)]
        if let Ok(piece) = model.token_to_str(new_token_id, llama_cpp_2::model::Special::Tokenize) {
            result.push_str(&piece);
        }

        batch.clear();
        let _ = batch.add(new_token_id, n_cur, &[0], true);
        n_cur += 1;
        if ctx.decode(&mut batch).is_err() {
            break;
        }
    }
    result
}
