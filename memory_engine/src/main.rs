mod ipc;
mod semantic;
mod storage;
mod vector_store;

use semantic::SemanticMemory;
use storage::WorkingMemoryStore;
use vector_store::VectorStore;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    println!("🧠 Initializing JARVIS Memory Daemon...");

    let fjall_db = WorkingMemoryStore::new(".fjall_data")?;
    let lancedb = VectorStore::new(".lancedb_data").await?;
    let semantic = SemanticMemory::new()?;

    println!("⚡ JARVIS Memory Daemon Online!");

    // Start listening on /tmp/aish.sock
    ipc::start_ipc(fjall_db, lancedb, semantic).await?;

    Ok(())
}
