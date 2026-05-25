use crate::models::{IndexedChunk, SearchHit};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const DOCUMENT_ID_KEY: &str = "document_id";
pub const FILE_NAME_KEY: &str = "file_name";
pub const EXTENSION_KEY: &str = "extension";
pub const TITLE_KEY: &str = "title";
pub const HEADING_PATH_KEY: &str = "heading_path";
pub const PAGE_START_KEY: &str = "page_start";
pub const PAGE_END_KEY: &str = "page_end";
pub const TEXT_KEY: &str = "text";
pub const PREVIEW_KEY: &str = "preview";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedDocument {
    pub document_id: String,
    pub file_path: String,
    pub file_name: String,
    pub extension: String,
    pub title: String,
    pub sections: Vec<ExtractedSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedSection {
    pub heading_path: Vec<String>,
    pub page_start: Option<usize>,
    pub page_end: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentChunk {
    pub id: String,
    pub document_id: String,
    pub file_path: String,
    pub file_name: String,
    pub extension: String,
    pub title: String,
    pub heading_path: Vec<String>,
    pub page_start: Option<usize>,
    pub page_end: Option<usize>,
    pub text: String,
    pub preview: String,
    pub enriched_text: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentSearchResult {
    pub chunk: DocumentChunk,
    pub score: f32,
}

impl From<DocumentChunk> for IndexedChunk {
    fn from(value: DocumentChunk) -> Self {
        let mut metadata = HashMap::from([
            (
                DOCUMENT_ID_KEY.to_string(),
                Value::String(value.document_id.clone()),
            ),
            (
                FILE_NAME_KEY.to_string(),
                Value::String(value.file_name.clone()),
            ),
            (
                EXTENSION_KEY.to_string(),
                Value::String(value.extension.clone()),
            ),
            (
                HEADING_PATH_KEY.to_string(),
                Value::Array(
                    value
                        .heading_path
                        .iter()
                        .cloned()
                        .map(Value::String)
                        .collect(),
                ),
            ),
            (TEXT_KEY.to_string(), Value::String(value.text.clone())),
            (
                PREVIEW_KEY.to_string(),
                Value::String(value.preview.clone()),
            ),
        ]);

        if !value.title.is_empty() {
            metadata.insert(TITLE_KEY.to_string(), Value::String(value.title.clone()));
        }
        if let Some(page_start) = value.page_start {
            metadata.insert(
                PAGE_START_KEY.to_string(),
                Value::Number((page_start as u64).into()),
            );
        }
        if let Some(page_end) = value.page_end {
            metadata.insert(
                PAGE_END_KEY.to_string(),
                Value::Number((page_end as u64).into()),
            );
        }

        IndexedChunk {
            id: value.id,
            content: value.enriched_text,
            file_path: value.file_path,
            kind: value.kind,
            metadata,
        }
    }
}

impl TryFrom<IndexedChunk> for DocumentChunk {
    type Error = anyhow::Error;

    fn try_from(value: IndexedChunk) -> Result<Self> {
        let IndexedChunk {
            id,
            content,
            file_path,
            kind,
            metadata,
        } = value;

        let file_name = required_string(&metadata, FILE_NAME_KEY)?;

        Ok(Self {
            id,
            document_id: required_string(&metadata, DOCUMENT_ID_KEY)?,
            file_path,
            file_name: file_name.clone(),
            extension: required_string(&metadata, EXTENSION_KEY)?,
            title: metadata
                .get(TITLE_KEY)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| file_stem(&file_name)),
            heading_path: heading_path(&metadata)?,
            page_start: metadata
                .get(PAGE_START_KEY)
                .and_then(Value::as_u64)
                .map(|n| n as usize),
            page_end: metadata
                .get(PAGE_END_KEY)
                .and_then(Value::as_u64)
                .map(|n| n as usize),
            text: required_string(&metadata, TEXT_KEY)?,
            preview: required_string(&metadata, PREVIEW_KEY)?,
            enriched_text: content,
            kind,
        })
    }
}

impl From<DocumentSearchResult> for SearchHit {
    fn from(value: DocumentSearchResult) -> Self {
        Self {
            chunk: value.chunk.into(),
            score: value.score,
        }
    }
}

impl TryFrom<SearchHit> for DocumentSearchResult {
    type Error = anyhow::Error;

    fn try_from(value: SearchHit) -> Result<Self> {
        Ok(Self {
            chunk: DocumentChunk::try_from(value.chunk)?,
            score: value.score,
        })
    }
}

fn required_string(metadata: &HashMap<String, Value>, key: &str) -> Result<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("indexed chunk missing string metadata '{}'", key))
}

fn heading_path(metadata: &HashMap<String, Value>) -> Result<Vec<String>> {
    let Some(value) = metadata.get(HEADING_PATH_KEY) else {
        return Ok(Vec::new());
    };

    let Value::Array(values) = value else {
        return Err(anyhow!(
            "indexed chunk missing array metadata '{}'",
            HEADING_PATH_KEY
        ));
    };

    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("heading path values must be strings"))
        })
        .collect()
}

fn file_stem(file_name: &str) -> String {
    std::path::Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file_name)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn document_chunk_try_from_indexed_chunk_preserves_document_metadata() {
        let chunk = IndexedChunk {
            id: "chunk-1".to_string(),
            content: "Document: auth-guide.md\nTitle: Auth Guide\nSection: Auth Guide > Tokens\nPage: 2\n\nBody"
                .to_string(),
            file_path: "/tmp/docs/auth-guide.md".to_string(),
            kind: "section".to_string(),
            metadata: HashMap::from([
                (DOCUMENT_ID_KEY.to_string(), json!("doc-1")),
                (FILE_NAME_KEY.to_string(), json!("auth-guide.md")),
                (EXTENSION_KEY.to_string(), json!("md")),
                (TITLE_KEY.to_string(), json!("Auth Guide")),
                (HEADING_PATH_KEY.to_string(), json!(["Auth Guide", "Tokens"])),
                (PAGE_START_KEY.to_string(), json!(2)),
                (PAGE_END_KEY.to_string(), json!(3)),
                (TEXT_KEY.to_string(), json!("Body")),
                (PREVIEW_KEY.to_string(), json!("Body")),
            ]),
        };

        let document_chunk = DocumentChunk::try_from(chunk).unwrap();

        assert_eq!(document_chunk.document_id, "doc-1");
        assert_eq!(document_chunk.file_name, "auth-guide.md");
        assert_eq!(document_chunk.extension, "md");
        assert_eq!(document_chunk.title, "Auth Guide");
        assert_eq!(document_chunk.heading_path, vec!["Auth Guide", "Tokens"]);
        assert_eq!(document_chunk.page_start, Some(2));
        assert_eq!(document_chunk.page_end, Some(3));
        assert_eq!(document_chunk.text, "Body");
        assert_eq!(document_chunk.preview, "Body");
        assert_eq!(document_chunk.kind, "section");
    }
}
