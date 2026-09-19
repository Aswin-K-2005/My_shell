mod audio;
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

// The GBNF Grammar that physically blocks the LLM from using markdown for tools
const TOOL_GRAMMAR: &str = r#"
root ::= (normal-text | tool-call)*
normal-text ::= [^@]+
tool-call ::= "@@ " [a-zA-Z0-9_ \-\.\/]+ " @@"
"#;

#[derive(Debug)]
enum RequestType {
    DualPass {
        voice_prompt: String,
        text_prompt: String,
        use_fast_model_for_text: bool,
    },
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
    println!("  Booting Aish AI Orchestrator...");

    audio::init_audio_worker().await;

    let semantic = Arc::new(SemanticMemory::new().expect("Failed to init FastEmbed"));
    let lancedb = Arc::new(VectorStore::new(".lancedb_data").await.unwrap());
    let fjall = Arc::new(WorkingMemoryStore::new(".fjall_data_ipc")?);

    let memory_manager = Arc::new(MemoryManager::new(
        Arc::clone(&lancedb),
        Arc::clone(&semantic),
    ));

    let fjall_ipc = Arc::clone(&fjall);
    let lancedb_ipc = Arc::clone(&lancedb);
    let semantic_ipc = Arc::clone(&semantic);
    tokio::spawn(async move {
        let _ = ipc::start_ipc(fjall_ipc, lancedb_ipc, semantic_ipc).await;
    });

    let fjall_consol = Arc::clone(&fjall);
    let lancedb_consol = Arc::clone(&lancedb);
    let semantic_consol = Arc::clone(&semantic);
    tokio::spawn(async move {
        consolidator::start_consolidator(fjall_consol, lancedb_consol, semantic_consol).await;
    });

    let backend = Arc::new(LlamaBackend::init().unwrap());
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

    std::thread::spawn(move || {
        let mut active_chat_shells: HashSet<i32> = HashSet::new();
        let mut model_1_5b: Option<LlamaModel> = None;
        let mut model_7b: Option<LlamaModel> = None;
        let mut model_14b: Option<LlamaModel> = None;

        if path_1_5b.exists() {
            let params = LlamaModelParams::default().with_n_gpu_layers(99);
            model_1_5b = LlamaModel::load_from_file(&backend_worker, &path_1_5b, &params).ok();
        }

        while let Some(task) = task_rx.blocking_recv() {
            match task.request {
                RequestType::ChatModeEnter(pid) => {
                    active_chat_shells.insert(pid);
                    if model_7b.is_none() && path_7b.exists() {
                        let params = LlamaModelParams::default().with_n_gpu_layers(99);
                        model_7b =
                            LlamaModel::load_from_file(&backend_worker, &path_7b, &params).ok();
                    }
                }
                RequestType::ChatModeExit(pid) => {
                    active_chat_shells.remove(&pid);
                    if active_chat_shells.is_empty() && model_7b.is_some() {
                        model_7b = None;
                    }
                }
                RequestType::DualPass {
                    voice_prompt,
                    text_prompt,
                    use_fast_model_for_text,
                } => {
                    if model_14b.is_some() {
                        model_14b = None;
                    }
                    if model_1_5b.is_none() && path_1_5b.exists() {
                        let params = LlamaModelParams::default().with_n_gpu_layers(99);
                        model_1_5b =
                            LlamaModel::load_from_file(&backend_worker, &path_1_5b, &params).ok();
                    }

                    if !voice_prompt.is_empty() {
                        if let Some(m) = model_1_5b.as_ref() {
                            let ctx_params = LlamaContextParams::default()
                                .with_n_ctx(std::num::NonZeroU32::new(2048));
                            if let Ok(mut c) = m.new_context(&backend_worker, ctx_params) {
                                let intro = run_inference_silent(&mut c, m, &voice_prompt, 50);
                                if !intro.trim().is_empty() {
                                    let _ = task
                                        .token_tx
                                        .blocking_send(format!("__SPOKEN_INTRO__ {}", intro));
                                }
                            }
                        }
                    }

                    if use_fast_model_for_text {
                        if let Some(m) = model_1_5b.as_ref() {
                            let ctx_params = LlamaContextParams::default()
                                .with_n_ctx(std::num::NonZeroU32::new(2048));
                            if let Ok(mut c) = m.new_context(&backend_worker, ctx_params) {
                                run_inference(&mut c, m, &text_prompt, &task.token_tx);
                            }
                        }
                    } else {
                        if model_7b.is_none() && path_7b.exists() {
                            let params = LlamaModelParams::default().with_n_gpu_layers(99);
                            model_7b =
                                LlamaModel::load_from_file(&backend_worker, &path_7b, &params).ok();
                        }
                        if let Some(m) = model_7b.as_ref() {
                            let ctx_params = LlamaContextParams::default()
                                .with_n_ctx(std::num::NonZeroU32::new(4096));
                            if let Ok(mut c) = m.new_context(&backend_worker, ctx_params) {
                                run_inference(&mut c, m, &text_prompt, &task.token_tx);
                            }
                        }
                    }
                }
                RequestType::DeepThink(ref prompt) => {
                    model_1_5b = None;
                    model_7b = None;
                    if path_14b.exists() {
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
                    }
                    if path_1_5b.exists() {
                        let params = LlamaModelParams::default().with_n_gpu_layers(99);
                        model_1_5b =
                            LlamaModel::load_from_file(&backend_worker, &path_1_5b, &params).ok();
                    }
                    if !active_chat_shells.is_empty() && path_7b.exists() {
                        let params = LlamaModelParams::default().with_n_gpu_layers(99);
                        model_7b =
                            LlamaModel::load_from_file(&backend_worker, &path_7b, &params).ok();
                    }
                }
            }
        }
    });

