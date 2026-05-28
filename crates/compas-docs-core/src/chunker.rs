use crate::models::{DocumentChunk, ExtractedDocument};

const MAX_SECTION_TOKENS: usize = 900;
const TARGET_SECTION_TOKENS: usize = 800;
const MIN_SECTION_TOKENS: usize = 500;
const SECTION_OVERLAP_TOKENS: usize = 100;

pub fn chunk_document(document: &ExtractedDocument) -> Vec<DocumentChunk> {
    let mut chunks = Vec::new();
    let title = if document.title.trim().is_empty() {
        file_stem(&document.file_name)
    } else {
        document.title.clone()
    };

    for (section_index, section) in document.sections.iter().enumerate() {
        let windows = split_section_windows(&section.text);
        let split_pdf_page = document.extension == "pdf" && windows.len() > 1;

        for (window_index, window) in windows.into_iter().enumerate() {
            let kind = match document.extension.as_str() {
                "txt" => "paragraph",
                "pdf" if split_pdf_page => "page_part",
                "pdf" => "page",
                _ => "section",
            }
            .to_string();

            let preview = preview_text(&window);
            let section_label = if section.heading_path.is_empty() {
                "(root)".to_string()
            } else {
                section.heading_path.join(" > ")
            };
            let page_label = section
                .page_start
                .map(|page| page.to_string())
                .unwrap_or_else(|| "n/a".to_string());
            let enriched_text = format!(
                "Document: {}\nTitle: {}\nSection: {}\nPage: {}\n\n{}",
                document.file_name, title, section_label, page_label, window
            );

            chunks.push(DocumentChunk {
                id: stable_chunk_id(&document.file_path, section_index, window_index),
                document_id: document.document_id.clone(),
                file_path: document.file_path.clone(),
                file_name: document.file_name.clone(),
                extension: document.extension.clone(),
                title: title.clone(),
                heading_path: section.heading_path.clone(),
                page_start: section.page_start,
                page_end: section.page_end,
                text: window,
                preview,
                enriched_text,
                kind,
            });
        }
    }

    chunks
}

fn stable_chunk_id(file_path: &str, section_index: usize, window_index: usize) -> String {
    format!("{}::{}::{}", file_path, section_index, window_index)
}

fn split_section_windows(text: &str) -> Vec<String> {
    let token_count = text.split_whitespace().count();
    if token_count <= MAX_SECTION_TOKENS {
        let trimmed = text.trim();
        return if trimmed.is_empty() {
            Vec::new()
        } else {
            vec![trimmed.to_string()]
        };
    }

    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut windows = Vec::new();
    let mut start = 0usize;

    while start < tokens.len() {
        let remaining = tokens.len() - start;
        if remaining <= MAX_SECTION_TOKENS {
            windows.push(tokens[start..].join(" "));
            break;
        }

        let mut end = (start + TARGET_SECTION_TOKENS).min(tokens.len());
        let tail = tokens.len().saturating_sub(end);
        if tail < MIN_SECTION_TOKENS {
            end = tokens.len() - MIN_SECTION_TOKENS;
        }
        if end.saturating_sub(start) < MIN_SECTION_TOKENS {
            end = (start + MIN_SECTION_TOKENS).min(tokens.len());
        }

        windows.push(tokens[start..end].join(" "));
        start = end.saturating_sub(SECTION_OVERLAP_TOKENS);
    }

    windows
}

fn preview_text(text: &str) -> String {
    collapse_whitespace(text).chars().take(240).collect()
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
    use crate::models::{ExtractedDocument, ExtractedSection};

    #[test]
    fn document_chunker_enriches_context_and_preserves_page_bounds() {
        let markdown = ExtractedDocument {
            document_id: "doc-md".to_string(),
            file_path: "/tmp/docs/auth-guide.md".to_string(),
            file_name: "auth-guide.md".to_string(),
            extension: "md".to_string(),
            title: "Auth Guide".to_string(),
            sections: vec![ExtractedSection {
                heading_path: vec!["Auth Guide".to_string(), "Tokens".to_string()],
                page_start: None,
                page_end: None,
                text: "Token refresh authentication details.".to_string(),
            }],
        };

        let markdown_chunks = chunk_document(&markdown);
        assert_eq!(markdown_chunks.len(), 1);
        assert_eq!(markdown_chunks[0].kind, "section");
        assert_eq!(
            markdown_chunks[0].enriched_text,
            "Document: auth-guide.md\nTitle: Auth Guide\nSection: Auth Guide > Tokens\nPage: n/a\n\nToken refresh authentication details."
        );
        assert_eq!(markdown_chunks[0].id, "/tmp/docs/auth-guide.md::0::0");
    }
}
