use crate::models::{ExtractedDocument, ExtractedSection};
use anyhow::{anyhow, Result};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use std::path::Path;

pub struct DocumentExtractorRegistry;

impl DocumentExtractorRegistry {
    pub fn new() -> Self {
        Self
    }

    pub fn extract(&self, path: &Path, bytes: &[u8]) -> Result<ExtractedDocument> {
        match lowercase_extension(path).as_deref() {
            Some("md") => extract_markdown(path, bytes),
            Some("txt") => extract_text(path, bytes),
            Some("pdf") => extract_pdf(path, bytes),
            _ => Err(anyhow!(
                "unsupported document extension for {}",
                path.display()
            )),
        }
    }
}

impl Default for DocumentExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn lowercase_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

fn file_fields(path: &Path) -> (String, String, String) {
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

fn extract_markdown(path: &Path, bytes: &[u8]) -> Result<ExtractedDocument> {
    let content = std::str::from_utf8(bytes)
        .map_err(|e| anyhow!("failed to decode markdown as UTF-8: {}", e))?;
    let (file_name, extension, fallback_title) = file_fields(path);
    let mut sections = Vec::new();
    let mut heading_path: Vec<String> = Vec::new();
    let mut body = String::new();
    let mut heading_level: Option<HeadingLevel> = None;
    let mut heading_text = String::new();
    let mut title: Option<String> = None;

    for event in Parser::new(content) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_section(&mut sections, &heading_path, &mut body);
                heading_level = Some(level);
                heading_text.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                let text = collapse_whitespace(&heading_text);
                if !text.is_empty() {
                    if title.is_none() && heading_level == Some(HeadingLevel::H1) {
                        title = Some(text.clone());
                    }
                    if let Some(level) = heading_level.take() {
                        let depth = heading_depth(level);
                        heading_path.truncate(depth.saturating_sub(1));
                        heading_path.push(text);
                    }
                }
                heading_text.clear();
            }
            Event::Text(text) | Event::Code(text) => {
                if heading_level.is_some() {
                    heading_text.push_str(&text);
                } else {
                    body.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if heading_level.is_some() {
                    heading_text.push(' ');
                } else {
                    body.push('\n');
                }
            }
            Event::Rule if heading_level.is_none() => body.push('\n'),
            _ => {}
        }
    }

    flush_section(&mut sections, &heading_path, &mut body);

    Ok(ExtractedDocument {
        document_id: path.to_string_lossy().to_string(),
        file_path: path.to_string_lossy().to_string(),
        file_name,
        extension,
        title: title.unwrap_or(fallback_title),
        sections,
    })
}

fn extract_text(path: &Path, bytes: &[u8]) -> Result<ExtractedDocument> {
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

fn extract_pdf(path: &Path, bytes: &[u8]) -> Result<ExtractedDocument> {
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

fn flush_section(sections: &mut Vec<ExtractedSection>, heading_path: &[String], body: &mut String) {
    let text = body.trim();
    if text.is_empty() {
        body.clear();
        return;
    }

    sections.push(ExtractedSection {
        heading_path: heading_path.to_vec(),
        page_start: None,
        page_end: None,
        text: text.to_string(),
    });
    body.clear();
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

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn heading_depth(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_extractor_captures_nested_heading_path() {
        let markdown = br#"# Auth Guide

Top-level authentication overview.

## Tokens

Token refresh authentication details.

### Rotation

Rotate tokens after login.
"#;

        let document = DocumentExtractorRegistry::new()
            .extract(Path::new("/tmp/docs/auth-guide.md"), markdown)
            .unwrap();

        assert_eq!(document.title, "Auth Guide");
        assert_eq!(document.sections.len(), 3);
        assert_eq!(document.sections[0].heading_path, vec!["Auth Guide"]);
        assert_eq!(
            document.sections[1].heading_path,
            vec!["Auth Guide", "Tokens"]
        );
        assert_eq!(
            document.sections[2].heading_path,
            vec!["Auth Guide", "Tokens", "Rotation"]
        );
    }

    #[test]
    fn text_extractor_splits_blank_line_paragraphs() {
        let text = b"Authentication cache warmup.\nStill first paragraph.\n\nSecond paragraph.\n\n\nThird paragraph.";
        let document = DocumentExtractorRegistry::new()
            .extract(Path::new("/tmp/docs/notes.txt"), text)
            .unwrap();

        assert_eq!(document.sections.len(), 3);
        assert_eq!(document.title, "notes");
    }
}
