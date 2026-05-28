use crate::code::{ranking::rerank_code_results, CodeRuntime};
use crate::config::AppConfig;
use crate::embedder::Embedder;
#[cfg(test)]
use crate::models::{IndexedChunk, SearchHit};
use crate::search::search_chunks;
use crate::store::Store;
use axum::{extract::Query, middleware, response::Json, routing::get, Router};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

pub struct RepoState {
    pub config: AppConfig,
    pub store: Arc<dyn Store>,
    pub code: Option<CodeRuntime>,
    pub embedder: Arc<dyn Embedder>,
}

pub struct AppState {
    pub repos: HashMap<String, RepoState>,
    pub default_repo: Option<String>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/search", get(search_handler))
        .route("/graph", get(graph_handler))
        .route("/repos", get(list_repos))
        .layer(middleware::from_fn(crate::middleware::request_logger))
        .with_state(state)
}

fn resolve_repo<'a>(
    state: &'a AppState,
    params: &HashMap<String, String>,
) -> Result<&'a RepoState, String> {
    let repo_name = params
        .get("repo")
        .cloned()
        .or_else(|| state.default_repo.clone())
        .ok_or_else(|| {
            let available: Vec<String> = state.repos.keys().cloned().collect();
            format!(
                "missing 'repo' parameter. Available repos: {}",
                available.join(", ")
            )
        })?;

    state
        .repos
        .get(&repo_name)
        .ok_or_else(|| format!("repo '{}' not found", repo_name))
}

async fn health(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let mut repo_statuses = serde_json::Map::new();
    for (name, repo) in &state.repos {
        repo_statuses.insert(
            name.clone(),
            json!({
                "status": "registered",
                "path": repo.config.repo.path,
                "store_provider": repo.config.store.provider,
                "vector_name": repo.config.store.vector_name,
            }),
        );
    }

    Json(json!({
        "status": "ok",
        "repos": repo_statuses,
        "default_repo": state.default_repo,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::graph::Graph;
    use crate::config::{
        AppConfig, EmbedderConfig, IndexConfig, RepoConfig, ServerConfig, StoreConfig,
    };
    use crate::embedder::{EmbedMode, Embedder};
    use crate::models::{IndexedChunk, SearchHit};
    use anyhow::{anyhow, Result};

    struct PanicStore;

    #[async_trait::async_trait]
    impl Store for PanicStore {
        async fn init(&self, _vector_size: usize) -> Result<()> {
            Err(anyhow!("store.init should not be called by /health"))
        }

        async fn upsert_indexed(
            &self,
            _chunks: &[IndexedChunk],
            _embeddings: &[Vec<f32>],
        ) -> Result<()> {
            unreachable!("upsert is not used by /health tests")
        }

        async fn search_indexed(
            &self,
            _embedding: &[f32],
            _limit: usize,
            _filters: &HashMap<String, String>,
        ) -> Result<Vec<SearchHit>> {
            unreachable!("search is not used by /health tests")
        }

        async fn delete_by_file(&self, _file_path: &str) -> Result<()> {
            unreachable!("delete_by_file is not used by /health tests")
        }
    }

    struct FakeEmbedder;

    #[async_trait::async_trait]
    impl Embedder for FakeEmbedder {
        async fn embed(&self, _text: &str, _mode: EmbedMode) -> Result<Vec<f32>> {
            Ok(vec![1.0, 0.0, 0.0, 0.0])
        }

        async fn embed_batch(&self, texts: &[String], _mode: EmbedMode) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![1.0, 0.0, 0.0, 0.0]).collect())
        }

        fn dimensions(&self) -> usize {
            4
        }
    }

    fn sample_config() -> AppConfig {
        AppConfig {
            repo: RepoConfig {
                path: "/tmp/bookswipe".to_string(),
                include: vec!["lib/**/*.dart".to_string()],
                exclude: vec![],
            },
            embedder: EmbedderConfig {
                provider: "fastembed".to_string(),
                model: "nomic-embed-text-v1.5-q".to_string(),
                query_prefix: None,
                doc_prefix: None,
            },
            store: StoreConfig {
                provider: "edge".to_string(),
                path: ".compas/edge-shard".to_string(),
                vector_name: "default".to_string(),
            },
            server: ServerConfig {
                port: "3001".to_string(),
                host: "127.0.0.1".to_string(),
            },
            index: IndexConfig {
                kind: "code".to_string(),
                chunk_by: "function".to_string(),
                watch: true,
            },
        }
    }

    #[tokio::test]
    async fn health_reports_registered_repos_without_touching_store() {
        let state = Arc::new(AppState {
            repos: HashMap::from([(
                "bookswipe".to_string(),
                RepoState {
                    config: sample_config(),
                    store: Arc::new(PanicStore),
                    code: Some(CodeRuntime {
                        graph: Arc::new(Graph::new()),
                    }),
                    embedder: Arc::new(FakeEmbedder),
                },
            )]),
            default_repo: Some("bookswipe".to_string()),
        });

        let Json(value) = health(axum::extract::State(state)).await;

        assert_eq!(value["status"], "ok");
        assert_eq!(value["default_repo"], "bookswipe");
        assert_eq!(value["repos"]["bookswipe"]["status"], "registered");
        assert_eq!(value["repos"]["bookswipe"]["path"], "/tmp/bookswipe");
        assert_eq!(value["repos"]["bookswipe"]["store_provider"], "edge");
        assert_eq!(value["repos"]["bookswipe"]["vector_name"], "default");
    }
}

