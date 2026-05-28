use crate::chunker::chunk_document;
use crate::config::{
    AppConfig, EmbedderConfig, IndexConfig, RepoConfig, ServerConfig, StoreConfig,
};
use crate::embedder::{build_embedder, EmbedMode};
use crate::exact;
use crate::extractors::{lowercase_extension, DocumentExtractorRegistry};
use crate::models::{
    default_folder_file_types, normalize_or_default_folder_file_types, DocumentStoragePaths,
    FolderRegistry,
};
use crate::ranking::merge_hybrid;
use crate::sqlite::{self, ChunkRow};
use crate::vector;
use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

pub use crate::models::{FolderRecord, LibraryStats, SearchDocumentItem};

/// Progress events emitted while indexing a folder so UIs can render real progress
/// instead of a generic spinner. Kept as a small enum so we can extend it without
/// breaking the Tauri event contract.
#[derive(Debug, Clone)]
pub enum IndexProgress {
    /// Indexing started; `total_files` is the number of files in scope after filtering.
    Started { total_files: usize },
    /// A file has been processed (either embedded, skipped, or unchanged).
    /// `processed_files` is the running count of files processed so far.
    File {
        processed_files: usize,
        total_files: usize,
        path: String,
        status: IndexFileStatus,
    },
    /// All files have been processed and we are finalizing search indices.
    Finalizing { total_files: usize },
}

#[derive(Debug, Clone, Copy)]
pub enum IndexFileStatus {
    Indexed,
    Unchanged,
    Skipped,
    Failed,
}

impl IndexFileStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Indexed => "indexed",
            Self::Unchanged => "unchanged",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// Type-erased progress callback. Synchronous because indexing already runs on a
/// dedicated task and the callback only needs to forward events (e.g. via Tauri emit).
pub type ProgressCallback = Box<dyn Fn(IndexProgress) + Send + Sync>;

pub fn default_document_config(path: &Path) -> AppConfig {
    let storage = document_storage_paths(path);
    AppConfig {
        repo: RepoConfig {
            path: path.to_string_lossy().to_string(),
            include: vec!["**/*.md".into(), "**/*.txt".into(), "**/*.pdf".into()],
            exclude: vec![],
        },
        embedder: EmbedderConfig {
            provider: "fastembed".into(),
            model: "nomic-embed-text-v1.5-q".into(),
            query_prefix: None,
            doc_prefix: None,
        },
        store: StoreConfig {
            provider: "document-hybrid".into(),
            path: storage.store_path.to_string_lossy().to_string(),
            vector_name: "default".into(),
        },
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: "3001".into(),
        },
        index: IndexConfig {
            kind: "document".into(),
            chunk_by: "function".into(),
            watch: false,
        },
    }
}

pub fn normalize_document_storage(config: &mut AppConfig) {
    if config.index.kind != "document" {
        return;
    }
    let repo_path = std::fs::canonicalize(&config.repo.path)
        .unwrap_or_else(|_| PathBuf::from(&config.repo.path));
    let storage = document_storage_paths(&repo_path);
    config.store.path = storage.store_path.to_string_lossy().to_string();
    if config.store.provider.is_empty() {
        config.store.provider = "document-hybrid".into();
    }
}

pub fn document_storage_paths(source_path: &Path) -> DocumentStoragePaths {
    let canonical_source =
        std::fs::canonicalize(source_path).unwrap_or_else(|_| source_path.to_path_buf());
    let source_id = stable_folder_id(&canonical_source);
    let root = document_library_root().join("indices").join(source_id);
    DocumentStoragePaths {
        root: root.clone(),
        store_path: root.join("edge-shard"),
        manifest_path: root.join("manifest.json"),
        sqlite_path: root.join("documents.sqlite3"),
        tantivy_dir: root.join("tantivy"),
        hnsw_dir: root.join("hnsw"),
    }
}

pub fn document_library_root() -> PathBuf {
    std::env::var_os("COMPAS_DOCS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
                .join("compas-docs")
        })
}

