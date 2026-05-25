mod markdown;
mod pdf;
mod text;

use crate::docs::models::ExtractedDocument;
use anyhow::{anyhow, Result};
use std::path::Path;

pub use markdown::MarkdownExtractor;
pub use pdf::PdfExtractor;
pub use text::TextExtractor;

pub trait DocumentExtractor: Send + Sync {
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<ExtractedDocument>;
}

pub struct DocumentExtractorRegistry {
    markdown: MarkdownExtractor,
    text: TextExtractor,
    pdf: PdfExtractor,
}

impl DocumentExtractorRegistry {
    pub fn new() -> Self {
        Self {
            markdown: MarkdownExtractor,
            text: TextExtractor,
            pdf: PdfExtractor,
        }
    }

    pub fn extract(&self, path: &Path, bytes: &[u8]) -> Result<ExtractedDocument> {
        self.extractor_for_path(path)
            .ok_or_else(|| anyhow!("unsupported document extension for {}", path.display()))?
            .extract(path, bytes)
    }

    fn extractor_for_path(&self, path: &Path) -> Option<&dyn DocumentExtractor> {
        match lowercase_extension(path).as_deref() {
            Some("md") => Some(&self.markdown),
            Some("txt") => Some(&self.text),
            Some("pdf") => Some(&self.pdf),
            _ => None,
        }
    }
}

impl Default for DocumentExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn file_fields(path: &Path) -> (String, String, String) {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    let extension = lowercase_extension(path).unwrap_or_default();
    let title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();

    (file_name, extension, title)
}

pub(crate) fn lowercase_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}