#[cfg(test)]
struct StaticSearchStore {
    hits: Vec<SearchHit>,
}

#[cfg(test)]
#[async_trait::async_trait]
impl Store for StaticSearchStore {
    async fn init(&self, _vector_size: usize) -> anyhow::Result<()> {
        Ok(())
    }

    async fn upsert_indexed(
        &self,
        _chunks: &[IndexedChunk],
        _embeddings: &[Vec<f32>],
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn search_indexed(
        &self,
        _embedding: &[f32],
        _limit: usize,
        _filters: &HashMap<String, String>,
    ) -> anyhow::Result<Vec<SearchHit>> {
        Ok(self.hits.clone())
    }

    async fn delete_by_file(&self, _file_path: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

async fn list_repos(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let repos: Vec<String> = state.repos.keys().cloned().collect();
    Json(json!({
        "repos": repos,
        "default_repo": state.default_repo,
    }))
}

async fn search_handler(
    Query(params): Query<HashMap<String, String>>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let repo = match resolve_repo(&state, &params) {
        Ok(r) => r,
        Err(e) => return Json(json!({"error": e})),
    };

    let query = params.get("q").cloned().unwrap_or_default();
    if query.is_empty() {
        return Json(json!({"error": "missing query"}));
    }

    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(15);

    let mut filters = HashMap::new();
    if let Some(lang) = params.get("language") {
        filters.insert("language".into(), lang.clone());
    }

    let Some(code) = repo.code.as_ref() else {
        return Json(json!({"error": "code search unavailable for this repo"}));
    };

    match search_chunks(
        repo.embedder.as_ref(),
        repo.store.as_ref(),
        &query,
        limit * 3,
        &filters,
    )
    .await
    {
        Ok(raw_results) => {
            let results = rerank_code_results(code.graph.as_ref(), raw_results, &query, limit);

            Json(json!({
                "query": query,
                "results": results,
            }))
        }
        Err(e) => Json(json!({"error": format!("search failed: {}", e)})),
    }
}

async fn graph_handler(
    Query(params): Query<HashMap<String, String>>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let repo = match resolve_repo(&state, &params) {
        Ok(r) => r,
        Err(e) => return Json(json!({"error": e})),
    };

    let symbol = params.get("symbol").cloned().unwrap_or_default();
    let file = params.get("file").cloned().unwrap_or_default();
    if symbol.is_empty() {
        return Json(json!({"error": "missing symbol"}));
    }
    let Some(code) = repo.code.as_ref() else {
        return Json(json!({"error": "graph unavailable for this repo"}));
    };
    let exact = code.graph.get(&symbol, &file);
    let matches = if let Some(node) = exact {
        vec![node]
    } else {
        code.graph.search(&symbol)
    };
    if matches.is_empty() {
        Json(json!({"error": "symbol not found"}))
    } else {
        Json(json!(matches))
    }
}

#[cfg(test)]
mod search_shape_tests {
    use super::*;
    use crate::code::graph::Graph;
    use crate::code::models::{CodeChunk, CodeSearchResult};
    use crate::embedder::{EmbedMode, Embedder};
    use crate::models::SearchHit;
    use serde_json::Value;

    fn sample_config() -> AppConfig {
        AppConfig {
            repo: crate::config::RepoConfig {
                path: "/tmp/bookswipe".to_string(),
                include: vec!["lib/**/*.dart".to_string()],
                exclude: vec![],
            },
            embedder: crate::config::EmbedderConfig {
                provider: "fastembed".to_string(),
                model: "nomic-embed-text-v1.5-q".to_string(),
                query_prefix: None,
                doc_prefix: None,
            },
            store: crate::config::StoreConfig {
                provider: "edge".to_string(),
                path: ".compas/edge-shard".to_string(),
                vector_name: "default".to_string(),
            },
            server: crate::config::ServerConfig {
                port: "3001".to_string(),
                host: "127.0.0.1".to_string(),
            },
            index: crate::config::IndexConfig {
                kind: "code".to_string(),
                chunk_by: "function".to_string(),
                watch: true,
            },
        }
    }

    struct FakeSearchEmbedder;

    #[async_trait::async_trait]
    impl Embedder for FakeSearchEmbedder {
        async fn embed(&self, _text: &str, _mode: EmbedMode) -> anyhow::Result<Vec<f32>> {
            Ok(vec![1.0, 0.0, 0.0, 0.0])
        }

        async fn embed_batch(
            &self,
            texts: &[String],
            _mode: EmbedMode,
        ) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![1.0, 0.0, 0.0, 0.0]).collect())
        }

        fn dimensions(&self) -> usize {
            4
        }
    }

