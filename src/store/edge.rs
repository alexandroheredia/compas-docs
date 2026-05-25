use super::Store;
use crate::code::models::{LANGUAGE_KEY, LINE_END_KEY, LINE_START_KEY, SYMBOL_KEY};
use crate::models::{IndexedChunk, SearchHit};
use anyhow::{anyhow, Context, Result};
use fs4::FileExt;
use qdrant_edge::{
    Condition, Distance, EdgeConfig, EdgeOptimizersConfig, EdgeShard, EdgeVectorParams,
    FieldCondition, Filter, JsonPath, Match, MatchValue, NamedQuery, Payload, PointId,
    PointInsertOperations, PointOperations, PointStruct, PointStructPersisted, QueryEnum,
    QueryRequest, ScoredPoint, ScoringQuery, UpdateOperation, ValueVariants, VectorInternal,
    Vectors, WithPayloadInterface, WithVector,
};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct EdgeStore {
    shard_path: PathBuf,
    vector_name: String,
    operation_lock: Arc<Mutex<()>>,
}

impl EdgeStore {
    pub fn new(shard_path: impl Into<PathBuf>, vector_name: impl Into<String>) -> Self {
        Self {
            shard_path: shard_path.into(),
            vector_name: vector_name.into(),
            operation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn shard_path(&self) -> &Path {
        &self.shard_path
    }

    pub fn optimize(&self) -> Result<bool> {
        self.with_locked_shard(None, |shard| shard.optimize().map_err(edge_error))
    }

    fn lock_path(&self) -> PathBuf {
        let lock_name = self
            .shard_path
            .file_name()
            .map(|name| format!("{}.lock", name.to_string_lossy()))
            .unwrap_or_else(|| "edge-shard.lock".to_string());

        self.shard_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(lock_name)
    }

    fn with_file_lock<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let _guard = self
            .operation_lock
            .lock()
            .map_err(|_| anyhow!("edge shard mutex poisoned"))?;

        let lock_path = self.lock_path();
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create edge lock directory at {}",
                    parent.display()
                )
            })?;
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("failed to open edge lock file at {}", lock_path.display()))?;

        lock_file.lock_exclusive().with_context(|| {
            format!("failed to lock edge shard at {}", self.shard_path.display())
        })?;

        let result = f();
        let unlock_result = lock_file.unlock().with_context(|| {
            format!(
                "failed to unlock edge shard at {}",
                self.shard_path.display()
            )
        });

        match (result, unlock_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    fn with_locked_shard<T>(
        &self,
        vector_size: Option<usize>,
        f: impl FnOnce(&EdgeShard) -> Result<T>,
    ) -> Result<T> {
        self.with_file_lock(|| {
            let shard = match vector_size {
                Some(vector_size) => self.init_or_load_shard(vector_size)?,
                None => self.load_existing_shard()?,
            };
            f(&shard)
        })
    }

    fn init_or_load_shard(&self, vector_size: usize) -> Result<EdgeShard> {
        std::fs::create_dir_all(&self.shard_path).with_context(|| {
            format!(
                "failed to create edge shard directory at {}",
                self.shard_path.display()
            )
        })?;

        let expected_config = self.edge_config(vector_size);
        let has_existing_data = std::fs::read_dir(&self.shard_path)
            .with_context(|| {
                format!(
                    "failed to inspect edge shard directory at {}",
                    self.shard_path.display()
                )
            })?
            .next()
            .transpose()?
            .is_some();

        if has_existing_data {
            EdgeShard::load(&self.shard_path, Some(expected_config))
                .map_err(edge_error)
                .with_context(|| {
                    format!(
                        "existing edge shard at {} is incompatible with vector '{}' and dimension {}. Delete {} and reindex if the embedding model or vector name changed",
                        self.shard_path.display(),
                        self.vector_name,
                        vector_size,
                        self.shard_path.display()
                    )
                })
        } else {
            EdgeShard::new(&self.shard_path, expected_config)
                .map_err(edge_error)
                .with_context(|| {
                    format!(
                        "failed to initialize edge shard at {}",
                        self.shard_path.display()
                    )
                })
        }
    }

    fn load_existing_shard(&self) -> Result<EdgeShard> {
        EdgeShard::load(&self.shard_path, None)
            .map_err(edge_error)
            .with_context(|| format!("failed to load edge shard at {}", self.shard_path.display()))
    }

    fn edge_config(&self, vector_size: usize) -> EdgeConfig {
        let mut config = EdgeConfig::default();
        config.vectors.insert(
            self.vector_name.clone(),
            EdgeVectorParams {
                size: vector_size,
                distance: Distance::Cosine,
                on_disk: Some(true),
                multivector_config: None,
                datatype: None,
                quantization_config: None,
                hnsw_config: None,
            },
        );
        config.optimizers = EdgeOptimizersConfig {
            deleted_threshold: Some(0.2),
            vacuum_min_vector_number: Some(100),
            default_segment_number: Some(2),
            ..Default::default()
        };
        config
    }
}

