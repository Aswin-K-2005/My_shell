use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandTelemetry {
    pub session_id: String,

    #[serde(skip)]
    pub command_id: String,

    pub raw_command: String,
    pub working_directory: PathBuf,
    pub git_branch: Option<String>,
    pub exit_code: i32,
    pub start_timestamp: DateTime<Utc>,
    pub execution_duration_ms: u64,
    pub captured_output_summary: Option<String>,
}

impl Default for CommandTelemetry {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            command_id: String::new(),
            raw_command: String::new(),
            working_directory: PathBuf::new(),
            git_branch: None,
            exit_code: 0,
            start_timestamp: chrono::Utc::now(),
            execution_duration_ms: 0,
            captured_output_summary: None,
        }
    }
}

// Moved this here so both databases and the AI can easily serialize it!
impl From<&CommandTelemetry> for Vec<u8> {
    fn from(val: &CommandTelemetry) -> Self {
        serde_json::to_vec(val).expect("Failed to serialize the CommandTelemetry")
    }
}