pub fn stable_folder_id(path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub fn load_folder_registry() -> FolderRegistry {
    let path = folder_registry_path();
    if !path.exists() {
        return FolderRegistry::default();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

pub fn save_folder_registry(registry: &FolderRegistry) -> Result<()> {
    let path = folder_registry_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(registry)?)?;
    Ok(())
}

pub fn folder_registry_path() -> PathBuf {
    document_library_root().join("library.json")
}

pub fn add_folder(path: &Path) -> Result<FolderRecord> {
    add_folder_with_file_types(path, None)
}

pub fn add_folder_with_file_types(
    path: &Path,
    file_types: Option<Vec<String>>,
) -> Result<FolderRecord> {
    let canonical = std::fs::canonicalize(path)?;
    if !canonical.is_dir() {
        return Err(anyhow!("'{}' is not a directory", canonical.display()));
    }
    let mut registry = load_folder_registry();
    let id = stable_folder_id(&canonical);
    let storage = document_storage_paths(&canonical);
    let normalized_file_types = match file_types {
        Some(selected) => normalize_or_default_folder_file_types(&selected),
        None => registry
            .folders
            .iter()
            .find(|folder| folder.id == id)
            .map(|folder| normalize_or_default_folder_file_types(&folder.file_types))
            .unwrap_or_else(default_folder_file_types),
    };

    let record = FolderRecord {
        id: id.clone(),
        path: canonical.to_string_lossy().to_string(),
        display_name: canonical
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("folder")
            .to_string(),
        storage_path: storage.store_path.to_string_lossy().to_string(),
        file_types: normalized_file_types,
        last_indexed_at: registry
            .folders
            .iter()
            .find(|folder| folder.id == id)
            .and_then(|folder| folder.last_indexed_at),
        watch_enabled: false,
    };

    if let Some(existing) = registry.folders.iter_mut().find(|folder| folder.id == id) {
        *existing = record.clone();
    } else {
        registry.folders.push(record.clone());
        registry
            .folders
            .sort_by(|a, b| a.display_name.cmp(&b.display_name));
    }

    save_folder_registry(&registry)?;
    Ok(record)
}

pub fn list_folders() -> Vec<FolderRecord> {
    let mut registry = load_folder_registry();
    for folder in &mut registry.folders {
        folder.file_types = normalize_or_default_folder_file_types(&folder.file_types);
    }
    registry
        .folders
        .sort_by(|a, b| a.display_name.cmp(&b.display_name));
    registry.folders
}

pub fn remove_folder(id: &str) -> Result<bool> {
    let mut registry = load_folder_registry();
    let before = registry.folders.len();
    let removed: Vec<FolderRecord> = registry
        .folders
        .iter()
        .filter(|folder| folder.id == id)
        .cloned()
        .collect();
    registry.folders.retain(|folder| folder.id != id);
    if registry.folders.len() == before {
        return Ok(false);
    }
    save_folder_registry(&registry)?;
    for folder in removed {
        let storage = document_storage_paths(Path::new(&folder.path));
        if storage.root.exists() {
            std::fs::remove_dir_all(&storage.root)?;
        }
    }
    Ok(true)
}

pub fn library_stats() -> Result<LibraryStats> {
    let folders = list_folders();
    let indexed_folder_count = folders
        .iter()
        .filter(|folder| folder.last_indexed_at.is_some())
        .count();
    let last_indexed_at = folders
        .iter()
        .filter_map(|folder| folder.last_indexed_at)
        .max();

    let mut document_count = 0usize;
    let mut chunk_count = 0usize;

    for folder in &folders {
        if folder.last_indexed_at.is_none() {
            continue;
        }

        let storage = document_storage_paths(Path::new(&folder.path));
        if !storage.sqlite_path.exists() {
            continue;
        }

        let conn = sqlite::open_database(&storage.sqlite_path)?;
        document_count += sqlite::document_count_for_folder(&conn, &folder.id)?;
        chunk_count += sqlite::chunk_count_for_folder(&conn, &folder.id)?;
    }

    Ok(LibraryStats {
        folder_count: folders.len(),
        indexed_folder_count,
        document_count,
        chunk_count,
        last_indexed_at,
    })
}

pub async fn index_folder(path: &Path, config: AppConfig) -> Result<FolderRecord> {
    index_folder_with_file_types(path, config, None).await
}

pub async fn index_folder_with_file_types(
    path: &Path,
    config: AppConfig,
    file_types: Option<Vec<String>>,
) -> Result<FolderRecord> {
    index_folder_with_progress(path, config, file_types, None).await
}

pub async fn index_folder_with_progress(
    path: &Path,
    mut config: AppConfig,
    file_types: Option<Vec<String>>,
    progress: Option<ProgressCallback>,
) -> Result<FolderRecord> {
    let emit = |event: IndexProgress| {
        if let Some(callback) = progress.as_ref() {
            callback(event);
        }
    };

    let canonical = std::fs::canonicalize(path)?;
    config.repo.path = canonical.to_string_lossy().to_string();
    normalize_document_storage(&mut config);
    let storage = document_storage_paths(&canonical);
    std::fs::create_dir_all(&storage.root)?;
    std::fs::create_dir_all(&storage.store_path)?;

    let record = add_folder_with_file_types(&canonical, file_types)?;
    let mut conn = sqlite::open_database(&storage.sqlite_path)?;
    let embedder = build_embedder(&config.embedder)?;
    let extractors = DocumentExtractorRegistry::new();

    let old_manifest = load_manifest(&storage.manifest_path);
    let current_files = collect_document_files(
        &canonical,
        &config.repo.include,
        &config.repo.exclude,
        &record.file_types,
    )?;
    let total_files = current_files.len();
    emit(IndexProgress::Started { total_files });

    let current_paths: HashSet<String> = current_files
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    let deleted: Vec<String> = old_manifest
        .keys()
        .filter(|path| !current_paths.contains(*path))
        .cloned()
        .collect();
    for deleted_path in &deleted {
        sqlite::delete_file(&conn, &record.id, deleted_path)?;
    }

    let mut new_manifest = HashMap::new();
    let mut failed_files = 0usize;
    let mut processed_files = 0usize;
    for file in current_files {
        let bytes = std::fs::read(&file)?;
        let hash = hash_bytes(&bytes);
        let file_str = file.to_string_lossy().to_string();

        if old_manifest.get(&file_str) == Some(&hash) {
            new_manifest.insert(file_str.clone(), hash);
            processed_files += 1;
            emit(IndexProgress::File {
                processed_files,
                total_files,
                path: file_str,
                status: IndexFileStatus::Unchanged,
            });
            continue;
        }

        let extracted = match extractors.extract(&file, &bytes) {
            Ok(extracted) => extracted,
            Err(err) => {
                warn!(path = %file.display(), error = %err, "skipping unreadable document during indexing");
                sqlite::delete_file(&conn, &record.id, &file_str)?;
                failed_files += 1;
                processed_files += 1;
                emit(IndexProgress::File {
                    processed_files,
                    total_files,
                    path: file_str,
                    status: IndexFileStatus::Failed,
                });
                continue;
            }
        };
        let chunks = chunk_document(&extracted);
        if chunks.is_empty() {
            sqlite::delete_file(&conn, &record.id, &file_str)?;
            new_manifest.insert(file_str.clone(), hash);
            processed_files += 1;
            emit(IndexProgress::File {
                processed_files,
                total_files,
                path: file_str,
                status: IndexFileStatus::Skipped,
            });
            continue;
        }

        let texts: Vec<String> = chunks
            .iter()
            .map(|chunk| chunk.enriched_text.clone())
            .collect();
        let embeddings = match embedder.embed_batch(&texts, EmbedMode::Document).await {
            Ok(embeddings) => embeddings,
            Err(err) => {
                warn!(path = %file.display(), error = %err, "skipping document after embedding failure");
                sqlite::delete_file(&conn, &record.id, &file_str)?;
                failed_files += 1;
                processed_files += 1;
                emit(IndexProgress::File {
                    processed_files,
                    total_files,
                    path: file_str,
                    status: IndexFileStatus::Failed,
                });
                continue;
            }
        };
        sqlite::replace_file_chunks(
            &mut conn,
            &record.id,
            &canonical,
            &record.display_name,
            &file_str,
            &chunks,
            &embeddings,
        )?;
        new_manifest.insert(file_str.clone(), hash);
        processed_files += 1;
        emit(IndexProgress::File {
            processed_files,
            total_files,
            path: file_str,
            status: IndexFileStatus::Indexed,
        });
    }

    emit(IndexProgress::Finalizing { total_files });

    let rows = sqlite::list_chunks_for_folder(&conn, &record.id)?;
    exact::rebuild_index(&storage.tantivy_dir, &rows)?;
    vector::rebuild_index(&storage.hnsw_dir, &rows)?;
    save_manifest(&storage.manifest_path, &new_manifest)?;
    // MIGRATION: document folders require reindex after upgrading from the edge-backed backend.
    info!(folder_id = %record.id, chunk_count = rows.len(), failed_files, "indexed document folder");

    let mut updated = record.clone();
    updated.last_indexed_at = Some(now_unix());
    let mut registry = load_folder_registry();
    if let Some(existing) = registry
        .folders
        .iter_mut()
        .find(|folder| folder.id == updated.id)
    {
        *existing = updated.clone();
    }
    save_folder_registry(&registry)?;
    Ok(updated)
}

pub async fn search_documents(
    query: &str,
    folder_id: Option<&str>,
    limit: usize,
    config: AppConfig,
) -> Result<Vec<SearchDocumentItem>> {
    let folders = list_folders();
    let selected: Vec<FolderRecord> = match folder_id {
        Some(id) => {
            let matching: Vec<FolderRecord> = folders
                .into_iter()
                .filter(|folder| folder.id == id)
                .collect();
            if matching.is_empty() {
                return Err(anyhow!("folder '{}' not found", id));
            }
            matching
        }
        None => folders
            .into_iter()
            .filter(|folder| folder.last_indexed_at.is_some())
            .collect(),
    };

    if selected.is_empty() {
        return Ok(Vec::new());
    }

    let embedder = build_embedder(&config.embedder)?;
    let query_embedding = embedder.embed(query, EmbedMode::Query).await?;
    let mut output = Vec::new();

    for folder in selected {
        let storage = document_storage_paths(Path::new(&folder.path));
        if !storage.sqlite_path.exists() {
            continue;
        }
        if !storage.tantivy_dir.join("meta.json").exists()
            || !storage.hnsw_dir.join("mapping.json").exists()
        {
            return Err(anyhow!(
                "folder {} index is corrupted or incomplete; run `compas index` to reindex",
                folder.id
            ));
        }

        let conn = sqlite::open_database(&storage.sqlite_path)?;
        let exact_hits = exact::exact_search(&storage.tantivy_dir, query, limit)?;
        let semantic_hits = vector::semantic_search(&storage.hnsw_dir, &query_embedding, limit)?;
        let chunk_ids: Vec<String> = exact_hits
            .iter()
            .map(|(chunk_id, _)| chunk_id.clone())
            .chain(semantic_hits.iter().map(|(chunk_id, _)| chunk_id.clone()))
            .collect();
        let metadata = sqlite::hydrate_chunks(&conn, &chunk_ids)?;
        let ranked = merge_hybrid(exact_hits, semantic_hits, query, &metadata);

        debug!(folder_id = %folder.id, merged_hits = ranked.len(), "searched document folder");
        for ranked_hit in ranked.into_iter().take(limit) {
            if let Some(row) = metadata.get(&ranked_hit.chunk_id) {
                output.push(to_search_item(&folder, row, ranked_hit.score));
            }
        }
    }

    output.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.absolute_path.cmp(&b.absolute_path))
    });
    output.truncate(limit);
    Ok(output)
}

