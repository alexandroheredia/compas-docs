use crate::docs::extractors::{file_fields, DocumentExtractor};
use crate::docs::models::{ExtractedDocument, ExtractedSection};
use anyhow::{anyhow, Result};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use std::path::Path;

pub struct MarkdownExtractor;

impl DocumentExtractor for MarkdownExtractor {
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<ExtractedDocument> {
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

        let document = MarkdownExtractor
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
}
