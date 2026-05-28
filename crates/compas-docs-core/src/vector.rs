use crate::sqlite::ChunkRow;
use anyhow::{anyhow, Result};
use hnsw_rs::api::AnnT;
use hnsw_rs::prelude::{DistCosine, Hnsw, HnswIo};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const HNSW_BASENAME: &str = "hnsw";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MappingEntry {
    chunk_id: String,
    external_id: usize,
}

pub fn rebuild_index(hnsw_dir: &Path, chunks: &[ChunkRow]) -> Result<()> {
    std::fs::create_dir_all(hnsw_dir)?;
    if chunks.is_empty() {
        cleanup_files(hnsw_dir)?;
        return Ok(());
    }

    let dims = chunks[0].embedding.len();
    let hnsw: Hnsw<'static, f32, DistCosine> = Hnsw::new(16, chunks.len(), 16, 200, DistCosine {});
    for (index, chunk) in chunks.iter().enumerate() {
        if chunk.embedding.len() != dims {
            return Err(anyhow!("inconsistent embedding dimensions in folder index"));
        }
        hnsw.insert((&chunk.embedding, index));
    }

    cleanup_files(hnsw_dir)?;
    let basename = hnsw.file_dump(hnsw_dir, HNSW_BASENAME)?;
    if basename != HNSW_BASENAME {
        return Err(anyhow!("unexpected HNSW basename '{}'", basename));
    }
    let mapping: Vec<MappingEntry> = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| MappingEntry {
            chunk_id: chunk.chunk_id.clone(),
            external_id: index,
        })
        .collect();
    std::fs::write(
        hnsw_dir.join("mapping.json"),
        serde_json::to_vec_pretty(&mapping)?,
    )?;
    Ok(())
}

pub fn semantic_search(
    hnsw_dir: &Path,
    query_embedding: &[f32],
    limit: usize,
) -> Result<Vec<(String, f32)>> {
    let mapping_path = hnsw_dir.join("mapping.json");
    if !mapping_path.exists() {
        return Ok(Vec::new());
    }

    let mapping: Vec<MappingEntry> = serde_json::from_slice(&std::fs::read(&mapping_path)?)?;
    if mapping.is_empty() {
        return Ok(Vec::new());
    }
    let id_to_chunk: HashMap<usize, String> = mapping
        .into_iter()
        .map(|entry| (entry.external_id, entry.chunk_id))
        .collect();
    let mut io = HnswIo::new(hnsw_dir, HNSW_BASENAME);
    let hnsw: Hnsw<'_, f32, DistCosine> = io.load_hnsw()?;
    let neighbours = hnsw.search(query_embedding, limit, limit.max(20));

    let mut results = Vec::new();
    for neighbour in neighbours {
        if let Some(chunk_id) = id_to_chunk.get(&neighbour.d_id) {
            let score = (1.0 - neighbour.distance / 2.0).clamp(0.0, 1.0);
            results.push((chunk_id.clone(), score));
        }
    }
    Ok(results)
}

fn cleanup_files(hnsw_dir: &Path) -> Result<()> {
    for suffix in ["hnsw.data", "hnsw.graph", "mapping.json"] {
        let path = hnsw_dir.join(suffix);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn chunk(id: &str, embedding: Vec<f32>) -> ChunkRow {
        ChunkRow {
            chunk_id: id.to_string(),
            folder_id: "folder-1".to_string(),
            document_id: "doc-1".to_string(),
            absolute_path: "/tmp/doc.md".to_string(),
            relative_path: "doc.md".to_string(),
            file_name: "doc.md".to_string(),
            title: "Doc".to_string(),
            heading_path: Vec::new(),
            page_start: None,
            page_end: None,
            preview: "preview".to_string(),
            text: "text".to_string(),
            embedding,
            updated_at: 0,
        }
    }

    #[test]
    fn vector_index_returns_nearest_chunk() {
        let temp = tempdir().unwrap();
        rebuild_index(
            temp.path(),
            &[
                chunk("chunk-1", vec![1.0, 0.0]),
                chunk("chunk-2", vec![0.0, 1.0]),
                chunk("chunk-3", vec![0.0, 0.5]),
            ],
        )
        .unwrap();

        let results = semantic_search(temp.path(), &[0.0, 1.0], 2).unwrap();
        assert_eq!(results[0].0, "chunk-2");
    }

    #[test]
    fn vector_index_persists_and_reloads() {
        let temp = tempdir().unwrap();
        rebuild_index(
            temp.path(),
            &[
                chunk("chunk-1", vec![1.0, 0.0]),
                chunk("chunk-2", vec![0.0, 1.0]),
            ],
        )
        .unwrap();

        let results = semantic_search(temp.path(), &[0.0, 1.0], 1).unwrap();
        assert_eq!(results[0].0, "chunk-2");
    }
}