pub fn open_document(path: &Path) -> Result<()> {
    let status = Command::new("open").arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("failed to open '{}'", path.display()))
    }
}

pub fn reveal_in_finder(path: &Path) -> Result<()> {
    let status = Command::new("open").arg("-R").arg(path).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("failed to reveal '{}' in Finder", path.display()))
    }
}

fn to_search_item(folder: &FolderRecord, row: &ChunkRow, score: f32) -> SearchDocumentItem {
    let section = if row.heading_path.is_empty() {
        "(root)".to_string()
    } else {
        row.heading_path.join(" > ")
    };
    let page = match (row.page_start, row.page_end) {
        (Some(start), Some(end)) if start != end => format!("{}-{}", start, end),
        (Some(start), _) => start.to_string(),
        _ => "n/a".to_string(),
    };

    SearchDocumentItem {
        folder_id: folder.id.clone(),
        folder_name: folder.display_name.clone(),
        file_path: row.relative_path.clone(),
        absolute_path: row.absolute_path.clone(),
        title: row.title.clone(),
        section,
        page,
        preview: row.preview.clone(),
        score,
    }
}

fn collect_document_files(
    repo_path: &Path,
    include: &[String],
    exclude: &[String],
    file_types: &[String],
) -> Result<Vec<PathBuf>> {
    let normalized_file_types = normalize_or_default_folder_file_types(file_types);
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(repo_path) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_path_buf();
        let relative = path.strip_prefix(repo_path).unwrap_or(path.as_path());
        if !should_include(relative, include, exclude) {
            continue;
        }
        let Some(extension) = lowercase_extension(&path) else {
            continue;
        };
        if !normalized_file_types
            .iter()
            .any(|file_type| file_type == &extension)
        {
            continue;
        }
        files.push(path);
    }
    Ok(files)
}