#[async_trait::async_trait]
impl Store for EdgeStore {
    async fn init(&self, vector_size: usize) -> Result<()> {
        self.with_locked_shard(Some(vector_size), |_shard| Ok(()))
    }

    async fn upsert_indexed(&self, chunks: &[IndexedChunk], embeddings: &[Vec<f32>]) -> Result<()> {
        if chunks.len() != embeddings.len() {
            return Err(anyhow!(
                "chunks/embedding count mismatch: {} chunks vs {} embeddings",
                chunks.len(),
                embeddings.len()
            ));
        }

        let points: Vec<PointStructPersisted> = chunks
            .iter()
            .zip(embeddings)
            .map(|(chunk, embedding)| -> Result<PointStructPersisted> {
                let id = chunk.id.parse::<PointId>().map_err(|_| {
                    anyhow!(
                        "chunk id '{}' is not a valid Qdrant Edge point id",
                        chunk.id
                    )
                })?;
                let mut payload = serde_json::Map::from_iter([
                    ("chunk_id".to_string(), Value::String(chunk.id.clone())),
                    (
                        "file_path".to_string(),
                        Value::String(chunk.file_path.clone()),
                    ),
                    ("content".to_string(), Value::String(chunk.content.clone())),
                    ("kind".to_string(), Value::String(chunk.kind.clone())),
                    (
                        "metadata".to_string(),
                        Value::Object(chunk.metadata.clone().into_iter().collect()),
                    ),
                    ("type".to_string(), Value::String(chunk.kind.clone())),
                ]);

                if let Some(language) = chunk.metadata_str(LANGUAGE_KEY) {
                    payload.insert("language".to_string(), Value::String(language.to_string()));
                }
                if let Some(symbol) = chunk.metadata_str(SYMBOL_KEY) {
                    payload.insert("symbol".to_string(), Value::String(symbol.to_string()));
                }
                if let Some(line_start) = chunk.metadata_usize(LINE_START_KEY) {
                    payload.insert(
                        "line_start".to_string(),
                        Value::Number((line_start as u64).into()),
                    );
                }
                if let Some(line_end) = chunk.metadata_usize(LINE_END_KEY) {
                    payload.insert(
                        "line_end".to_string(),
                        Value::Number((line_end as u64).into()),
                    );
                }

                Ok(PointStruct::new(
                    id,
                    Vectors::new_named([(self.vector_name.as_str(), embedding.clone())]),
                    Value::Object(payload),
                )
                .into())
            })
            .collect::<Result<_>>()?;

        self.with_locked_shard(None, |shard| {
            shard
                .update(UpdateOperation::PointOperation(
                    PointOperations::UpsertPoints(PointInsertOperations::PointsList(points)),
                ))
                .map_err(edge_error)
                .context("failed to upsert points into edge shard")
        })
    }

