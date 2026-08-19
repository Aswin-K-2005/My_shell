mod consolidator;
mod memory_manager;
mod memory_types;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;

use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use memory_engine::{ipc, SemanticMemory, VectorStore, WorkingMemoryStore};
use memory_manager::MemoryManager;

const CHAT_SOCKET_PATH: &str = "/tmp/aish_chat.sock";

enum RequestType {
    FastFix(String),
    StandardChat(String),
    DeepThink(String),
}

struct InferenceTask {
    request: RequestType,
    token_tx: mpsc::Sender<String>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    println!("🚀 Booting aish AI Orchestrator (Daily Driver Mode)...");

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

    let fjall_ipc = Arc::clone(&fjall);
    let lancedb_ipc = Arc::clone(&lancedb);
    let semantic_ipc = Arc::clone(&semantic);
    tokio::spawn(async move {
        if let Err(e) = ipc::start_ipc(fjall_ipc, lancedb_ipc, semantic_ipc).await {
            println!("IPC Telemetry server error: {}", e);
        }
    });

    let fjall_consol = Arc::clone(&fjall);
    let lancedb_consol = Arc::clone(&lancedb);
    let semantic_consol = Arc::clone(&semantic);
    tokio::spawn(async move {
        consolidator::start_consolidator(fjall_consol, lancedb_consol, semantic_consol).await;
    });

    let backend = Arc::new(LlamaBackend::init().expect("Failed to initialize LlamaBackend"));

    // =========================================================================
    // ⚙️ UNIFIED WORKER: Lexical Scope VRAM Swapping
    // =========================================================================
    let (task_tx, mut task_rx) = mpsc::channel::<InferenceTask>(32);
    let backend_worker = Arc::clone(&backend);

    std::thread::spawn(move || {
        let mut pending_deep_think: Option<InferenceTask> = None;

        // OUTER LOOP: Manages VRAM states and satisfies the Borrow Checker natively
        loop {
            // 1. Process pending Deep Think (14B) if VRAM is empty
            if let Some(task) = pending_deep_think.take() {
                if let RequestType::DeepThink(ref prompt) = task.request {
                    let path_14b =
                        Path::new("models/models/Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf");
                    let direct_14b = Path::new("models/Qwen2.5-Coder-14B-Instruct-Q4_K_M.gguf");
                    let target_14b = if path_14b.exists() {
                        path_14b
                    } else {
                        direct_14b
                    };

                    if target_14b.exists() {
                        println!("🚀 [VRAM] Loading 14B Model...");
                        let params = LlamaModelParams::default().with_n_gpu_layers(28);
                        if let Ok(m_14b) =
                            LlamaModel::load_from_file(&backend_worker, target_14b, &params)
                        {
                            let ctx_params = LlamaContextParams::default()
                                .with_n_ctx(std::num::NonZeroU32::new(2048));
                            if let Ok(mut c_14b) = m_14b.new_context(&backend_worker, ctx_params) {
                                run_inference(&mut c_14b, &m_14b, prompt, &task.token_tx);
                            }
                        }
                        println!("🧹 [VRAM] Deep task complete. Unloading 14B model.");
                    } else {
                        println!("⚠️ [File] 14B Model not found at {:?}", target_14b);
                        let _ = task
                            .token_tx
                            .blocking_send(" [System Error: 14B Model not found.]".to_string());
                    }
                }
            }

            // 2. Pin 1.5B to Idle State
            println!("⚡ [VRAM] Pinning 1.5B Fast Model (~1.2GB) for instant Auto-Fixes...");
            let path_1_5b = Path::new("models/Qwen2.5-Coder-1.5B-Instruct-Q4_K_M.gguf");
            let model_1_5b = if path_1_5b.exists() {
                let params = LlamaModelParams::default().with_n_gpu_layers(99);
                LlamaModel::load_from_file(&backend_worker, path_1_5b, &params).ok()
            } else {
                println!("⚠️ [File] 1.5B Model not found.");
                None
            };

            let mut ctx_1_5b = if let Some(ref m) = model_1_5b {
                let ctx_params =
                    LlamaContextParams::default().with_n_ctx(std::num::NonZeroU32::new(2048));
                m.new_context(&backend_worker, ctx_params).ok()
            } else {
                None
            };

            // 3. INNER LOOP: Process rapid messages
            let mut channel_active = false;
            while let Some(task) = task_rx.blocking_recv() {
                channel_active = true;
                match task.request {
                    RequestType::FastFix(ref prompt) => {
                        if let (Some(m), Some(c)) = (model_1_5b.as_ref(), ctx_1_5b.as_mut()) {
                            run_inference(c, m, prompt, &task.token_tx);
                        } else {
                            let _ = task
                                .token_tx
                                .blocking_send(" [System Error: No Auto-Fix model]".to_string());
                        }
                    }

                    RequestType::StandardChat(ref prompt) => {
                        println!("💬 [VRAM] Chat requested! Loading 7B model dynamically...");
                        let path_7b = Path::new("models/Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf");

                        if path_7b.exists() {
                            let params = LlamaModelParams::default().with_n_gpu_layers(24);
                            if let Ok(m_7b) =
                                LlamaModel::load_from_file(&backend_worker, path_7b, &params)
                            {
                                let ctx_params = LlamaContextParams::default()
                                    .with_n_ctx(std::num::NonZeroU32::new(2048));
                                if let Ok(mut c_7b) = m_7b.new_context(&backend_worker, ctx_params)
                                {
                                    run_inference(&mut c_7b, &m_7b, prompt, &task.token_tx);
                                }
                            }
                            println!(
                                "🧹 [VRAM] Chat complete. 7B model unloaded. VRAM freed for OS."
                            );
                        } else {
                            println!("⚠️ [File] 7B missing! Falling back to 1.5B.");
                            if let (Some(m), Some(c)) = (model_1_5b.as_ref(), ctx_1_5b.as_mut()) {
                                run_inference(c, m, prompt, &task.token_tx);
                            }
                        }
                    }

                    RequestType::DeepThink(_) => {
                        println!("🧠 [VRAM] Deep Reasoning requested! Dropping 1.5B to maximize space...");
                        pending_deep_think = Some(task);
                        break; // Breaking inner loop drops ctx_1_5b and model_1_5b from memory
                    }
                }
            }

            if !channel_active && pending_deep_think.is_none() {
                break;
            }
        }
    });

