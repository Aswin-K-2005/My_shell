use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryScope {
    Global,
    Project(String),
    Session(String),
}

impl MemoryScope {
    pub fn from_str(s: &str) -> Self {
        if s == "Global" {
            MemoryScope::Global
        } else if s.starts_with("Project:") {
            MemoryScope::Project(s.trim_start_matches("Project:").to_string())
        } else if s.starts_with("Session:") {
            MemoryScope::Session(s.trim_start_matches("Session:").to_string())
        } else {
            MemoryScope::Global
        }
    }

    pub fn to_string_repr(&self) -> String {
        match self {
            MemoryScope::Global => "Global".to_string(),
            MemoryScope::Project(p) => format!("Project:{}", p),
            MemoryScope::Session(s) => format!("Session:{}", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    Fact,
    Preference,
    Rule,
    Experience,
    ErrorFix,
    Workflow,
}

impl MemoryType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Preference" => MemoryType::Preference,
            "ErrorFix" => MemoryType::ErrorFix,
            "Workflow" => MemoryType::Workflow,
            "Rule" => MemoryType::Rule,
            "Experience" => MemoryType::Experience,
            _ => MemoryType::Fact,
        }
    }

    pub fn to_string_repr(&self) -> String {
        match self {
            MemoryType::Fact => "Fact",
            MemoryType::Preference => "Preference",
            MemoryType::Rule => "Rule",
            MemoryType::Experience => "Experience",
            MemoryType::ErrorFix => "ErrorFix",
            MemoryType::Workflow => "Workflow",
        }
        .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntity {
    pub id: String,
    pub content: String,
    pub scope: MemoryScope,
    pub mem_type: MemoryType,
    pub created_at: u64,
    pub valid_until: Option<u64>,
    pub utility_score: i32,
    pub supersedes_id: Option<String>,
}

impl MemoryEntity {
    pub fn new(content: String, scope: MemoryScope, mem_type: MemoryType) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            scope,
            mem_type,
            created_at: now,
            valid_until: None,
            utility_score: 0,
            supersedes_id: None,
        }
    }

    pub fn supersedes(mut self, old_memory_id: String) -> Self {
        self.supersedes_id = Some(old_memory_id);
        self
    }

    pub fn reward(&mut self) {
        self.utility_score += 1;
    }

    pub fn penalize(&mut self) {
        self.utility_score -= 1;
    }
}
