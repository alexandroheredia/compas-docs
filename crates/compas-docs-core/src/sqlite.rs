use crate::models::DocumentChunk;
use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkRow {
    pub chunk_id: String,
    pub folder_id: String,
    pub document_id: String,
    pub absolute_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub title: String,
    pub heading_path: Vec<String>,
    pub page_start: Option<usize>,
    pub page_end: Option<usize>,
    pub preview: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub updated_at: u64,
}

pub fn open_database(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS folders (
            folder_id TEXT PRIMARY KEY,
            folder_path TEXT NOT NULL,
            display_name TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS documents (
            document_id TEXT PRIMARY KEY,
            folder_id TEXT NOT NULL,
            absolute_path TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            file_name TEXT NOT NULL,
            title TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(folder_id) REFERENCES folders(folder_id) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS chunks (
            chunk_id TEXT PRIMARY KEY,
            folder_id TEXT NOT NULL,
            document_id TEXT NOT NULL,
            absolute_path TEXT NOT NULL,
            relative_path TEXT NOT NULL,
            file_name TEXT NOT NULL,
            title TEXT NOT NULL,
            heading_path_json TEXT NOT NULL,
            page_start INTEGER,
            page_end INTEGER,
            preview TEXT NOT NULL,
            text TEXT NOT NULL,
            embedding BLOB NOT NULL,
            updated_at INTEGER NOT NULL,
            FOREIGN KEY(folder_id) REFERENCES folders(folder_id) ON DELETE CASCADE,
            FOREIGN KEY(document_id) REFERENCES documents(document_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_chunks_folder_id ON chunks(folder_id);
        CREATE INDEX IF NOT EXISTS idx_chunks_absolute_path ON chunks(absolute_path);
        ",
    )?;
    Ok(conn)
}

pub fn upsert_folder(
    conn: &Connection,
    folder_id: &str,
    folder_path: &str,
    display_name: &str,
) -> Result<()> {
    let now = unix_timestamp();
    conn.execute(
        "INSERT INTO folders (folder_id, folder_path, display_name, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(folder_id) DO UPDATE SET folder_path = excluded.folder_path, display_name = excluded.display_name, updated_at = excluded.updated_at",
        params![folder_id, folder_path, display_name, now],
    )?;
    Ok(())
}

pub fn replace_file_chunks(
    conn: &mut Connection,
    folder_id: &str,
    folder_path: &Path,
    display_name: &str,
    absolute_path: &str,
    chunks: &[DocumentChunk],
    embeddings: &[Vec<f32>],
) -> Result<()> {
    if chunks.len() != embeddings.len() {
        return Err(anyhow!("chunks/embeddings length mismatch"));
    }

    let relative_path = Path::new(absolute_path)
        .strip_prefix(folder_path)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| absolute_path.to_string());
    let now = unix_timestamp();
    let tx = conn.transaction()?;

    upsert_folder(&tx, folder_id, &folder_path.to_string_lossy(), display_name)?;
    tx.execute(
        "DELETE FROM chunks WHERE folder_id = ?1 AND absolute_path = ?2",
        params![folder_id, absolute_path],
    )?;
    tx.execute(
        "DELETE FROM documents WHERE folder_id = ?1 AND absolute_path = ?2",
        params![folder_id, absolute_path],
    )?;

    if let Some(first) = chunks.first() {
        tx.execute(
            "INSERT INTO documents (document_id, folder_id, absolute_path, relative_path, file_name, title, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(document_id) DO UPDATE SET absolute_path = excluded.absolute_path, relative_path = excluded.relative_path, file_name = excluded.file_name, title = excluded.title, updated_at = excluded.updated_at",
            params![
                first.document_id,
                folder_id,
                absolute_path,
                relative_path,
                first.file_name,
                first.title,
                now
            ],
        )?;
    }

    for (chunk, embedding) in chunks.iter().zip(embeddings) {
        let heading_path_json = serde_json::to_string(&chunk.heading_path)?;
        tx.execute(
            "INSERT INTO chunks (chunk_id, folder_id, document_id, absolute_path, relative_path, file_name, title, heading_path_json, page_start, page_end, preview, text, embedding, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(chunk_id) DO UPDATE SET document_id = excluded.document_id, absolute_path = excluded.absolute_path, relative_path = excluded.relative_path, file_name = excluded.file_name, title = excluded.title, heading_path_json = excluded.heading_path_json, page_start = excluded.page_start, page_end = excluded.page_end, preview = excluded.preview, text = excluded.text, embedding = excluded.embedding, updated_at = excluded.updated_at",
            params![
                chunk.id,
                folder_id,
                chunk.document_id,
                absolute_path,
                relative_path,
                chunk.file_name,
                chunk.title,
                heading_path_json,
                chunk.page_start.map(|value| value as i64),
                chunk.page_end.map(|value| value as i64),
                chunk.preview,
                chunk.text,
                encode_embedding(embedding),
                now,
            ],
        )?;
    }

    tx.commit()?;
    Ok(())
}

pub fn delete_file(conn: &Connection, folder_id: &str, absolute_path: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM chunks WHERE folder_id = ?1 AND absolute_path = ?2",
        params![folder_id, absolute_path],
    )?;
    conn.execute(
        "DELETE FROM documents WHERE folder_id = ?1 AND absolute_path = ?2",
        params![folder_id, absolute_path],
    )?;
    Ok(())
}

pub fn list_chunks_for_folder(conn: &Connection, folder_id: &str) -> Result<Vec<ChunkRow>> {
    let mut stmt = conn.prepare(
        "SELECT chunk_id, folder_id, document_id, absolute_path, relative_path, file_name, title, heading_path_json, page_start, page_end, preview, text, embedding, updated_at
         FROM chunks WHERE folder_id = ?1 ORDER BY absolute_path, chunk_id",
    )?;
    let rows = stmt
        .query_map(params![folder_id], row_to_chunk)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn chunks_for_file(
    conn: &Connection,
    folder_id: &str,
    absolute_path: &str,
) -> Result<Vec<ChunkRow>> {
    let mut stmt = conn.prepare(
        "SELECT chunk_id, folder_id, document_id, absolute_path, relative_path, file_name, title, heading_path_json, page_start, page_end, preview, text, embedding, updated_at
         FROM chunks WHERE folder_id = ?1 AND absolute_path = ?2 ORDER BY chunk_id",
    )?;
    let rows = stmt
        .query_map(params![folder_id, absolute_path], row_to_chunk)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn hydrate_chunks(
    conn: &Connection,
    chunk_ids: &[String],
) -> Result<HashMap<String, ChunkRow>> {
    let mut output = HashMap::new();
    for chunk_id in chunk_ids {
        if let Some(row) = chunk_by_id(conn, chunk_id)? {
            output.insert(chunk_id.clone(), row);
        }
    }
    Ok(output)
}

pub fn chunk_by_id(conn: &Connection, chunk_id: &str) -> Result<Option<ChunkRow>> {
    conn.query_row(
        "SELECT chunk_id, folder_id, document_id, absolute_path, relative_path, file_name, title, heading_path_json, page_start, page_end, preview, text, embedding, updated_at
         FROM chunks WHERE chunk_id = ?1",
        params![chunk_id],
        row_to_chunk,
    )
    .optional()
    .map_err(Into::into)
}

pub fn document_count_for_folder(conn: &Connection, folder_id: &str) -> Result<usize> {
    let count = conn.query_row(
        "SELECT COUNT(*) FROM documents WHERE folder_id = ?1",
        params![folder_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count.max(0) as usize)
}

pub fn chunk_count_for_folder(conn: &Connection, folder_id: &str) -> Result<usize> {
    let count = conn.query_row(
        "SELECT COUNT(*) FROM chunks WHERE folder_id = ?1",
        params![folder_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(count.max(0) as usize)
}

fn row_to_chunk(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChunkRow> {
    let heading_path_json: String = row.get(7)?;
    let embedding_blob: Vec<u8> = row.get(12)?;
    Ok(ChunkRow {
        chunk_id: row.get(0)?,
        folder_id: row.get(1)?,
        document_id: row.get(2)?,
        absolute_path: row.get(3)?,
        relative_path: row.get(4)?,
        file_name: row.get(5)?,
        title: row.get(6)?,
        heading_path: serde_json::from_str(&heading_path_json).map_err(json_err)?,
        page_start: row.get::<_, Option<i64>>(8)?.map(|value| value as usize),
        page_end: row.get::<_, Option<i64>>(9)?.map(|value| value as usize),
        preview: row.get(10)?,
        text: row.get(11)?,
        embedding: decode_embedding(&embedding_blob).map_err(json_err)?,
        updated_at: row.get::<_, i64>(13)? as u64,
    })
}

fn json_err(err: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            err.to_string(),
        )),
    )
}

pub fn encode_embedding(values: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub fn decode_embedding(bytes: &[u8]) -> Result<Vec<f32>> {
    if !bytes.len().is_multiple_of(4) {
        return Err(anyhow!("invalid embedding blob length {}", bytes.len()));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_chunk() -> DocumentChunk {
        DocumentChunk {
            id: "chunk-1".to_string(),
            document_id: "doc-1".to_string(),
            file_path: "/tmp/docs/policy.md".to_string(),
            file_name: "policy.md".to_string(),
            extension: "md".to_string(),
            title: "Policy".to_string(),
            heading_path: vec!["Policy".to_string()],
            page_start: None,
            page_end: None,
            text: "Renewal date is January 15.".to_string(),
            preview: "Renewal date is January 15.".to_string(),
            enriched_text: "ignored".to_string(),
            kind: "section".to_string(),
        }
    }

    #[test]
    fn sqlite_store_initializes_schema_and_round_trips_chunk() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("documents.sqlite3");
        let mut conn = open_database(&db_path).unwrap();
        let chunk = sample_chunk();

        replace_file_chunks(
            &mut conn,
            "folder-1",
            Path::new("/tmp/docs"),
            "docs",
            &chunk.file_path,
            &[chunk.clone()],
            &[vec![1.0, 2.0, 3.0]],
        )
        .unwrap();

        let reloaded = chunk_by_id(&conn, &chunk.id).unwrap().unwrap();
        assert_eq!(reloaded.chunk_id, chunk.id);
        assert_eq!(reloaded.title, "Policy");
        assert_eq!(reloaded.embedding, vec![1.0, 2.0, 3.0]);
    }
}
