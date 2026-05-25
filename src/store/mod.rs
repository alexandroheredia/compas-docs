use crate::models::{IndexedChunk, SearchHit};
use anyhow::Result;
use std::collections::HashMap;

#[async_trait::async_trait]
pub trait Store: Send + Sync {
    async fn init(&self, vector_size: usize) -> Result<()>;
    async fn upsert_indexed(&self, chunks: &[IndexedChunk], embeddings: &[Vec<f32>]) -> Result<()>;
    async fn search_indexed(
        &self,
        embedding: &[f32],
        limit: usize,
        filters: &HashMap<String, String>,
    ) -> Result<Vec<SearchHit>>;
    async fn delete_by_file(&self, file_path: &str) -> Result<()>;
}

pub mod edge;
