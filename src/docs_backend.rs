use crate::config::AppConfig;
use crate::docs::indexing::DocumentIndexAdapter;
use crate::docs::models::DocumentSearchResult;
use crate::embedder::build_embedder;
use crate::indexing::Indexer;
use crate::search::search_chunks;
use crate::store::{edge::EdgeStore, Store};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct DocumentStoragePaths {
    pub store_path: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderRecord {
    pub id: String,
    pub path: String,
    pub display_name: String,
    pub storage_path: String,
    pub last_indexed_at: Option<u64>,
    pub watch_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FolderRegistry {
    pub folders: Vec<FolderRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchDocumentItem {
    pub file_path: String,
    pub title: String,
    pub section: String,
    pub page: String,
    pub preview: String,
    pub score: f32,
}

pub fn default_document_config(path: &Path) -> AppConfig {
    let storage = document_storage_paths(path);

    AppConfig {
        repo: crate::config::RepoConfig {
            path: path.to_string_lossy().to_string(),
            include: vec!["**/*.md".into(), "**/*.txt".into(), "**/*.pdf".into()],
            exclude: vec![],
        },
        embedder: crate::config::EmbedderConfig {
            provider: "fastembed".into(),
            model: "nomic-ai/nomic-embed-text-v1.5".into(),
            query_prefix: None,
            doc_prefix: None,
        },
        store: crate::config::StoreConfig {
            provider: "edge".into(),
            path: storage.store_path.to_string_lossy().to_string(),
            vector_name: "default".into(),
        },
        server: crate::config::ServerConfig {
            host: "127.0.0.1".into(),
            port: "3001".into(),
        },
        index: crate::config::IndexConfig {
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
}

pub fn document_storage_paths(source_path: &Path) -> DocumentStoragePaths {
    let canonical_source =
        std::fs::canonicalize(source_path).unwrap_or_else(|_| source_path.to_path_buf());
    let source_id = stable_folder_id(&canonical_source);
    let root = document_library_root().join("indices").join(source_id);

    DocumentStoragePaths {
        store_path: root.join("edge-shard"),
        manifest_path: root.join("manifest.json"),
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

pub fn resolved_store_path(config: &AppConfig, repo_path: &Path) -> PathBuf {
    if config.index.kind == "document" {
        return document_storage_paths(repo_path).store_path;
    }

    let store_path = Path::new(&config.store.path);
    if store_path.is_absolute() {
        store_path.to_path_buf()
    } else {
        repo_path.join(store_path)
    }
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
    let canonical = std::fs::canonicalize(path)?;
    if !canonical.is_dir() {
        return Err(anyhow!("'{}' is not a directory", canonical.display()));
    }

    let mut registry = load_folder_registry();
    let id = stable_folder_id(&canonical);
    let storage = document_storage_paths(&canonical);

    let record = FolderRecord {
        id: id.clone(),
        path: canonical.to_string_lossy().to_string(),
        display_name: canonical
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("folder")
            .to_string(),
        storage_path: storage.store_path.to_string_lossy().to_string(),
        last_indexed_at: None,
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
        let storage_path = PathBuf::from(folder.storage_path);
        if let Some(parent) = storage_path.parent() {
            if parent.exists() {
                std::fs::remove_dir_all(parent)?;
            }
        }
    }
    Ok(true)
}

pub async fn index_folder(path: &Path, mut config: AppConfig) -> Result<FolderRecord> {
    let canonical = std::fs::canonicalize(path)?;
    config.repo.path = canonical.to_string_lossy().to_string();
    normalize_document_storage(&mut config);

    let embedder = build_embedder(&config.embedder)?;
    let store = EdgeStore::new(
        resolved_store_path(&config, &canonical),
        &config.store.vector_name,
    );
    store.init(embedder.dimensions()).await?;

    let storage = document_storage_paths(&canonical);
    let adapter = DocumentIndexAdapter::new();
    let indexer = Indexer::new(
        &canonical,
        &config.repo.include,
        &config.repo.exclude,
        &store,
        embedder.as_ref(),
    )
    .with_manifest_path(&storage.manifest_path);
    indexer.index_repo(&adapter).await?;

    let mut record = add_folder(&canonical)?;
    record.last_indexed_at = Some(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );

    let mut registry = load_folder_registry();
    if let Some(existing) = registry
        .folders
        .iter_mut()
        .find(|folder| folder.id == record.id)
    {
        *existing = record.clone();
    }
    save_folder_registry(&registry)?;
    Ok(record)
}

pub async fn search_documents(
    query: &str,
    folder_id: Option<&str>,
    limit: usize,
    config: AppConfig,
) -> Result<Vec<SearchDocumentItem>> {
    let embedder = build_embedder(&config.embedder)?;
    let folders = list_folders();
    let selected: Vec<FolderRecord> = match folder_id {
        Some(id) => folders
            .into_iter()
            .filter(|folder| folder.id == id)
            .collect(),
        None => folders,
    };

    let mut results = Vec::new();
    for folder in selected {
        let repo_path = PathBuf::from(&folder.path);
        let store = EdgeStore::new(
            PathBuf::from(&folder.storage_path),
            &config.store.vector_name,
        );
        let hits = search_chunks(embedder.as_ref(), &store, query, limit, &HashMap::new()).await?;
        for hit in hits {
            let result = DocumentSearchResult::try_from(hit)?;
            let rel_file = Path::new(&result.chunk.file_path)
                .strip_prefix(&repo_path)
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|_| result.chunk.file_path.clone());
            let section = if result.chunk.heading_path.is_empty() {
                "(root)".to_string()
            } else {
                result.chunk.heading_path.join(" > ")
            };
            let page = match (result.chunk.page_start, result.chunk.page_end) {
                (Some(start), Some(end)) if start != end => format!("{}-{}", start, end),
                (Some(start), _) => start.to_string(),
                _ => "n/a".to_string(),
            };

            results.push(SearchDocumentItem {
                file_path: rel_file,
                title: result.chunk.title,
                section,
                page,
                preview: result.chunk.preview,
                score: result.score,
            });
        }
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);
    Ok(results)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("compas-docs-backend-{name}-{nanos}"))
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

    #[test]
    fn add_list_and_remove_folder_round_trip_registry() {
        let _guard = lock_tests();

        let library_dir = unique_temp_path("registry");
        let folder_dir = unique_temp_path("folder");
        std::fs::create_dir_all(&folder_dir).unwrap();
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);

        let record = add_folder(&folder_dir).unwrap();
        let listed = list_folders();
        assert_eq!(listed, vec![record.clone()]);
        assert!(remove_folder(&record.id).unwrap());
        assert!(list_folders().is_empty());

        std::fs::remove_dir_all(folder_dir).unwrap();
        if library_dir.exists() {
            std::fs::remove_dir_all(library_dir).unwrap();
        }
    }

    #[test]
    fn reveal_and_open_helpers_build_expected_commands() {
        let path = Path::new("/tmp/sample.pdf");
        assert!(open_document_command(path).ends_with(&["/tmp/sample.pdf".to_string()]));
        assert_eq!(
            reveal_in_finder_command(path),
            vec![
                "open".to_string(),
                "-R".to_string(),
                "/tmp/sample.pdf".to_string()
            ]
        );
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
        assert_eq!(results[0].file_path, "docs/policy.md");
        assert_eq!(results[0].title, "Insurance Policy");
        assert_eq!(results[0].page, "n/a");

        std::fs::remove_dir_all(folder_dir).unwrap();
        if library_dir.exists() {
            std::fs::remove_dir_all(library_dir).unwrap();
        }
    }

    fn open_document_command(path: &Path) -> Vec<String> {
        vec!["open".to_string(), path.to_string_lossy().to_string()]
    }

    fn reveal_in_finder_command(path: &Path) -> Vec<String> {
        vec![
            "open".to_string(),
            "-R".to_string(),
            path.to_string_lossy().to_string(),
        ]
    }
}
