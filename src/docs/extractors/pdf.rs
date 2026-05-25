use crate::docs::extractors::{file_fields, DocumentExtractor};
use crate::docs::models::{ExtractedDocument, ExtractedSection};
use anyhow::{anyhow, Result};
use std::path::Path;

pub struct PdfExtractor;

impl DocumentExtractor for PdfExtractor {
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<ExtractedDocument> {
        let (file_name, extension, title) = file_fields(path);
        let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)
            .map_err(|e| anyhow!("failed to extract PDF text: {}", e))?;

        let sections = pages
            .into_iter()
            .enumerate()
            .filter_map(|(index, page)| {
                let text = page.trim();
                if text.is_empty() {
                    return None;
                }

                let page_number = index + 1;
                Some(ExtractedSection {
                    heading_path: Vec::new(),
                    page_start: Some(page_number),
                    page_end: Some(page_number),
                    text: text.to_string(),
                })
            })
            .collect();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_extractor_returns_page_numbered_text_for_text_selectable_pdf() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("documents")
            .join("text-selectable-two-pages.pdf");
        let bytes = std::fs::read(&fixture).unwrap();

        let document = PdfExtractor.extract(&fixture, &bytes).unwrap();

        assert_eq!(document.sections.len(), 2);
        assert_eq!(document.sections[0].page_start, Some(1));
        assert_eq!(document.sections[0].page_end, Some(1));
        assert!(document.sections[0]
            .text
            .contains("Page one authentication"));
        assert_eq!(document.sections[1].page_start, Some(2));
        assert_eq!(document.sections[1].page_end, Some(2));
        assert!(document.sections[1].text.contains("Page two cache notes"));
    }
}
