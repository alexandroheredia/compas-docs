use crate::docs::chunker::chunk_document;
use crate::docs::extractors::{lowercase_extension, DocumentExtractorRegistry};
use crate::docs::models::DocumentChunk;
use crate::indexing::IndexingAdapter;
use crate::models::IndexedChunk;
use anyhow::Result;
use std::path::Path;
use tracing::warn;

pub struct DocumentPreparedFile {
    indexed_chunks: Vec<IndexedChunk>,
}

pub struct DocumentIndexAdapter {
    extractors: DocumentExtractorRegistry,
}

impl DocumentIndexAdapter {
    pub fn new() -> Self {
        Self {
            extractors: DocumentExtractorRegistry::new(),
        }
    }
}

impl Default for DocumentIndexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexingAdapter for DocumentIndexAdapter {
    type PreparedFile = DocumentPreparedFile;

    fn supports_path(&self, path: &Path) -> bool {
        matches!(
            lowercase_extension(path).as_deref(),
            Some("md" | "txt" | "pdf")
        )
    }

    fn prepare_file(&self, path: &Path, bytes: &[u8]) -> Result<Option<Self::PreparedFile>> {
        let extracted = self.extractors.extract(path, bytes)?;
        let chunks: Vec<DocumentChunk> = chunk_document(&extracted);
        if chunks.is_empty() {
            warn!("skip document {}: no extractable text", path.display());
            return Ok(None);
        }

        Ok(Some(DocumentPreparedFile {
            indexed_chunks: chunks.into_iter().map(IndexedChunk::from).collect(),
        }))
    }

    fn indexed_chunks<'a>(&self, prepared: &'a Self::PreparedFile) -> &'a [IndexedChunk] {
        &prepared.indexed_chunks
    }

    fn after_upsert(&self, _file_path: &str, _prepared: &Self::PreparedFile) -> Result<()> {
        Ok(())
    }

    fn after_delete(&self, _file_path: &str) -> Result<()> {
        Ok(())
    }
}
