use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, Once};

const DEFAULT_FASTEMBED_MAX_BATCH_SIZE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedMode {
    Query,
    Document,
}

#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str, mode: EmbedMode) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String], mode: EmbedMode) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
}

pub fn build_embedder(config: &crate::config::EmbedderConfig) -> Result<Arc<dyn Embedder>> {
    match config.provider.as_str() {
        "fastembed" => Ok(Arc::new(FastEmbedEmbedder::new(
            &config.model,
            config.query_prefix.clone().unwrap_or_default(),
            config.doc_prefix.clone().unwrap_or_default(),
        )?)),
        "test" => Ok(Arc::new(TestEmbedder)),
        other => Err(anyhow::anyhow!(
            "unsupported embedder provider '{}'; only 'fastembed' and 'test' are supported",
            other
        )),
    }
}

pub struct FastEmbedEmbedder {
    model: Arc<std::sync::Mutex<TextEmbedding>>,
    query_prefix: String,
    doc_prefix: String,
    dims: usize,
}

impl FastEmbedEmbedder {
    pub fn new(
        model_name: &str,
        query_prefix: impl Into<String>,
        doc_prefix: impl Into<String>,
    ) -> Result<Self> {
        configure_fastembed_runtime();
        let model_variant = parse_model(model_name)?;
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join("compas")
            .join("fastembed");
        std::fs::create_dir_all(&cache_dir).ok();

        let mut model =
            TextEmbedding::try_new(InitOptions::new(model_variant).with_cache_dir(cache_dir))
                .context("failed to initialize FastEmbed model")?;
        let sample = model
            .embed(vec!["sample".to_string()], None)
            .context("failed to compute dimensions from sample embedding")?;

        Ok(Self {
            model: Arc::new(std::sync::Mutex::new(model)),
            query_prefix: query_prefix.into(),
            doc_prefix: doc_prefix.into(),
            dims: sample.first().map(|v| v.len()).unwrap_or(768),
        })
    }

    fn prefix_text(&self, text: &str, mode: EmbedMode) -> String {
        match mode {
            EmbedMode::Query => format!("{}{}", self.query_prefix, text),
            EmbedMode::Document => format!("{}{}", self.doc_prefix, text),
        }
    }
}

fn configure_fastembed_runtime() {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        if std::env::var_os("OMP_NUM_THREADS").is_none() {
            let threads = std::thread::available_parallelism()
                .map(|parallelism| parallelism.get().min(4))
                .unwrap_or(1)
                .max(1);
            std::env::set_var("OMP_NUM_THREADS", threads.to_string());
        }
    });
}

fn parse_model(model: &str) -> Result<EmbeddingModel> {
    match normalize_model_name(model).as_str() {
        "nomicainomicembedtextv15q" | "nomicembedtextv15q" => {
            Ok(EmbeddingModel::NomicEmbedTextV15Q)
        }
        _ => Err(anyhow::anyhow!("unsupported FastEmbed model '{}'", model)),
    }
}

fn normalize_model_name(model: &str) -> String {
    model
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[async_trait::async_trait]
impl Embedder for FastEmbedEmbedder {
    async fn embed(&self, text: &str, mode: EmbedMode) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text.to_string()], mode).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no embedding returned"))
    }

    async fn embed_batch(&self, texts: &[String], mode: EmbedMode) -> Result<Vec<Vec<f32>>> {
        let prefixed: Vec<String> = texts
            .iter()
            .map(|text| self.prefix_text(text, mode))
            .collect();
        let model = Arc::clone(&self.model);

        tokio::task::spawn_blocking(move || {
            let mut model = model
                .lock()
                .map_err(|_| anyhow::anyhow!("fastembed mutex poisoned"))?;

            // `fastembed` retains intermediate outputs for every internal batch until a single
            // `embed()` call completes. Calling it with an entire folder's chunks at once can
            // drive document indexing into multi-GB RAM usage, so keep each call bounded here.
            let max_batch_size = fastembed_max_batch_size();
            let mut embeddings = Vec::with_capacity(prefixed.len());
            for batch in prefixed.chunks(max_batch_size) {
                let mut batch_embeddings = model
                    .embed(batch, Some(batch.len()))
                    .context("failed to generate FastEmbed embeddings")?;
                embeddings.append(&mut batch_embeddings);
            }

            Ok(embeddings)
        })
        .await
        .context("FastEmbed embedding task panicked")?
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
}

fn fastembed_max_batch_size() -> usize {
    std::env::var("COMPAS_FASTEMBED_BATCH_SIZE")
        .ok()
        .and_then(|value| value.parse::<NonZeroUsize>().ok())
        .map(NonZeroUsize::get)
        .unwrap_or(DEFAULT_FASTEMBED_MAX_BATCH_SIZE)
}

#[derive(Default)]
pub struct TestEmbedder;

#[async_trait::async_trait]
impl Embedder for TestEmbedder {
    async fn embed(&self, text: &str, _mode: EmbedMode) -> Result<Vec<f32>> {
        Ok(embedding_for(text))
    }

    async fn embed_batch(&self, texts: &[String], _mode: EmbedMode) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|text| embedding_for(text)).collect())
    }

    fn dimensions(&self) -> usize {
        768
    }
}

fn embedding_for(text: &str) -> Vec<f32> {
    let lower = text.to_lowercase();
    let mut embedding = vec![0.0; 768];
    if lower.contains("renewal") || lower.contains("policy") || lower.contains("insurance") {
        embedding[0] = 1.0;
    } else if lower.contains("auth") || lower.contains("authentication") || lower.contains("login")
    {
        embedding[1] = 1.0;
    } else if lower.contains("cache") {
        embedding[2] = 1.0;
    } else {
        embedding[3] = 1.0;
    }
    embedding
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fastembed_batch_size_defaults_to_small_safe_value() {
        std::env::remove_var("COMPAS_FASTEMBED_BATCH_SIZE");
        assert_eq!(fastembed_max_batch_size(), DEFAULT_FASTEMBED_MAX_BATCH_SIZE);
    }

    #[test]
    fn fastembed_batch_size_reads_valid_override() {
        std::env::set_var("COMPAS_FASTEMBED_BATCH_SIZE", "7");
        assert_eq!(fastembed_max_batch_size(), 7);
        std::env::remove_var("COMPAS_FASTEMBED_BATCH_SIZE");
    }

    #[test]
    fn fastembed_batch_size_ignores_invalid_override() {
        std::env::set_var("COMPAS_FASTEMBED_BATCH_SIZE", "0");
        assert_eq!(fastembed_max_batch_size(), DEFAULT_FASTEMBED_MAX_BATCH_SIZE);
        std::env::set_var("COMPAS_FASTEMBED_BATCH_SIZE", "abc");
        assert_eq!(fastembed_max_batch_size(), DEFAULT_FASTEMBED_MAX_BATCH_SIZE);
        std::env::remove_var("COMPAS_FASTEMBED_BATCH_SIZE");
    }

    #[test]
    fn parse_model_accepts_quantized_nomic_aliases() {
        assert_eq!(
            parse_model("nomic-ai/nomic-embed-text-v1.5-q").unwrap(),
            EmbeddingModel::NomicEmbedTextV15Q
        );
        assert_eq!(
            parse_model("nomic-embed-text-v1.5-q").unwrap(),
            EmbeddingModel::NomicEmbedTextV15Q
        );
    }
}