    #[tokio::test]
    async fn search_handler_returns_legacy_code_chunk_shape_from_generic_hits() {
        let hit: SearchHit = CodeSearchResult {
            chunk: CodeChunk {
                id: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".to_string(),
                content: "auth body".to_string(),
                language: "dart".to_string(),
                file_path: "/tmp/lib/auth.dart".to_string(),
                symbol: "AuthService.login".to_string(),
                line_start: 10,
                line_end: 18,
                kind: "method".to_string(),
                meta: HashMap::new(),
            },
            score: 0.75,
        }
        .into();

        let state = Arc::new(AppState {
            repos: HashMap::from([(
                "bookswipe".to_string(),
                RepoState {
                    config: sample_config(),
                    store: Arc::new(StaticSearchStore { hits: vec![hit] }),
                    code: Some(CodeRuntime {
                        graph: Arc::new(Graph::new()),
                    }),
                    embedder: Arc::new(FakeSearchEmbedder),
                },
            )]),
            default_repo: Some("bookswipe".to_string()),
        });

        let Json(value) = search_handler(
            Query(HashMap::from([
                ("repo".to_string(), "bookswipe".to_string()),
                ("q".to_string(), "authentication".to_string()),
            ])),
            axum::extract::State(state),
        )
        .await;

        let first = &value["results"][0]["chunk"];
        assert_eq!(
            first["symbol"],
            Value::String("AuthService.login".to_string())
        );
        assert_eq!(first["language"], Value::String("dart".to_string()));
        assert_eq!(first["line_start"], Value::from(10));
        assert_eq!(first["line_end"], Value::from(18));
        assert_eq!(first["type"], Value::String("method".to_string()));
    }

    #[tokio::test]
    async fn search_handler_returns_code_unavailable_for_document_repo() {
        let state = Arc::new(AppState {
            repos: HashMap::from([(
                "docs".to_string(),
                RepoState {
                    config: sample_config(),
                    store: Arc::new(StaticSearchStore { hits: vec![] }),
                    code: None,
                    embedder: Arc::new(FakeSearchEmbedder),
                },
            )]),
            default_repo: Some("docs".to_string()),
        });

        let Json(value) = search_handler(
            Query(HashMap::from([
                ("repo".to_string(), "docs".to_string()),
                ("q".to_string(), "authentication".to_string()),
            ])),
            axum::extract::State(state),
        )
        .await;

        assert_eq!(
            value,
            json!({"error": "code search unavailable for this repo"})
        );
    }
}
