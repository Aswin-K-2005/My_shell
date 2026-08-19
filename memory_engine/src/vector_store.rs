// src/vector_store.rs

use arrow_array::{
    FixedSizeListArray, Float32Array, Int32Array, Int64Array, RecordBatch, RecordBatchIterator,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::connect;
use lancedb::connection::Connection;
use lancedb::table::Table;
use std::sync::Arc;

pub const VECTOR_DIM: usize = 384;
const TABLE_NAME: &str = "command_embeddings";

pub struct VectorStore {
    pub table: Table,
}

impl VectorStore {
    /// Helper function to generate the universal schema
    fn get_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("scope", DataType::Utf8, false),
            Field::new("mem_type", DataType::Utf8, false),
            Field::new("created_at", DataType::Int64, false),
            Field::new("utility_score", DataType::Int32, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    VECTOR_DIM as i32,
                ),
                false,
            ),
        ]))
    }

    /// Opens or creates the LanceDB database and initializes the Memory table schema.
    pub async fn new(db_path: &str) -> Result<Self, anyhow::Error> {
        let db: Connection = connect(db_path).execute().await?;
        let schema = Self::get_schema();

        let table = match db.open_table(TABLE_NAME).execute().await {
            Ok(existing_table) => existing_table,
            Err(_) => {
                let empty_batch = RecordBatch::new_empty(schema.clone());
                let batches = RecordBatchIterator::new(vec![Ok(empty_batch)], schema.clone());
                db.create_table(TABLE_NAME, batches).execute().await?
            }
        };

        Ok(Self { table })
    }

    /// Inserts a consolidated memory vector alongside its metadata into LanceDB
    pub fn add_memory_vector(
        &self,
        id: &str,
        content: &str,
        scope: &str,
        mem_type: &str,
        created_at: i64,
        utility_score: i32,
        vector: &[f32],
    ) -> Result<RecordBatch, anyhow::Error> {
        if vector.len() != VECTOR_DIM {
            anyhow::bail!("Vector dimension mismatch");
        }

        let schema = Self::get_schema();

        // Construct Arrow Column Arrays
        let id_array = Arc::new(StringArray::from(vec![id]));
        let content_array = Arc::new(StringArray::from(vec![content]));
        let scope_array = Arc::new(StringArray::from(vec![scope]));
        let type_array = Arc::new(StringArray::from(vec![mem_type]));
        let time_array = Arc::new(Int64Array::from(vec![created_at]));
        let score_array = Arc::new(Int32Array::from(vec![utility_score]));

        let values_array = Float32Array::from(vector.to_vec());
        let vector_array = Arc::new(FixedSizeListArray::new(
            Arc::new(Field::new("item", DataType::Float32, true)),
            VECTOR_DIM as i32,
            Arc::new(values_array),
            None,
        ));

        // Package arrays into a single Arrow RecordBatch
        let batch = RecordBatch::try_new(
            schema,
            vec![
                id_array,
                content_array,
                scope_array,
                type_array,
                time_array,
                score_array,
                vector_array,
            ],
        )?;

        Ok(batch)
    }
    pub async fn save_batch(&self, batch: RecordBatch) -> Result<(), anyhow::Error> {
        let schema = batch.schema();
        let iterator = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.table.add(iterator).execute().await?;
        Ok(())
    }

    // ... keep save_batch and search_similar_command for now
}
