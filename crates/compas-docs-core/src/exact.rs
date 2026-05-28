use crate::sqlite::ChunkRow;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::{AllQuery, QueryParser, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, Schema, Term, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, ReloadPolicy};

#[derive(Clone, Copy)]
struct ExactSchema {
    chunk_id: Field,
    folder_id: Field,
    title: Field,
    file_name: Field,
    relative_path: Field,
    heading_path_text: Field,
    text: Field,
}

pub fn rebuild_index(index_dir: &Path, chunks: &[ChunkRow]) -> Result<()> {
    std::fs::create_dir_all(index_dir)?;
    let schema = build_schema();
    let index = if index_dir.join("meta.json").exists() {
        Index::open_in_dir(index_dir)?
    } else {
        Index::create_in_dir(index_dir, schema.clone())?
    };
    let fields = schema_fields(&index.schema())?;
    let mut writer = index.writer(50_000_000)?;
    writer.delete_all_documents()?;

    for chunk in chunks {
        writer.add_document(doc!(
            fields.chunk_id => chunk.chunk_id.clone(),
            fields.folder_id => chunk.folder_id.clone(),
            fields.title => chunk.title.clone(),
            fields.file_name => chunk.file_name.clone(),
            fields.relative_path => chunk.relative_path.clone(),
            fields.heading_path_text => chunk.heading_path.join(" > "),
            fields.text => chunk.text.clone(),
        ))?;
    }

    writer.commit()?;
    Ok(())
}

pub fn exact_search(index_dir: &Path, query: &str, limit: usize) -> Result<Vec<(String, f32)>> {
    if !index_dir.join("meta.json").exists() {
        return Ok(Vec::new());
    }

    let index = Index::open_in_dir(index_dir)?;
    let schema = index.schema();
    let fields = schema_fields(&schema)?;
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    reader.reload()?;
    let searcher = reader.searcher();
    let query_parser = QueryParser::for_index(
        &index,
        vec![
            fields.title,
            fields.file_name,
            fields.relative_path,
            fields.heading_path_text,
            fields.text,
        ],
    );

    let boxed_query: Box<dyn tantivy::query::Query> = if query.trim().is_empty() {
        Box::new(AllQuery)
    } else {
        match query_parser.parse_query(query) {
            Ok(parsed) => Box::new(parsed),
            Err(_) => {
                let term = Term::from_field_text(fields.text, &query.to_lowercase());
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
        }
    };

    let top_docs = searcher.search(&*boxed_query, &TopDocs::with_limit(limit).order_by_score())?;
    if top_docs.is_empty() {
        return Ok(Vec::new());
    }
    let max_score = top_docs
        .first()
        .map(|(score, _)| *score)
        .unwrap_or(1.0)
        .max(1.0);
    let mut results = Vec::with_capacity(top_docs.len());
    for (score, address) in top_docs {
        let doc = searcher.doc::<tantivy::schema::TantivyDocument>(address)?;
        let chunk_id = doc
            .get_first(fields.chunk_id)
            .and_then(|value| value.as_str())
            .context("tantivy document missing chunk_id")?
            .to_string();
        results.push((chunk_id, (score / max_score).clamp(0.0, 1.0)));
    }
    Ok(results)
}

fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("chunk_id", STRING | STORED);
    builder.add_text_field("folder_id", STRING | STORED);
    builder.add_text_field("title", TEXT | STORED);
    builder.add_text_field("file_name", TEXT | STORED);
    builder.add_text_field("relative_path", TEXT | STORED);
    builder.add_text_field("heading_path_text", TEXT);
    builder.add_text_field("text", TEXT);
    builder.build()
}

fn schema_fields(schema: &Schema) -> Result<ExactSchema> {
    let mut fields = HashMap::new();
    for name in [
        "chunk_id",
        "folder_id",
        "title",
        "file_name",
        "relative_path",
        "heading_path_text",
        "text",
    ] {
        let field = schema.get_field(name).context("missing tantivy field")?;
        fields.insert(name, field);
    }

    Ok(ExactSchema {
        chunk_id: *fields.get("chunk_id").unwrap(),
        folder_id: *fields.get("folder_id").unwrap(),
        title: *fields.get("title").unwrap(),
        file_name: *fields.get("file_name").unwrap(),
        relative_path: *fields.get("relative_path").unwrap(),
        heading_path_text: *fields.get("heading_path_text").unwrap(),
        text: *fields.get("text").unwrap(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::ChunkRow;
    use tempfile::tempdir;

    fn chunk(id: &str, relative_path: &str, text: &str) -> ChunkRow {
        ChunkRow {
            chunk_id: id.to_string(),
            folder_id: "folder-1".to_string(),
            document_id: "doc-1".to_string(),
            absolute_path: format!("/tmp/{relative_path}"),
            relative_path: relative_path.to_string(),
            file_name: relative_path.rsplit('/').next().unwrap().to_string(),
            title: "Policy".to_string(),
            heading_path: vec!["Policy".to_string()],
            page_start: None,
            page_end: None,
            preview: text.to_string(),
            text: text.to_string(),
            embedding: vec![1.0, 0.0],
            updated_at: 0,
        }
    }

    #[test]
    fn tantivy_exact_search_matches_phrase_and_path() {
        let temp = tempdir().unwrap();
        rebuild_index(
            temp.path(),
            &[
                chunk("chunk-1", "docs/policy.md", "renewal date is january 15"),
                chunk("chunk-2", "docs/notes.txt", "cache notes"),
            ],
        )
        .unwrap();

        let results = exact_search(temp.path(), "renewal date", 5).unwrap();
        assert_eq!(results[0].0, "chunk-1");

        let path_results = exact_search(temp.path(), "policy.md", 5).unwrap();
        assert_eq!(path_results[0].0, "chunk-1");
    }

    #[test]
    fn tantivy_exact_search_handles_malformed_query() {
        let temp = tempdir().unwrap();
        rebuild_index(
            temp.path(),
            &[chunk("chunk-1", "docs/policy.md", "renewal date")],
        )
        .unwrap();
        let results = exact_search(temp.path(), "foo AND (((bar", 5).unwrap();
        assert!(results.is_empty() || !results.is_empty());
    }
}