    async fn search_indexed(
        &self,
        embedding: &[f32],
        limit: usize,
        filters: &HashMap<String, String>,
    ) -> Result<Vec<SearchHit>> {
        let filter = build_filter(filters)?;

        self.with_locked_shard(Some(embedding.len()), |shard| {
            let results = shard
                .query(QueryRequest {
                    prefetches: vec![],
                    query: Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery::new(
                        VectorInternal::Dense(embedding.to_vec()),
                        self.vector_name.clone(),
                    )))),
                    filter,
                    score_threshold: None,
                    limit,
                    offset: 0,
                    params: None,
                    with_vector: WithVector::Bool(false),
                    with_payload: WithPayloadInterface::Bool(true),
                })
                .map_err(edge_error)
                .context("failed to search edge shard")?;

            results
                .into_iter()
                .map(search_result_from_scored_point)
                .collect()
        })
    }

    async fn delete_by_file(&self, file_path: &str) -> Result<()> {
        let filter = Filter::new_must(Condition::Field(FieldCondition::new_match(
            parse_json_path("file_path")?,
            Match::Value(MatchValue {
                value: ValueVariants::String(file_path.to_string()),
            }),
        )));

        self.with_locked_shard(None, |shard| {
            shard
                .update(UpdateOperation::PointOperation(
                    PointOperations::DeletePointsByFilter(filter),
                ))
                .map_err(edge_error)
                .with_context(|| format!("failed to delete points for file '{}'", file_path))
        })
    }
}

fn build_filter(filters: &HashMap<String, String>) -> Result<Option<Filter>> {
    if filters.is_empty() {
        return Ok(None);
    }

    let must = filters
        .iter()
        .map(|(key, value)| {
            Ok(Condition::Field(FieldCondition::new_match(
                parse_json_path(key)?,
                Match::Value(MatchValue {
                    value: ValueVariants::String(value.clone()),
                }),
            )))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Some(Filter {
        should: None,
        min_should: None,
        must: Some(must),
        must_not: None,
    }))
}

fn search_result_from_scored_point(point: ScoredPoint) -> Result<SearchHit> {
    let payload = point
        .payload
        .ok_or_else(|| anyhow!("edge search result missing payload"))?;

    let metadata = payload_metadata(&payload)?;
    let kind = payload_string(&payload, "kind")?
        .or_else(|| payload_string(&payload, "type").ok().flatten())
        .ok_or_else(|| anyhow!("edge search result missing kind payload"))?;

    Ok(SearchHit {
        chunk: IndexedChunk {
            id: payload_string(&payload, "chunk_id")?.unwrap_or_else(|| point.id.to_string()),
            file_path: payload_string(&payload, "file_path")?
                .ok_or_else(|| anyhow!("edge search result missing file_path payload"))?,
            content: payload_string(&payload, "content")?
                .ok_or_else(|| anyhow!("edge search result missing content payload"))?,
            kind,
            metadata,
        },
        score: point.score,
    })
}

fn payload_metadata(payload: &Payload) -> Result<HashMap<String, Value>> {
    if let Some(Value::Object(metadata)) = payload.0.get("metadata") {
        return Ok(metadata.clone().into_iter().collect());
    }

    let mut metadata = HashMap::new();
    if let Some(symbol) = payload_string(payload, "symbol")? {
        metadata.insert(SYMBOL_KEY.to_string(), Value::String(symbol));
    }
    if let Some(language) = payload_string(payload, "language")? {
        metadata.insert(LANGUAGE_KEY.to_string(), Value::String(language));
    }
    if let Some(line_start) = payload_usize(payload, "line_start")? {
        metadata.insert(
            LINE_START_KEY.to_string(),
            Value::Number((line_start as u64).into()),
        );
    }
    if let Some(line_end) = payload_usize(payload, "line_end")? {
        metadata.insert(
            LINE_END_KEY.to_string(),
            Value::Number((line_end as u64).into()),
        );
    }

    Ok(metadata)
}

fn payload_string(payload: &Payload, key: &str) -> Result<Option<String>> {
    match payload.0.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(other) => Err(anyhow!(
            "edge payload key '{}' expected string, got {}",
            key,
            other
        )),
    }
}

