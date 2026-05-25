use crate::embedder::{EmbedMode, Embedder};
use crate::models::SearchHit;
use crate::store::Store;
use anyhow::Result;
use std::collections::HashMap;

pub async fn search_chunks(
    embedder: &dyn Embedder,
    store: &dyn Store,
    query: &str,
    limit: usize,
    filters: &HashMap<String, String>,
) -> Result<Vec<SearchHit>> {
    let embedding = embedder.embed(query, EmbedMode::Query).await?;
    store.search_indexed(&embedding, limit, filters).await
}