    // =========================================================================
    // 🔌 UNIX CHAT SOCKET SERVER & TRIAGE ROUTER
    // =========================================================================
    if Path::new(CHAT_SOCKET_PATH).exists() {
        std::fs::remove_file(CHAT_SOCKET_PATH)?;
    }
    let listener = UnixListener::bind(CHAT_SOCKET_PATH)?;
    println!(
        "🔌 Unix Chat Socket Server listening on {}",
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

            if !raw_request.is_empty() {
                let current_project_path = std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| "/home/aswinkss/coding/My_shell".to_string());

                let is_auto_fix = raw_request.contains("The user typed the command:");
                let is_deep_think = raw_request.to_lowercase().starts_with("think:")
                    || raw_request.contains("--deep");

                // Extract clean command name formatted as a query for LanceDB search
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

                // Query LanceDB
                let retrieved_context = memory_manager
                    .retrieve_and_compress_context(&search_query, &current_project_path)
                    .await
                    .unwrap_or_default();

                if is_auto_fix {
                    println!("🔍 [RAG Debug] Search Query: {}", search_query);
                    if !retrieved_context.is_empty() {
                        println!("🟢 [RAG Debug] Context Found: {}", retrieved_context);
                    } else {
                        println!("🔴 [RAG Debug] No context found in LanceDB! Falling back to SLM parametric knowledge.");
                    }
                }

                // =============================================================
                // 🧠 OPTION 1: RUST-NATIVE COGNITIVE BRANCHING
                // =============================================================
                let mega_prompt = if is_auto_fix {
                    if !retrieved_context.is_empty() {
                        // BRANCH A: Verified Memory Found -> Pre-fill output and force SLM to copy fix
                        let system_role = "You are a strict CLI auto-fix assistant. A verified fix for this exact command was retrieved from memory. Output the exact fix from the memory.";
                        format!(
                            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n[VERIFIED MEMORY FIX]:\n{}\n\nCURRENT FAILED COMMAND:\n{}<|im_end|>\n<|im_start|>assistant\nHere is the verified fix:\n```sh\n",
                            system_role, retrieved_context, raw_request
                        )
                    } else {
                        // BRANCH B: Memory Miss -> Let SLM freely reason with its parametric knowledge
                        let system_role = "You are an expert Linux CLI auto-fix assistant. Analyze the failed command and error message. IGNORE shell prefixes like 'lsh:' or 'bash:'. Provide a brief explanation and the fix command inside a ```sh codeblock.";
                        format!(
                            "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\nCURRENT FAILED COMMAND TO FIX:\n{}<|im_end|>\n<|im_start|>assistant\n",
                            system_role, raw_request
                        )
                    }
                } else if is_deep_think {
                    let system_role =
                        "You are Aish, a Deep Software Architect. Analyze this problem thoroughly.";
                    let context_block = if !retrieved_context.is_empty() {
                        format!(
                            "\n\n[RETRIEVED LOCAL MEMORIES & KNOWN RULES]:\n{}",
                            retrieved_context
                        )
                    } else {
                        String::new()
                    };
                    format!(
                        "<|im_start|>system\n{}{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                        system_role, context_block, raw_request
                    )
                } else {
                    let system_role = "You are Aish, a friendly, elite Staff Software Engineer CLI assistant. Always provide a helpful and conversational response.";
                    let context_block = if !retrieved_context.is_empty() {
                        format!(
                            "\n\n[RETRIEVED LOCAL MEMORIES & KNOWN RULES]:\n{}",
                            retrieved_context
                        )
                    } else {
                        String::new()
                    };
                    format!(
                        "<|im_start|>system\n{}{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                        system_role, context_block, raw_request
                    )
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
                        println!("💡 [Orchestrator] Captured AI Suggested Fix: '{}'", code);
                    }
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
            println!("❌ [Inference Error] Tokenization failed: {}", e);
            let _ = token_tx.blocking_send(" [Error: Tokenization failed]".to_string());
            return;
        }
    };

    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(2048, 1);
    for (i, &token) in tokens.iter().enumerate() {
        let is_last = i == tokens.len() - 1;
        let _ = batch.add(token, i as i32, &[0], is_last);
    }

    if let Err(e) = ctx.decode(&mut batch) {
        println!("❌ [Inference Error] VRAM Decode failed: {}", e);
        let _ = token_tx.blocking_send(" [Error: VRAM Decode failed]".to_string());
        return;
    }

    let mut n_cur = tokens.len() as i32;
    let n_max = n_cur + 512;

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
            println!("❌ [Inference Error] Decode step failed mid-generation");
            break;
        }
    }

    if !generated_something {
        println!("⚠️ [Inference Warning] Model instantly returned EOG without generating text.");
        let _ = token_tx.blocking_send(" Hello! I'm here. How can I help you today?".to_string());
    }
}
