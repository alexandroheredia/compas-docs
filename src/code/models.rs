use crate::models::{IndexedChunk, SearchHit};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const LANGUAGE_KEY: &str = "language";
pub const SYMBOL_KEY: &str = "symbol";
pub const LINE_START_KEY: &str = "line_start";
pub const LINE_END_KEY: &str = "line_end";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeChunk {
    pub id: String,
    pub content: String,
    pub language: String,
    pub file_path: String,
    pub symbol: String,
    #[serde(rename = "line_start")]
    pub line_start: usize,
    #[serde(rename = "line_end")]
    pub line_end: usize,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub meta: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeSearchResult {
    pub chunk: CodeChunk,
    pub score: f32,
}

impl From<CodeChunk> for IndexedChunk {
    fn from(value: CodeChunk) -> Self {
        let mut metadata = HashMap::from([
            (
                LANGUAGE_KEY.to_string(),
                Value::String(value.language.clone()),
            ),
            (SYMBOL_KEY.to_string(), Value::String(value.symbol.clone())),
            (
                LINE_START_KEY.to_string(),
                Value::Number((value.line_start as u64).into()),
            ),
            (
                LINE_END_KEY.to_string(),
                Value::Number((value.line_end as u64).into()),
            ),
        ]);

        for (key, val) in value.meta {
            metadata.entry(key).or_insert(Value::String(val));
        }

        IndexedChunk {
            id: value.id,
            content: value.content,
            file_path: value.file_path,
            kind: value.kind,
            metadata,
        }
    }
}

impl TryFrom<IndexedChunk> for CodeChunk {
    type Error = anyhow::Error;

    fn try_from(value: IndexedChunk) -> Result<Self> {
        let IndexedChunk {
            id,
            content,
            file_path,
            kind,
            metadata,
        } = value;

        let language = required_string(&metadata, LANGUAGE_KEY)?;
        let symbol = required_string(&metadata, SYMBOL_KEY)?;
        let line_start = required_usize(&metadata, LINE_START_KEY)?;
        let line_end = required_usize(&metadata, LINE_END_KEY)?;

        let mut meta = HashMap::new();
        for (key, value) in metadata {
            if matches!(
                key.as_str(),
                LANGUAGE_KEY | SYMBOL_KEY | LINE_START_KEY | LINE_END_KEY
            ) {
                continue;
            }
            if let Value::String(value) = value {
                meta.insert(key, value);
            }
        }

        Ok(Self {
            id,
            content,
            language,
            file_path,
            symbol,
            line_start,
            line_end,
            kind,
            meta,
        })
    }
}

impl From<CodeSearchResult> for SearchHit {
    fn from(value: CodeSearchResult) -> Self {
        Self {
            chunk: value.chunk.into(),
            score: value.score,
        }
    }
}

impl TryFrom<SearchHit> for CodeSearchResult {
    type Error = anyhow::Error;

    fn try_from(value: SearchHit) -> Result<Self> {
        Ok(Self {
            chunk: CodeChunk::try_from(value.chunk)?,
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

fn required_usize(metadata: &HashMap<String, Value>, key: &str) -> Result<usize> {
    metadata
        .get(key)
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .ok_or_else(|| anyhow!("indexed chunk missing numeric metadata '{}'", key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn code_chunk_try_from_indexed_chunk_preserves_legacy_fields() {
        let chunk = IndexedChunk {
            id: "chunk-1".to_string(),
            content: "auth body".to_string(),
            file_path: "/tmp/lib/auth.dart".to_string(),
            kind: "method".to_string(),
            metadata: HashMap::from([
                (LANGUAGE_KEY.to_string(), json!("dart")),
                (SYMBOL_KEY.to_string(), json!("AuthService.login")),
                (LINE_START_KEY.to_string(), json!(12)),
                (LINE_END_KEY.to_string(), json!(24)),
                ("owner".to_string(), json!("team-auth")),
            ]),
        };

        let code_chunk = CodeChunk::try_from(chunk).unwrap();

        assert_eq!(code_chunk.language, "dart");
        assert_eq!(code_chunk.symbol, "AuthService.login");
        assert_eq!(code_chunk.line_start, 12);
        assert_eq!(code_chunk.line_end, 24);
        assert_eq!(code_chunk.kind, "method");
        assert_eq!(code_chunk.file_path, "/tmp/lib/auth.dart");
        assert_eq!(code_chunk.meta.get("owner"), Some(&"team-auth".to_string()));
    }
}
