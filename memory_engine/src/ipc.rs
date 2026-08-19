use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;

use crate::semantic::SemanticMemory;
use crate::storage::WorkingMemoryStore;
use crate::vector_store::VectorStore;
use shared_types::CommandTelemetry;

const SOCKET_PATH: &str = "/tmp/aish.sock";

pub async fn start_ipc(
    fjall_db: Arc<WorkingMemoryStore>,
    _lancedb: Arc<VectorStore>, // Kept in signature for future Triage integration
    _semantic: Arc<SemanticMemory>, // Kept in signature for future Triage integration
) -> Result<(), anyhow::Error> {
    if Path::new(SOCKET_PATH).exists() {
        fs::remove_file(SOCKET_PATH)?;
    }

    let listener = UnixListener::bind(SOCKET_PATH)?;
    println!("📡 Episodic Telemetry Socket listening on {}", SOCKET_PATH);

    loop {
        let (mut stream, _) = listener.accept().await?;
        // Now we just clone the Arc that was passed in from main.rs!
        let fjall_db = Arc::clone(&fjall_db);

        tokio::spawn(async move {
            let mut buffer = Vec::new();
            if let Ok(_bytes_read) = stream.read_to_end(&mut buffer).await {
                match serde_json::from_slice::<CommandTelemetry>(&buffer) {
                    Ok(mut telemetry) => {
                        // 1. Generate the unique command_id
                        telemetry.command_id = uuid::Uuid::new_v4().to_string();

                        // 2. Save ONLY to the Episodic Buffer (Fjall)
                        if let Err(e) = fjall_db.persist_command(&telemetry) {
                            println!("❌ Failed to persist to Fjall: {}", e);
                        } else {
                            println!(
                                "📝 Logged to Episodic Memory: [{}] {}",
                                telemetry.exit_code, telemetry.raw_command
                            );
                        }

                        // ==========================================
                        // 🛑 COGNITIVE TRIAGE BOUNDARY
                        // We NO LONGER auto-embed and dump into LanceDB!
                        // A separate system will evaluate Fjall's logs
                        // to ensure causal confidence before promotion.
                        // ==========================================
                    }
                    Err(e) => println!("Failed to parse JSON telemetry: {}", e),
                }
            }
        });
    }
}