    if std::path::Path::new(CHAT_SOCKET_PATH).exists() {
        std::fs::remove_file(CHAT_SOCKET_PATH)?;
    }
    let listener = UnixListener::bind(CHAT_SOCKET_PATH)?;

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

            let is_state_event = raw_request.contains("\"event\": \"entered_chat\"")
                || raw_request.contains("\"event\": \"exited_chat\"");
            if !is_state_event {
                audio::stop_speaking();
            }

            if raw_request.contains("\"event\": \"entered_chat\"") {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw_request) {
                    if let Some(pid) = json["pid"].as_i64() {
                        let (token_tx, _) = mpsc::channel(1);
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

            let current_project_path = std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let is_auto_fix = raw_request.contains("The user typed the command:");

            let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            let profile_path = format!("{}/.aish_profile.md", home_dir);
            let user_profile = std::fs::read_to_string(&profile_path).unwrap_or_default();

            let mut context_block = if !user_profile.is_empty() {
                format!("\n\n[CRITICAL: USER IDENTITY PROFILE]\n{}\n", user_profile)
            } else {
                String::new()
            };

            if let Ok(entries) = std::fs::read_dir(std::env::current_dir().unwrap_or_default()) {
                let mut files = String::new();
                for entry in entries.flatten() {
                    files.push_str(&format!("- {}\n", entry.file_name().to_string_lossy()));
                }
                context_block.push_str(&format!("\n[FILES IN CURRENT DIRECTORY]:\n{}\n", files));
            }

            let search_query = if is_auto_fix {
                raw_request
                    .lines()
                    .find(|line| line.contains("The user typed the command:"))
                    .unwrap_or(&raw_request)
                    .to_string()
            } else {
                raw_request.clone()
            };

            let retrieved_context = memory_manager
                .retrieve_and_compress_context(&search_query, &current_project_path)
                .await
                .unwrap_or_default();
            if !retrieved_context.is_empty() {
                context_block.push_str(&format!(
                    "\n[RETRIEVED LOCAL MEMORIES & KNOWN RULES]:\n{}\n",
                    retrieved_context
                ));
            }

            let voice_sys = if is_auto_fix {
                "You are Aish. The user's command failed. Write EXACTLY ONE short, joyful, conversational spoken sentence reacting with good humor. DO NOT output markdown.".to_string()
            } else {
                format!(
                    "You are the joyful voice of Aish. The user just asked: '{}' \
                Write EXACTLY ONE enthusiastic, spoken sentence reacting to this. \
                1. Vary your opening (e.g., 'Aha!', 'Oh awesome,', 'Sweet!'). \
                2. Keep it to ONE casual sentence.",
                    raw_request
                )
            };

            let text_sys = if is_auto_fix {
                format!("You are a strict CLI auto-fix assistant. Output the exact fix inside a ```sh block.\n{}", context_block)
            } else {
                format!("You are Aish, an elite AI terminal assistant. \
                CRITICAL INSTRUCTIONS: \
                1. If you need to inspect a file (e.g., new.c), YOU MUST autonomously run the command using this exact format: @@ cat new.c @@ \
                2. NEVER tell the user to run the command. You must use the @@ format to run it yourself. \
                3. DO NOT output markdown code blocks for tools. \
                4. Stop generating text immediately after the closing @@.\n{}", context_block)
            };

            let mut current_voice_prompt = format!("<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n", voice_sys, raw_request);
            let mut current_text_prompt = format!("<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n", text_sys, raw_request);

            loop {
                let (token_tx, mut token_rx) = mpsc::channel::<String>(32);
                let task = InferenceTask {
                    request: RequestType::DualPass {
                        voice_prompt: current_voice_prompt.clone(),
                        text_prompt: current_text_prompt.clone(),
                        use_fast_model_for_text: is_auto_fix,
                    },
                    token_tx,
                };

                current_voice_prompt = String::new();

                if task_tx.send(task).await.is_ok() {
                    let mut tool_buffer = String::new();
                    let mut capturing_tool = false;
                    let mut tool_executed = false;
                    let mut tool_result = String::new();

                    while let Some(chunk) = token_rx.recv().await {
                        if chunk.contains("__SPOKEN_INTRO__") {
                            let intro = chunk.replace("__SPOKEN_INTRO__", "").trim().to_string();
                            let clean_intro = prepare_speech_text(&intro);

                            if !clean_intro.is_empty() {
                                audio::speak_stream(clean_intro.clone());
                                let _ = stream.write_all(b"__SPEAKING__").await;
                                let _ = stream.flush().await;
                                let delay_ms = clean_intro.len() as u64 * 65;
                                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms))
                                    .await;
                            }
                            continue;
                        }

                        if chunk.contains("@@") && !capturing_tool {
                            capturing_tool = true;
                            let split_idx = chunk.find("@@").unwrap();

                            if split_idx > 0 {
                                let _ = stream.write_all(chunk[..split_idx].as_bytes()).await;
                            }

                            tool_buffer.push_str(&chunk[split_idx..]);
                        } else if capturing_tool {
                            tool_buffer.push_str(&chunk);
                        }

                        if capturing_tool {
                            if tool_buffer.len() > 2 && tool_buffer[2..].contains("@@") {
                                if let Some(end_offset) = tool_buffer[2..].find("@@") {
                                    let tool_cmd =
                                        tool_buffer[2..end_offset + 2].trim().to_string();

                                    let blocked =
                                        ["curl ", "wget ", "rm ", "sudo ", "apt ", ">", "chmod "]
                                            .iter()
                                            .any(|&k| tool_cmd.contains(k));

                                    if !blocked && !tool_cmd.is_empty() {
                                        if let Ok(output) = std::process::Command::new("sh")
                                            .arg("-c")
                                            .arg(&tool_cmd)
                                            .current_dir(&current_project_path)
                                            .output()
                                        {
                                            tool_result =
                                                String::from_utf8_lossy(&output.stdout).to_string();
                                            if tool_result.is_empty() {
                                                tool_result =
                                                    String::from_utf8_lossy(&output.stderr)
                                                        .to_string();
                                            }
                                            tool_executed = true;
                                        }
                                    }
                                }
                                break;
                            }
                            continue;
                        }

                        let _ = stream.write_all(chunk.as_bytes()).await;
                    }

                    if tool_executed {
                        current_text_prompt.push_str(&format!("\n<system_output>\n{}\n</system_output>\nNow, summarize this output for the user directly.", tool_result));
                        continue;
                    }

                    let _ = stream.write_all(b"__END__").await;
                    let _ = stream.flush().await;
                    break;
                }
            }
        });
    }
}