fn should_include(path: &Path, include: &[String], exclude: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    for ex in exclude {
        if let Ok(glob) = globset::Glob::new(ex) {
            if glob.compile_matcher().is_match(&*path_str) {
                return false;
            }
        }
    }
    if include.is_empty() {
        return true;
    }
    for inc in include {
        if let Ok(glob) = globset::Glob::new(inc) {
            if glob.compile_matcher().is_match(&*path_str) {
                return true;
            }
        }
    }
    false
}

fn hash_bytes(bytes: &[u8]) -> String {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(bytes);
    format!("{:x}", hasher.finish())
}

fn load_manifest(path: &Path) -> HashMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_manifest(path: &Path, manifest: &HashMap<String, String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(manifest)?)?;
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("compas-docs-core-{name}-{nanos}"))
    }

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn lock_tests() -> MutexGuard<'static, ()> {
        match test_lock().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &Path) -> Self {
            let original = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.original {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    #[tokio::test]
    async fn index_folder_and_search_documents_round_trip() {
        let _guard = lock_tests();
        let library_dir = unique_temp_path("search-library");
        let folder_dir = unique_temp_path("search-folder");
        std::fs::create_dir_all(folder_dir.join("docs")).unwrap();
        std::fs::write(
            folder_dir.join("docs").join("policy.md"),
            "# Insurance Policy\n\nRenewal date is January 15.\n",
        )
        .unwrap();
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);

        let mut config = default_document_config(&folder_dir);
        config.embedder.provider = "test".into();
        config.embedder.model = "test".into();

        let record = index_folder(&folder_dir, config.clone()).await.unwrap();
        let results = search_documents("renewal date", Some(&record.id), 5, config)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].folder_id, record.id);
        assert_eq!(results[0].folder_name, record.display_name);
        assert_eq!(results[0].file_path, "docs/policy.md");
        assert!(results[0].absolute_path.ends_with("docs/policy.md"));
        assert_eq!(results[0].title, "Insurance Policy");
        assert_eq!(results[0].page, "n/a");
        assert!(results[0].score > 0.0);
    }

    #[tokio::test]
    async fn hybrid_search_returns_search_document_item_fields() {
        let _guard = lock_tests();
        let library_dir = unique_temp_path("search-library-fields");
        let folder_dir = unique_temp_path("search-folder-fields");
        std::fs::create_dir_all(folder_dir.join("docs")).unwrap();
        std::fs::write(
            folder_dir.join("docs").join("policy.md"),
            "# Insurance Policy\n\nRenewal date is January 15.\n",
        )
        .unwrap();
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);

        let mut config = default_document_config(&folder_dir);
        config.embedder.provider = "test".into();
        config.embedder.model = "test".into();
        let record = index_folder(&folder_dir, config.clone()).await.unwrap();
        let result = search_documents("policy", Some(&record.id), 1, config)
            .await
            .unwrap()
            .remove(0);

        assert!(!result.folder_id.is_empty());
        assert!(!result.folder_name.is_empty());
        assert!(!result.file_path.is_empty());
        assert!(!result.absolute_path.is_empty());
        assert!(!result.title.is_empty());
        assert!(!result.section.is_empty());
        assert!(!result.page.is_empty());
        assert!(!result.preview.is_empty());
        assert!(result.score >= 0.0);
    }

    #[tokio::test]
    async fn search_documents_returns_error_for_unknown_folder_id() {
        let _guard = lock_tests();
        let library_dir = unique_temp_path("unknown-folder");
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);
        let mut config = default_document_config(Path::new("."));
        config.embedder.provider = "test".into();
        config.embedder.model = "test".into();

        let error = search_documents("renewal", Some("missing"), 5, config)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("missing"));
    }

    #[test]
    fn remove_folder_deletes_sqlite_tantivy_and_hnsw_assets() {
        let _guard = lock_tests();
        let library_dir = unique_temp_path("remove-assets");
        let folder_dir = unique_temp_path("remove-assets-folder");
        std::fs::create_dir_all(&folder_dir).unwrap();
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);
        let record = add_folder(&folder_dir).unwrap();
        let storage = document_storage_paths(&folder_dir);
        std::fs::create_dir_all(&storage.root).unwrap();
        std::fs::write(&storage.sqlite_path, b"db").unwrap();
        std::fs::create_dir_all(&storage.tantivy_dir).unwrap();
        std::fs::create_dir_all(&storage.hnsw_dir).unwrap();

        assert!(remove_folder(&record.id).unwrap());
        assert!(!storage.root.exists());
    }

    #[tokio::test]
    async fn search_documents_returns_reindex_error_when_tantivy_missing() {
        let _guard = lock_tests();
        let library_dir = unique_temp_path("tantivy-missing");
        let folder_dir = unique_temp_path("tantivy-folder");
        std::fs::create_dir_all(folder_dir.join("docs")).unwrap();
        std::fs::write(
            folder_dir.join("docs").join("policy.md"),
            "# Insurance Policy\n\nRenewal date is January 15.\n",
        )
        .unwrap();
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);

        let mut config = default_document_config(&folder_dir);
        config.embedder.provider = "test".into();
        config.embedder.model = "test".into();
        let record = index_folder(&folder_dir, config.clone()).await.unwrap();
        let storage = document_storage_paths(&folder_dir);
        std::fs::remove_dir_all(&storage.tantivy_dir).unwrap();

        let error = search_documents("renewal", Some(&record.id), 5, config)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("reindex"));
        assert!(error.to_string().contains(&record.id));
    }

    #[test]
    fn add_folder_defaults_and_normalizes_file_types() {
        let _guard = lock_tests();
        let library_dir = unique_temp_path("file-types-defaults");
        let folder_dir = unique_temp_path("file-types-folder");
        std::fs::create_dir_all(&folder_dir).unwrap();
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);

        let record = add_folder(&folder_dir).unwrap();
        assert_eq!(record.file_types, default_folder_file_types());

        let updated = add_folder_with_file_types(
            &folder_dir,
            Some(vec!["PDF".into(), "txt".into(), "zip".into(), "txt".into()]),
        )
        .unwrap();
        assert_eq!(
            updated.file_types,
            vec!["txt".to_string(), "pdf".to_string()]
        );

        let listed = list_folders();
        assert_eq!(
            listed[0].file_types,
            vec!["txt".to_string(), "pdf".to_string()]
        );
    }

    #[tokio::test]
    async fn index_folder_filters_by_selected_file_types() {
        let _guard = lock_tests();
        let library_dir = unique_temp_path("index-file-types-library");
        let folder_dir = unique_temp_path("index-file-types-folder");
        std::fs::create_dir_all(folder_dir.join("docs")).unwrap();
        std::fs::write(
            folder_dir.join("docs").join("policy.md"),
            "# Insurance Policy\n\nRenewal date is January 15.\n",
        )
        .unwrap();
        std::fs::write(
            folder_dir.join("docs").join("notes.txt"),
            "Renewal notes for the policy holder.\n",
        )
        .unwrap();
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);

        let mut config = default_document_config(&folder_dir);
        config.embedder.provider = "test".into();
        config.embedder.model = "test".into();

        let record =
            index_folder_with_file_types(&folder_dir, config.clone(), Some(vec!["txt".into()]))
                .await
                .unwrap();
        assert_eq!(record.file_types, vec!["txt".to_string()]);

        let results = search_documents("renewal notes", Some(&record.id), 5, config)
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "docs/notes.txt");
    }
}
