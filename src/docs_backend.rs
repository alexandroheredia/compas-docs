use crate::config::AppConfig;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub use compas_docs_core::models::{FolderRecord, FolderRegistry, SearchDocumentItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentStoragePaths {
    pub root: PathBuf,
    pub store_path: PathBuf,
    pub manifest_path: PathBuf,
    pub sqlite_path: PathBuf,
    pub tantivy_dir: PathBuf,
    pub hnsw_dir: PathBuf,
}

pub fn default_document_config(path: &Path) -> AppConfig {
    from_core_config(compas_docs_core::backend::default_document_config(path))
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
    let storage = compas_docs_core::backend::document_storage_paths(source_path);
    DocumentStoragePaths {
        root: storage.root,
        store_path: storage.store_path,
        manifest_path: storage.manifest_path,
        sqlite_path: storage.sqlite_path,
        tantivy_dir: storage.tantivy_dir,
        hnsw_dir: storage.hnsw_dir,
    }
}

pub fn document_library_root() -> PathBuf {
    compas_docs_core::backend::document_library_root()
}

pub fn stable_folder_id(path: &Path) -> String {
    compas_docs_core::backend::stable_folder_id(path)
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
    compas_docs_core::backend::load_folder_registry()
}

pub fn save_folder_registry(registry: &FolderRegistry) -> Result<()> {
    compas_docs_core::backend::save_folder_registry(registry)
}

pub fn folder_registry_path() -> PathBuf {
    compas_docs_core::backend::folder_registry_path()
}

pub fn add_folder(path: &Path) -> Result<FolderRecord> {
    compas_docs_core::backend::add_folder(path)
}

pub fn list_folders() -> Vec<FolderRecord> {
    compas_docs_core::backend::list_folders()
}

pub fn remove_folder(id: &str) -> Result<bool> {
    compas_docs_core::backend::remove_folder(id)
}

pub async fn index_folder(path: &Path, config: AppConfig) -> Result<FolderRecord> {
    compas_docs_core::backend::index_folder(path, to_core_config(config)).await
}

pub async fn search_documents(
    query: &str,
    folder_id: Option<&str>,
    limit: usize,
    config: AppConfig,
) -> Result<Vec<SearchDocumentItem>> {
    compas_docs_core::backend::search_documents(query, folder_id, limit, to_core_config(config))
        .await
}

pub fn open_document(path: &Path) -> Result<()> {
    compas_docs_core::backend::open_document(path)
}

pub fn reveal_in_finder(path: &Path) -> Result<()> {
    compas_docs_core::backend::reveal_in_finder(path)
}

fn to_core_config(config: AppConfig) -> compas_docs_core::config::AppConfig {
    compas_docs_core::config::AppConfig {
        repo: compas_docs_core::config::RepoConfig {
            path: config.repo.path,
            include: config.repo.include,
            exclude: config.repo.exclude,
        },
        embedder: compas_docs_core::config::EmbedderConfig {
            provider: config.embedder.provider,
            model: config.embedder.model,
            query_prefix: config.embedder.query_prefix,
            doc_prefix: config.embedder.doc_prefix,
        },
        store: compas_docs_core::config::StoreConfig {
            provider: config.store.provider,
            path: config.store.path,
            vector_name: config.store.vector_name,
        },
        server: compas_docs_core::config::ServerConfig {
            port: config.server.port,
            host: config.server.host,
        },
        index: compas_docs_core::config::IndexConfig {
            kind: config.index.kind,
            chunk_by: config.index.chunk_by,
            watch: config.index.watch,
        },
    }
}

fn from_core_config(config: compas_docs_core::config::AppConfig) -> AppConfig {
    AppConfig {
        repo: crate::config::RepoConfig {
            path: config.repo.path,
            include: config.repo.include,
            exclude: config.repo.exclude,
        },
        embedder: crate::config::EmbedderConfig {
            provider: config.embedder.provider,
            model: config.embedder.model,
            query_prefix: config.embedder.query_prefix,
            doc_prefix: config.embedder.doc_prefix,
        },
        store: crate::config::StoreConfig {
            provider: config.store.provider,
            path: config.store.path,
            vector_name: config.store.vector_name,
        },
        server: crate::config::ServerConfig {
            port: config.server.port,
            host: config.server.host,
        },
        index: crate::config::IndexConfig {
            kind: config.index.kind,
            chunk_by: config.index.chunk_by,
            watch: config.index.watch,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

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
        assert_eq!(results[0].folder_id, record.id);
        assert_eq!(results[0].folder_name, record.display_name);
        assert_eq!(results[0].file_path, "docs/policy.md");
        assert!(results[0].absolute_path.ends_with("docs/policy.md"));
        assert_eq!(results[0].title, "Insurance Policy");
        assert_eq!(results[0].page, "n/a");
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
