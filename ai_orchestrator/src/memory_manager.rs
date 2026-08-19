use crate::memory_types::{MemoryEntity, MemoryScope, MemoryType};
use arrow_array::{
    cast::AsArray,
    types::{Float32Type, Int32Type, Int64Type},
};
use futures::StreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use memory_engine::{SemanticMemory, VectorStore};
use std::sync::Arc;

pub struct MemoryManager {
    lancedb: Arc<VectorStore>,
    semantic: Arc<SemanticMemory>,
}

impl MemoryManager {
    pub fn new(lancedb: Arc<VectorStore>, semantic: Arc<SemanticMemory>) -> Self {
        Self { lancedb, semantic }
    }

    pub async fn retrieve_and_compress_context(
        &self,
        user_query: &str,
        current_project_path: &str,
    ) -> Result<String, anyhow::Error> {
        let query_vector = self.semantic.generate_embedding(user_query)?;
        let raw_candidates = self.fetch_raw_candidates(&query_vector, 30).await?;

        let mut scored_candidates: Vec<(f32, MemoryEntity)> = Vec::new();

        for (base_similarity, memory) in raw_candidates {
            if let MemoryScope::Project(ref path) = memory.scope {
                if path != current_project_path {
                    continue;
                }
            }

            let mut final_score = base_similarity;

            if let MemoryScope::Project(ref path) = memory.scope {
                if path == current_project_path {
                    final_score += 0.5;
                }
            }

            final_score += (memory.utility_score as f32) * 0.1;

            if memory.supersedes_id.is_some() {
                final_score -= 10.0;
            }

            if final_score > 0.5 {
                scored_candidates.push((final_score, memory));
            }
        }

        scored_candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

        let mut deduplicated: Vec<MemoryEntity> = Vec::new();
        for (_, memory) in scored_candidates {
            let prefix_end = memory
                .content
                .char_indices()
                .map(|(i, _)| i)
                .nth(10)
                .unwrap_or(memory.content.len());
            let prefix = &memory.content[..prefix_end];

            let is_duplicate = deduplicated.iter().any(|existing| {
                existing.mem_type == memory.mem_type
                    && !prefix.is_empty()
                    && existing.content.contains(prefix)
            });

            if !is_duplicate {
                deduplicated.push(memory);
            }

            if deduplicated.len() >= 5 {
                break;
            }
        }

        if deduplicated.is_empty() {
            return Ok(String::new());
        }

        let mut compressed_context = String::from("\n[SYSTEM CONTEXT: VERIFIED MEMORY]\n");
        for memory in deduplicated {
            let label = match memory.mem_type {
                MemoryType::Preference => "[PREFERENCE]",
                MemoryType::ErrorFix => "[ERROR FIX]",
                MemoryType::Workflow => "[WORKFLOW]",
                _ => "[FACT]",
            };
            compressed_context.push_str(&format!("{} {}\n", label, memory.content));
        }

        Ok(compressed_context)
    }

    async fn fetch_raw_candidates(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<(f32, MemoryEntity)>, anyhow::Error> {
        let mut results = self
            .lancedb
            .table
            .vector_search(query_vector)?
            .limit(limit)
            .execute()
            .await?;

        let mut candidates = Vec::new();

        while let Some(result) = results.next().await {
            let batch = result?;

            let id_col = batch
                .column_by_name("id")
                .expect("Missing id")
                .as_string::<i32>();
            let content_col = batch
                .column_by_name("content")
                .expect("Missing content")
                .as_string::<i32>();
            let scope_col = batch
                .column_by_name("scope")
                .expect("Missing scope")
                .as_string::<i32>();
            let type_col = batch
                .column_by_name("mem_type")
                .expect("Missing mem_type")
                .as_string::<i32>();
            let time_col = batch
                .column_by_name("created_at")
                .expect("Missing created_at")
                .as_primitive::<Int64Type>();
            let score_col = batch
                .column_by_name("utility_score")
                .expect("Missing utility_score")
                .as_primitive::<Int32Type>();
            let dist_col = batch
                .column_by_name("_distance")
                .expect("Missing _distance")
                .as_primitive::<Float32Type>();

            for i in 0..batch.num_rows() {
                let l2_distance = dist_col.value(i);
                let base_similarity = 1.0 / (1.0 + l2_distance);

                let entity = MemoryEntity {
                    id: id_col.value(i).to_string(),
                    content: content_col.value(i).to_string(),
                    scope: MemoryScope::from_str(scope_col.value(i)),
                    mem_type: MemoryType::from_str(type_col.value(i)),
                    created_at: time_col.value(i) as u64,
                    valid_until: None,
                    utility_score: score_col.value(i),
                    supersedes_id: None,
                };

                candidates.push((base_similarity, entity));
            }
        }

        Ok(candidates)
    }
}
