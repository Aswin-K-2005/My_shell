use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

pub struct SemanticMemory {
    model: TextEmbedding,
}

impl SemanticMemory {
    /// Initializes the AI model on your CPU
    pub fn new() -> Result<Self, anyhow::Error> {
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_show_download_progress(true),
        )?;

        Ok(Self { model })
    }

    /// Takes a shell command and turns it into a vector map
    pub fn generate_embedding(&self, text: &str) -> Result<Vec<f32>, anyhow::Error> {
        let documents = vec![text];
        let mut embeddings = self.model.embed(documents, None)?;
        let vector = embeddings.pop().expect("Failed to generate embedding");
        Ok(vector)
    }
}
