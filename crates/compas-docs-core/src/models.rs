use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DOCUMENT_ID_KEY: &str = "document_id";
pub const FILE_NAME_KEY: &str = "file_name";
pub const EXTENSION_KEY: &str = "extension";
pub const TITLE_KEY: &str = "title";
pub const HEADING_PATH_KEY: &str = "heading_path";
pub const PAGE_START_KEY: &str = "page_start";
pub const PAGE_END_KEY: &str = "page_end";
pub const TEXT_KEY: &str = "text";
pub const PREVIEW_KEY: &str = "preview";
pub const SUPPORTED_DOCUMENT_FILE_TYPES: &[&str] = &["md", "txt", "pdf"];

pub fn default_folder_file_types() -> Vec<String> {
    SUPPORTED_DOCUMENT_FILE_TYPES
        .iter()
        .map(|file_type| (*file_type).to_string())
        .collect()
}

pub fn normalize_folder_file_types(file_types: &[String]) -> Vec<String> {
    SUPPORTED_DOCUMENT_FILE_TYPES
        .iter()
        .filter(|supported| {
            file_types
                .iter()
                .any(|file_type| file_type.trim().eq_ignore_ascii_case(supported))
        })
        .map(|file_type| (*file_type).to_string())
        .collect()
}

pub fn normalize_or_default_folder_file_types(file_types: &[String]) -> Vec<String> {
    let normalized = normalize_folder_file_types(file_types);
    if normalized.is_empty() {
        return default_folder_file_types();
    }
    normalized
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentStoragePaths {
    pub root: PathBuf,
    pub store_path: PathBuf,
    pub manifest_path: PathBuf,
    pub sqlite_path: PathBuf,
    pub tantivy_dir: PathBuf,
    pub hnsw_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderRecord {
    pub id: String,
    pub path: String,
    pub display_name: String,
    pub storage_path: String,
    #[serde(default = "default_folder_file_types")]
    pub file_types: Vec<String>,
    pub last_indexed_at: Option<u64>,
    pub watch_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FolderRegistry {
    pub folders: Vec<FolderRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchDocumentItem {
    pub folder_id: String,
    pub folder_name: String,
    pub file_path: String,
    pub absolute_path: String,
    pub title: String,
    pub section: String,
    pub page: String,
    pub preview: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibraryStats {
    pub folder_count: usize,
    pub indexed_folder_count: usize,
    pub document_count: usize,
    pub chunk_count: usize,
    pub last_indexed_at: Option<u64>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RankedHit {
    pub chunk_id: String,
    pub score: f32,
}