fn payload_usize(payload: &Payload, key: &str) -> Result<Option<usize>> {
    match payload.0.get(key) {
        None => Ok(None),
        Some(Value::Number(value)) => value
            .as_u64()
            .map(|n| n as usize)
            .ok_or_else(|| anyhow!("edge payload key '{}' is not an unsigned integer", key))
            .map(Some),
        Some(other) => Err(anyhow!(
            "edge payload key '{}' expected number, got {}",
            key,
            other
        )),
    }
}

fn edge_error(err: qdrant_edge::OperationError) -> anyhow::Error {
    anyhow!(err.to_string())
}

fn parse_json_path(path: &str) -> Result<JsonPath> {
    path.parse()
        .map_err(|_| anyhow!("invalid edge payload path '{}'", path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::models::{CodeChunk, CodeSearchResult};
    use serde_json::json;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_shard_path(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("compas-edge-{test_name}-{nanos}"))
    }

    fn sample_chunk(id: &str, file_path: &str, symbol: &str, language: &str) -> CodeChunk {
        CodeChunk {
            id: id.to_string(),
            content: format!("{symbol} body"),
            language: language.to_string(),
            file_path: file_path.to_string(),
            symbol: symbol.to_string(),
            line_start: 1,
            line_end: 5,
            kind: "method".to_string(),
            meta: Default::default(),
        }
    }

    fn sample_indexed_chunk(id: &str, file_path: &str) -> IndexedChunk {
        IndexedChunk {
            id: id.to_string(),
            content: "section body".to_string(),
            file_path: file_path.to_string(),
            kind: "section".to_string(),
            metadata: HashMap::from([
                ("title".to_string(), json!("Annual Report 2024")),
                ("page_start".to_string(), json!(37)),
            ]),
        }
    }

    #[tokio::test]
    async fn edge_store_upsert_search_delete_and_reload() {
        let shard_path = temp_shard_path("lifecycle");
        let store = EdgeStore::new(&shard_path, "default");
        store.init(4).await.unwrap();

        let chunks = vec![
            sample_chunk(
                "11111111-1111-1111-1111-111111111111",
                "/tmp/lib/auth.dart",
                "AuthService.login",
                "dart",
            ),
            sample_chunk(
                "22222222-2222-2222-2222-222222222222",
                "/tmp/lib/cache.dart",
                "CacheService.save",
                "dart",
            ),
        ];
        let embeddings = vec![vec![1.0, 0.0, 0.0, 0.0], vec![0.0, 1.0, 0.0, 0.0]];

        let indexed_chunks: Vec<IndexedChunk> =
            chunks.into_iter().map(IndexedChunk::from).collect();
        store
            .upsert_indexed(&indexed_chunks, &embeddings)
            .await
            .unwrap();

        let results: Vec<CodeSearchResult> = store
            .search_indexed(&[1.0, 0.0, 0.0, 0.0], 5, &HashMap::new())
            .await
            .unwrap()
            .into_iter()
            .map(CodeSearchResult::try_from)
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].chunk.id, "11111111-1111-1111-1111-111111111111");
        assert_eq!(results[0].chunk.symbol, "AuthService.login");

        store.delete_by_file("/tmp/lib/auth.dart").await.unwrap();

        let remaining: Vec<CodeSearchResult> = store
            .search_indexed(&[1.0, 0.0, 0.0, 0.0], 5, &HashMap::new())
            .await
            .unwrap()
            .into_iter()
            .map(CodeSearchResult::try_from)
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].chunk.file_path, "/tmp/lib/cache.dart");

        drop(store);

        let reopened = EdgeStore::new(&shard_path, "default");
        let persisted: Vec<CodeSearchResult> = reopened
            .search_indexed(&[0.0, 1.0, 0.0, 0.0], 5, &HashMap::new())
            .await
            .unwrap()
            .into_iter()
            .map(CodeSearchResult::try_from)
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].chunk.symbol, "CacheService.save");

        drop(reopened);
        fs::remove_dir_all(shard_path).unwrap();
    }

    #[test]
    fn edge_store_new_does_not_create_shard_or_lock_files() {
        let shard_path = temp_shard_path("constructor-only");
        let lock_path = shard_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!(
                "{}.lock",
                shard_path.file_name().unwrap().to_string_lossy()
            ));

        let store = EdgeStore::new(&shard_path, "default");

        assert_eq!(store.shard_path(), shard_path.as_path());
        assert!(
            !shard_path.exists(),
            "constructor should not create shard path"
        );
        assert!(
            !lock_path.exists(),
            "constructor should not create lock file"
        );
    }

    #[tokio::test]
    async fn edge_store_search_respects_filters() {
        let shard_path = temp_shard_path("filters");
        let store = EdgeStore::new(&shard_path, "default");
        store.init(4).await.unwrap();

        let chunks = vec![
            sample_chunk(
                "33333333-3333-3333-3333-333333333333",
                "/tmp/lib/auth.dart",
                "AuthService.login",
                "dart",
            ),
            sample_chunk(
                "44444444-4444-4444-4444-444444444444",
                "/tmp/src/auth.rs",
                "AuthService::login",
                "rust",
            ),
        ];
        let embeddings = vec![vec![0.8, 0.2, 0.0, 0.0], vec![0.8, 0.2, 0.0, 0.0]];
        let indexed_chunks: Vec<IndexedChunk> =
            chunks.into_iter().map(IndexedChunk::from).collect();
        store
            .upsert_indexed(&indexed_chunks, &embeddings)
            .await
            .unwrap();

        let mut filters = HashMap::new();
        filters.insert("language".to_string(), "dart".to_string());

        let results: Vec<CodeSearchResult> = store
            .search_indexed(&[0.8, 0.2, 0.0, 0.0], 5, &filters)
            .await
            .unwrap()
            .into_iter()
            .map(CodeSearchResult::try_from)
            .collect::<Result<_>>()
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk.language, "dart");
        assert_eq!(results[0].chunk.file_path, "/tmp/lib/auth.dart");

        drop(store);
        fs::remove_dir_all(shard_path).unwrap();
    }

    #[tokio::test]
    async fn edge_store_rejects_non_uuid_or_numeric_chunk_ids() {
        let shard_path = temp_shard_path("invalid-id");
        let store = EdgeStore::new(&shard_path, "default");
        store.init(4).await.unwrap();

        let chunk = sample_chunk(
            "not-a-valid-point-id",
            "/tmp/lib/auth.dart",
            "Bad.id",
            "dart",
        );
        let err = store
            .upsert_indexed(&[IndexedChunk::from(chunk)], &[vec![1.0, 0.0, 0.0, 0.0]])
            .await
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("is not a valid Qdrant Edge point id"));

        drop(store);
        fs::remove_dir_all(shard_path).unwrap();
    }

    #[tokio::test]
    async fn edge_store_rejects_incompatible_vector_size() {
        let shard_path = temp_shard_path("vector-size-mismatch");
        let store = EdgeStore::new(&shard_path, "default");
        store.init(4).await.unwrap();
        drop(store);

        let reopened = EdgeStore::new(&shard_path, "default");
        let err = reopened.init(8).await.unwrap_err();
        let message = err.to_string();
        assert!(message.contains("incompatible with vector 'default' and dimension 8"));

        fs::remove_dir_all(shard_path).unwrap();
    }

    #[tokio::test]
    async fn edge_store_rejects_incompatible_vector_name() {
        let shard_path = temp_shard_path("vector-name-mismatch");
        let store = EdgeStore::new(&shard_path, "default");
        store.init(4).await.unwrap();
        drop(store);

        let reopened = EdgeStore::new(&shard_path, "secondary");
        let err = reopened.init(4).await.unwrap_err();
        let message = err.to_string();
        assert!(message.contains("incompatible with vector 'secondary' and dimension 4"));

        fs::remove_dir_all(shard_path).unwrap();
    }

    #[tokio::test]
    async fn edge_store_handles_large_batch_and_optimize() {
        let shard_path = temp_shard_path("large-batch");
        let store = EdgeStore::new(&shard_path, "default");
        store.init(4).await.unwrap();

        let chunks: Vec<CodeChunk> = (0..128)
            .map(|index| {
                sample_chunk(
                    &format!("00000000-0000-0000-0000-{:012}", index + 1),
                    &format!("/tmp/lib/file_{index}.dart"),
                    &format!("Service{index}.run"),
                    "dart",
                )
            })
            .collect();
        let embeddings: Vec<Vec<f32>> = (0..128)
            .map(|index| vec![1.0, index as f32 / 128.0, 0.0, 0.0])
            .collect();

        let indexed_chunks: Vec<IndexedChunk> =
            chunks.into_iter().map(IndexedChunk::from).collect();
        store
            .upsert_indexed(&indexed_chunks, &embeddings)
            .await
            .unwrap();
        let _ = store.optimize().unwrap();

        let results = store
            .search_indexed(&[1.0, 0.0, 0.0, 0.0], 10, &HashMap::new())
            .await
            .unwrap();
        assert_eq!(results.len(), 10);

        drop(store);
        fs::remove_dir_all(shard_path).unwrap();
    }

    #[tokio::test]
    async fn edge_store_supports_concurrent_searches() {
        let shard_path = temp_shard_path("concurrent-search");
        let store = Arc::new(EdgeStore::new(&shard_path, "default"));
        store.init(4).await.unwrap();

        let chunks = vec![sample_chunk(
            "55555555-5555-5555-5555-555555555555",
            "/tmp/lib/auth.dart",
            "AuthService.login",
            "dart",
        )];
        let embeddings = vec![vec![1.0, 0.0, 0.0, 0.0]];
        let indexed_chunks: Vec<IndexedChunk> =
            chunks.into_iter().map(IndexedChunk::from).collect();
        store
            .upsert_indexed(&indexed_chunks, &embeddings)
            .await
            .unwrap();

        let tasks: Vec<_> = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                tokio::spawn(async move {
                    store
                        .search_indexed(&[1.0, 0.0, 0.0, 0.0], 5, &HashMap::new())
                        .await
                        .unwrap()
                        .into_iter()
                        .map(CodeSearchResult::try_from)
                        .collect::<Result<Vec<_>>>()
                        .unwrap()
                })
            })
            .collect();

        for task in tasks {
            let results = task.await.unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].chunk.symbol, "AuthService.login");
        }

        drop(store);
        fs::remove_dir_all(shard_path).unwrap();
    }

    #[tokio::test]
    async fn edge_store_supports_multiple_instances_on_same_shard() {
        let shard_path = temp_shard_path("multi-instance");
        let writer = Arc::new(EdgeStore::new(&shard_path, "default"));
        writer.init(4).await.unwrap();

        let chunks = vec![sample_chunk(
            "66666666-6666-6666-6666-666666666666",
            "/tmp/lib/auth.dart",
            "AuthService.login",
            "dart",
        )];
        let embeddings = vec![vec![1.0, 0.0, 0.0, 0.0]];
        let indexed_chunks: Vec<IndexedChunk> =
            chunks.into_iter().map(IndexedChunk::from).collect();
        writer
            .upsert_indexed(&indexed_chunks, &embeddings)
            .await
            .unwrap();

        let reader = Arc::new(EdgeStore::new(&shard_path, "default"));

        let writer_task = {
            let writer = Arc::clone(&writer);
            tokio::spawn(async move {
                writer
                    .search_indexed(&[1.0, 0.0, 0.0, 0.0], 5, &HashMap::new())
                    .await
                    .unwrap()
                    .into_iter()
                    .map(CodeSearchResult::try_from)
                    .collect::<Result<Vec<_>>>()
                    .unwrap()
            })
        };
        let reader_task = {
            let reader = Arc::clone(&reader);
            tokio::spawn(async move {
                reader
                    .search_indexed(&[1.0, 0.0, 0.0, 0.0], 5, &HashMap::new())
                    .await
                    .unwrap()
                    .into_iter()
                    .map(CodeSearchResult::try_from)
                    .collect::<Result<Vec<_>>>()
                    .unwrap()
            })
        };

        let writer_results = writer_task.await.unwrap();
        let reader_results = reader_task.await.unwrap();

        assert_eq!(writer_results.len(), 1);
        assert_eq!(reader_results.len(), 1);
        assert_eq!(writer_results[0].chunk.symbol, "AuthService.login");
        assert_eq!(reader_results[0].chunk.symbol, "AuthService.login");

        drop(writer);
        drop(reader);
        fs::remove_dir_all(shard_path).unwrap();
    }

    #[tokio::test]
    async fn edge_store_round_trips_indexed_chunk_metadata() {
        let shard_path = temp_shard_path("indexed-metadata");
        let store = EdgeStore::new(&shard_path, "default");
        store.init(4).await.unwrap();

        let chunk = sample_indexed_chunk(
            "77777777-7777-7777-7777-777777777777",
            "/tmp/docs/report.pdf",
        );
        store
            .upsert_indexed(&[chunk], &[vec![1.0, 0.0, 0.0, 0.0]])
            .await
            .unwrap();

        let results = store
            .search_indexed(&[1.0, 0.0, 0.0, 0.0], 5, &HashMap::new())
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk.kind, "section");
        assert_eq!(
            results[0].chunk.metadata_str("title"),
            Some("Annual Report 2024")
        );
        assert_eq!(results[0].chunk.metadata_usize("page_start"), Some(37));

        drop(store);
        fs::remove_dir_all(shard_path).unwrap();
    }

    #[tokio::test]
    async fn edge_store_reads_legacy_code_payload_without_metadata_map() {
        let shard_path = temp_shard_path("legacy-payload");
        let store = EdgeStore::new(&shard_path, "default");
        store.init(4).await.unwrap();

        store
            .with_locked_shard(None, |shard| {
                let point = PointStruct::new(
                    "88888888-8888-8888-8888-888888888888"
                        .parse::<PointId>()
                        .unwrap(),
                    Vectors::new_named([("default", vec![1.0, 0.0, 0.0, 0.0])]),
                    serde_json::json!({
                        "chunk_id": "88888888-8888-8888-8888-888888888888",
                        "file_path": "/tmp/lib/auth.dart",
                        "symbol": "AuthService.login",
                        "language": "dart",
                        "type": "method",
                        "content": "auth body",
                        "line_start": 10,
                        "line_end": 18
                    }),
                );

                shard
                    .update(UpdateOperation::PointOperation(
                        PointOperations::UpsertPoints(PointInsertOperations::PointsList(vec![
                            point.into(),
                        ])),
                    ))
                    .map_err(edge_error)
                    .context("failed to insert legacy payload")
            })
            .unwrap();

        let results = store
            .search_indexed(&[1.0, 0.0, 0.0, 0.0], 5, &HashMap::new())
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk.kind, "method");
        assert_eq!(
            results[0].chunk.metadata_str(SYMBOL_KEY),
            Some("AuthService.login")
        );
        assert_eq!(results[0].chunk.metadata_str(LANGUAGE_KEY), Some("dart"));
        assert_eq!(results[0].chunk.metadata_usize(LINE_START_KEY), Some(10));
        assert_eq!(results[0].chunk.metadata_usize(LINE_END_KEY), Some(18));

        drop(store);
        fs::remove_dir_all(shard_path).unwrap();
    }
}
