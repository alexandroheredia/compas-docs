use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexedChunk {
    pub id: String,
    pub content: String,
    pub file_path: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub metadata: HashMap<String, Value>,
}

impl IndexedChunk {
    pub fn metadata_str(&self, key: &str) -> Option<&str> {
        self.metadata.get(key)?.as_str()
    }

    pub fn metadata_string(&self, key: &str) -> Option<String> {
        self.metadata_str(key).map(str::to_string)
    }

    pub fn metadata_usize(&self, key: &str) -> Option<usize> {
        self.metadata.get(key)?.as_u64().map(|n| n as usize)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    pub chunk: IndexedChunk,
    pub score: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn indexed_chunk_metadata_helpers_preserve_strings_and_numbers() {
        let chunk = IndexedChunk {
            id: "chunk-1".to_string(),
            content: "body".to_string(),
            file_path: "/tmp/file.txt".to_string(),
            kind: "section".to_string(),
            metadata: HashMap::from([
                ("symbol".to_string(), json!("AuthService.login")),
                ("line_start".to_string(), json!(42)),
            ]),
        };

        assert_eq!(chunk.metadata_str("symbol"), Some("AuthService.login"));
        assert_eq!(
            chunk.metadata_string("symbol"),
            Some("AuthService.login".to_string())
        );
        assert_eq!(chunk.metadata_usize("line_start"), Some(42));
    }
}
