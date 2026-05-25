use crate::docs::extractors::{file_fields, DocumentExtractor};
use crate::docs::models::{ExtractedDocument, ExtractedSection};
use anyhow::{anyhow, Result};
use std::path::Path;

pub struct TextExtractor;

impl DocumentExtractor for TextExtractor {
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<ExtractedDocument> {
        let content = std::str::from_utf8(bytes)
            .map_err(|e| anyhow!("failed to decode text file as UTF-8: {}", e))?;
        let (file_name, extension, title) = file_fields(path);
        let mut sections = Vec::new();
        let mut paragraph_lines = Vec::new();

        for line in content.lines() {
            if line.trim().is_empty() {
                push_paragraph(&mut sections, &mut paragraph_lines);
                continue;
            }
            paragraph_lines.push(line.trim().to_string());
        }
        push_paragraph(&mut sections, &mut paragraph_lines);

        Ok(ExtractedDocument {
            document_id: path.to_string_lossy().to_string(),
            file_path: path.to_string_lossy().to_string(),
            file_name,
            extension,
            title,
            sections,
        })
    }
}

fn push_paragraph(sections: &mut Vec<ExtractedSection>, paragraph_lines: &mut Vec<String>) {
    if paragraph_lines.is_empty() {
        return;
    }

    sections.push(ExtractedSection {
        heading_path: Vec::new(),
        page_start: None,
        page_end: None,
        text: paragraph_lines.join("\n"),
    });
    paragraph_lines.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_extractor_splits_blank_line_paragraphs() {
        let text = b"Authentication cache warmup.\nStill first paragraph.\n\nSecond paragraph.\n\n\nThird paragraph.";

        let document = TextExtractor
            .extract(Path::new("/tmp/docs/notes.txt"), text)
            .unwrap();

        assert_eq!(document.title, "notes");
        assert_eq!(document.sections.len(), 3);
        assert!(document
            .sections
            .iter()
            .all(|section| section.heading_path.is_empty()));
        assert!(document
            .sections
            .iter()
            .all(|section| section.page_start.is_none()));
    }
}
