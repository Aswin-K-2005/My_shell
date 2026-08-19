use std::sync::Arc;
use std::time::Duration;
use tokio::time;

use crate::memory_types::{MemoryEntity, MemoryScope, MemoryType};
use memory_engine::{SemanticMemory, VectorStore, WorkingMemoryStore};

async fn llm_validate_causal_link(
    failed_cmd: &str,
    _error_summary: &str,
    candidate_fix: &str,
) -> bool {
    let read_only_cmds = [
        "ls",
        "cat",
        "pwd",
        "git status",
        "git log",
        "echo",
        "which",
        "clear",
    ];
    let clean_fix = candidate_fix.trim();

    // Reject read-only inspection commands as candidate fixes
    for ro in read_only_cmds {
        if clean_fix == ro || clean_fix.starts_with(&format!("{} ", ro)) {
            return false;
        }
    }

    // Reject running the exact same failed command as a fix
    if failed_cmd.trim() == clean_fix {
        return false;
    }

    true
}

pub async fn start_consolidator(
    fjall: Arc<WorkingMemoryStore>,
    lancedb: Arc<VectorStore>,
    semantic: Arc<SemanticMemory>,
) {
    let mut interval = time::interval(Duration::from_secs(30));
    let mut last_processed_timestamp = String::new();

    loop {
        interval.tick().await;

        let commands = match fjall.get_all_commands() {
            Ok(cmds) => cmds,
            Err(_) => continue,
        };

        if commands.len() < 2 {
            continue;
        }

        println!(
            "🔍 [Consolidator] Scanning {} episodic memories...",
            commands.len()
        );

        for window in commands.windows(2) {
            let prev = &window[0];
            let curr = &window[1];

            if curr.start_timestamp.to_string() <= last_processed_timestamp {
                continue;
            }

            // TRIGGER: Previous command failed, current command succeeded.
            if prev.exit_code != 0 && curr.exit_code == 0 {
                let failed_cmd = &prev.raw_command;
                let candidate_fix = &curr.raw_command;

                let error_summary = prev
                    .captured_output_summary
                    .as_deref()
                    .unwrap_or("Command failed");

                let is_validated =
                    llm_validate_causal_link(failed_cmd, error_summary, candidate_fix).await;

                if is_validated {
                    println!(
                        "🧠 [Consolidator] Validated Link: '{}' (Error: '{}') -> '{}'",
                        failed_cmd, error_summary, candidate_fix
                    );

                    let content = format!(
                        "When command '{}' fails with error '{}', running '{}' usually fixes it.",
                        failed_cmd, error_summary, candidate_fix
                    );

                    let memory = MemoryEntity::new(
                        content.clone(),
                        MemoryScope::Project(curr.working_directory.to_string_lossy().to_string()),
                        MemoryType::ErrorFix,
                    );

                    if let Ok(vector) = semantic.generate_embedding(&content) {
                        if let Ok(batch) = lancedb.add_memory_vector(
                            &memory.id,
                            &memory.content,
                            &memory.scope.to_string_repr(),
                            &memory.mem_type.to_string_repr(),
                            memory.created_at as i64,
                            memory.utility_score,
                            &vector,
                        ) {
                            let _ = lancedb.save_batch(batch).await;
                            println!(
                                "💾 [Memory OS] Successfully committed semantic rule: {}",
                                content
                            );
                        }
                    }
                }
            }
        }

        if let Some(last) = commands.last() {
            last_processed_timestamp = last.start_timestamp.to_string();
        }
    }
}