fn prepare_speech_text(text: &str) -> String {
    text.replace("`", "")
        .replace("*sigh*", "huuuuh")
        .replace("*laughs*", "haha")
        .trim()
        .to_string()
}

fn run_inference(
    ctx: &mut llama_cpp_2::context::LlamaContext,
    model: &LlamaModel,
    prompt: &str,
    token_tx: &mpsc::Sender<String>,
) {
    ctx.clear_kv_cache();
    let mut tokens = model
        .str_to_token(prompt, llama_cpp_2::model::AddBos::Never)
        .unwrap_or_default();
    if tokens.is_empty() {
        return;
    }
    if tokens.len() > 1900 {
        tokens.truncate(1900);
    }

    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(2048, 1);
    for (i, &token) in tokens.iter().enumerate() {
        let _ = batch.add(token, i as i32, &[0], i == tokens.len() - 1);
    }
    if ctx.decode(&mut batch).is_err() {
        return;
    }

    let mut n_cur = tokens.len() as i32;
    let n_max = n_cur + 2048;

    // Initialize the grammar as a direct pluggable sampler using the model's vocabulary
    let grammar_sampler = llama_cpp_2::sampling::LlamaSampler::grammar(model, TOOL_GRAMMAR, "root")
        .expect("Failed to initialize grammar sampler");

    let mut sampler = llama_cpp_2::sampling::LlamaSampler::chain_simple([
        llama_cpp_2::sampling::LlamaSampler::temp(0.4),
        grammar_sampler,
        llama_cpp_2::sampling::LlamaSampler::dist(1337),
    ]);

    while n_cur < n_max {
        let new_token_id = sampler.sample(ctx, -1);
        sampler.accept(new_token_id);
        if model.is_eog_token(new_token_id) {
            break;
        }

        #[allow(deprecated)]
        if let Ok(piece) = model.token_to_str(new_token_id, llama_cpp_2::model::Special::Tokenize) {
            if token_tx.blocking_send(piece).is_err() {
                break;
            }
        }
        batch.clear();
        let _ = batch.add(new_token_id, n_cur, &[0], true);
        n_cur += 1;
        if ctx.decode(&mut batch).is_err() {
            break;
        }
    }
}

fn run_inference_silent(
    ctx: &mut llama_cpp_2::context::LlamaContext,
    model: &LlamaModel,
    prompt: &str,
    max_tokens: i32,
) -> String {
    ctx.clear_kv_cache();
    let mut tokens = model
        .str_to_token(prompt, llama_cpp_2::model::AddBos::Never)
        .unwrap_or_default();
    if tokens.is_empty() {
        return String::new();
    }
    if tokens.len() > 1800 {
        tokens.truncate(1800);
    }

    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(2048, 1);
    for (i, &token) in tokens.iter().enumerate() {
        let _ = batch.add(token, i as i32, &[0], i == tokens.len() - 1);
    }
    if ctx.decode(&mut batch).is_err() {
        return String::new();
    }

    let mut n_cur = tokens.len() as i32;
    let n_max = n_cur + max_tokens;

    let mut sampler = llama_cpp_2::sampling::LlamaSampler::chain_simple([
        llama_cpp_2::sampling::LlamaSampler::temp(0.4),
        llama_cpp_2::sampling::LlamaSampler::dist(1337),
    ]);

    let mut result = String::new();
    while n_cur < n_max {
        let new_token_id = sampler.sample(ctx, -1);
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
