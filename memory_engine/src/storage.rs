// src/storage.rs

use fjall::{Config, Keyspace, PartitionCreateOptions, PartitionHandle};
use shared_types::CommandTelemetry;

pub struct WorkingMemoryStore {
    #[allow(unused)]
    db: Keyspace,
    command_logs: PartitionHandle,
    #[allow(unused)]
    session_nodes: PartitionHandle,
}

impl WorkingMemoryStore {
    pub fn new(storage_path: &str) -> fjall::Result<Self> {
        let db = Config::new(storage_path).open()?;
        let command_logs = db.open_partition("command_logs", PartitionCreateOptions::default())?;
        let session_nodes =
            db.open_partition("session_nodes", PartitionCreateOptions::default())?;

        Ok(Self {
            db,
            command_logs,
            session_nodes,
        })
    }

    pub fn persist_command(&self, telemetry: &CommandTelemetry) -> fjall::Result<()> {
        let serialized_data: Vec<u8> = telemetry.into();
        self.command_logs
            .insert(telemetry.command_id.as_bytes(), serialized_data)?;
        Ok(())
    }

    pub fn get_command(&self, command_id: &str) -> fjall::Result<Option<CommandTelemetry>> {
        let Some(item_bytes) = self.command_logs.get(command_id.as_bytes())? else {
            return Ok(None);
        };

        let mut telemetry: CommandTelemetry =
            serde_json::from_slice(&item_bytes).expect("Deserialize didnt work");
        telemetry.command_id = command_id.to_string();

        Ok(Some(telemetry))
    }
    pub fn get_all_commands(&self) -> fjall::Result<Vec<CommandTelemetry>> {
        let mut commands = Vec::new();

        // Iterate over all key-value pairs in the command_logs partition
        for kv in self.command_logs.iter() {
            let (_, item_bytes) = kv?;
            if let Ok(telemetry) = serde_json::from_slice::<CommandTelemetry>(&item_bytes) {
                commands.push(telemetry);
            }
        }

        // Sort chronologically using the ISO 8601 string timestamp
        commands.sort_by(|a, b| a.start_timestamp.cmp(&b.start_timestamp));

        Ok(commands)
    }
}
