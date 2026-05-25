use crate::embedder::{EmbedMode, Embedder};
use crate::models::IndexedChunk;
use crate::store::Store;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

const EMBED_BATCH_SIZE: usize = 32;

pub fn hash_bytes(bytes: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    format!("{:x}", hasher.finish())
}

pub struct CompasIgnore {
    matchers: Vec<globset::GlobMatcher>,
}

impl CompasIgnore {
    pub fn load(repo_path: &Path) -> Self {
        let path = repo_path.join(".compasignore");
        let mut matchers = Vec::new();

        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Ok(glob) = globset::Glob::new(line) {
                    matchers.push(glob.compile_matcher());
                }
                if line.ends_with('/') {
                    let nested = format!("{}**", line);
                    if let Ok(glob) = globset::Glob::new(&nested) {
                        matchers.push(glob.compile_matcher());
                    }
                }
            }
        }

        Self { matchers }
    }

    pub fn is_ignored(&self, relative_path: &Path) -> bool {
        let path_str = relative_path.to_string_lossy();
        self.matchers
            .iter()
            .any(|matcher| matcher.is_match(&*path_str))
    }
}

pub fn should_include(path: &Path, include: &[String], exclude: &[String]) -> bool {
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

pub trait IndexingAdapter: Send + Sync {
    type PreparedFile;

    fn supports_path(&self, path: &Path) -> bool;
    fn prepare_file(&self, file_path: &str, content: &str) -> Result<Option<Self::PreparedFile>>;
    fn indexed_chunks<'a>(&self, prepared: &'a Self::PreparedFile) -> &'a [IndexedChunk];
    fn after_upsert(
        &self,
        file_path: &str,
        content: &str,
        prepared: &Self::PreparedFile,
    ) -> Result<()>;
    fn after_delete(&self, file_path: &str) -> Result<()>;
}

pub struct IndexingReport {
    pub manifest: HashMap<String, String>,
    pub processed_paths: HashSet<String>,
    pub total_files: usize,
    pub changed_files: usize,
    pub deleted_files: usize,
    pub processed_files: usize,
    pub skipped_files: usize,
    pub failed_files: usize,
    pub total_chunks: usize,
    pub elapsed: Duration,
    pub used_tui: bool,
}

pub struct Indexer<'a> {
    repo_path: &'a Path,
    include: &'a [String],
    exclude: &'a [String],
    store: &'a dyn Store,
    embedder: &'a dyn Embedder,
}

impl<'a> Indexer<'a> {
    pub fn new(
        repo_path: &'a Path,
        include: &'a [String],
        exclude: &'a [String],
        store: &'a dyn Store,
        embedder: &'a dyn Embedder,
    ) -> Self {
        Self {
            repo_path,
            include,
            exclude,
            store,
            embedder,
        }
    }

    pub async fn index_repo<A: IndexingAdapter>(&self, adapter: &A) -> Result<IndexingReport> {
        let compas_ignore = CompasIgnore::load(self.repo_path);

        let mut files_with_hashes: Vec<(PathBuf, String)> = Vec::new();
        for entry in walkdir::WalkDir::new(self.repo_path) {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    warn!("walkdir error: {}", e);
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();
            let relative = path.strip_prefix(self.repo_path).unwrap_or(path);
            if !should_include(relative, self.include, self.exclude) {
                continue;
            }
            if compas_ignore.is_ignored(relative) {
                continue;
            }
            if !adapter.supports_path(path) {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(content) => content,
                Err(e) => {
                    warn!("skip read error for {}: {}", path.display(), e);
                    continue;
                }
            };

            files_with_hashes.push((path.to_path_buf(), hash_bytes(content.as_bytes())));
        }

        let old_manifest = self.load_manifest();
        let mut new_manifest = old_manifest.clone();
        let current_paths: HashSet<String> = files_with_hashes
            .iter()
            .map(|(path, _)| path.to_string_lossy().to_string())
            .collect();

        let deleted_files: Vec<String> = old_manifest
            .keys()
            .filter(|path| !current_paths.contains(*path))
            .cloned()
            .collect();

        for path in &deleted_files {
            if let Err(e) = self.store.delete_by_file(path).await {
                warn!("failed to delete chunks for removed file {}: {}", path, e);
            }
            if let Err(e) = adapter.after_delete(path) {
                warn!("failed to clean up removed file {}: {}", path, e);
            }
            new_manifest.remove(path);
        }

        let changed_count = files_with_hashes
            .iter()
            .filter(|(path, hash)| {
                old_manifest.get(&path.to_string_lossy().to_string()) != Some(hash)
            })
            .count();

        let use_tui = std::env::var("RUST_LOG").is_err() && std::io::stderr().is_terminal();

        if use_tui {
            println!(
                "Indexing {}  ({} files, {} changed, {} deleted)",
                self.repo_path.display(),
                files_with_hashes.len(),
                changed_count,
                deleted_files.len()
            );
        } else {
            info!(
                "indexing {:?} ({} files, {} changed, {} deleted)",
                self.repo_path,
                files_with_hashes.len(),
                changed_count,
                deleted_files.len()
            );
        }

        let progress = if use_tui {
            let bar = ProgressBar::new(files_with_hashes.len() as u64);
            bar.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("=>-"),
            );
            bar.set_message("starting...");
            Some(bar)
        } else {
            None
        };

        let start = Instant::now();
        let mut processed = 0usize;
        let mut processed_paths = HashSet::new();
        let mut skipped = 0usize;
        let mut failed = 0usize;
        let mut total_chunks = 0usize;

        for (path, hash) in &files_with_hashes {
            let path_str = path.to_string_lossy().to_string();
            let relative = path.strip_prefix(self.repo_path).unwrap_or(path);
            let rel_str = relative.to_string_lossy().to_string();

            if old_manifest.get(&path_str) == Some(hash) {
                if let Some(ref bar) = progress {
                    bar.set_message(format!("skipping {}", rel_str));
                    bar.inc(1);
                } else {
                    debug!("skipping unchanged file: {}", rel_str);
                }
                skipped += 1;
                continue;
            }

            if let Some(ref bar) = progress {
                bar.set_message(rel_str.clone());
            } else {
                info!("→ {}", rel_str);
            }

            let content = match tokio::fs::read_to_string(path).await {
                Ok(content) => content,
                Err(e) => {
                    if let Some(ref bar) = progress {
                        bar.println(format!("⚠  skip read error in {}: {}", rel_str, e));
                        bar.inc(1);
                    } else {
                        warn!("  skip read error: {}", e);
                    }
                    failed += 1;
                    continue;
                }
            };

            let line_count = content.lines().count();
            if line_count > 1000 {
                if let Some(ref bar) = progress {
                    bar.println(format!("⚠  large file: {} ({} lines)", rel_str, line_count));
                } else {
                    warn!("  ⚠️  large file: {} lines", line_count);
                }
            }

            let prepared = match adapter.prepare_file(&path_str, &content) {
                Ok(Some(prepared)) => prepared,
                Ok(None) => {
                    if let Some(ref bar) = progress {
                        bar.inc(1);
                    }
                    continue;
                }
                Err(e) => {
                    if let Some(ref bar) = progress {
                        bar.println(format!("⚠  skip chunk error in {}: {}", rel_str, e));
                        bar.inc(1);
                    } else {
                        warn!("  skip chunk error: {}", e);
                    }
                    failed += 1;
                    continue;
                }
            };

            let chunk_count = adapter.indexed_chunks(&prepared).len();
            if let Err(e) = self.store.delete_by_file(&path_str).await {
                if let Some(ref bar) = progress {
                    bar.println(format!(
                        "⚠  failed to delete old chunks in {}: {}",
                        rel_str, e
                    ));
                } else {
                    warn!("  failed to delete old chunks: {}", e);
                }
            }

            if chunk_count == 0 {
                if let Err(e) = adapter.after_delete(&path_str) {
                    warn!("failed to clean up empty file {}: {}", path_str, e);
                }
                new_manifest.remove(&path_str);
                if let Some(ref bar) = progress {
                    bar.inc(1);
                } else {
                    info!("  0 chunks, skipping");
                }
                continue;
            }

            let embeddings = match self
                .embed_chunks(
                    adapter.indexed_chunks(&prepared),
                    &rel_str,
                    progress.as_ref(),
                )
                .await
            {
                Ok(embeddings) => embeddings,
                Err(e) => {
                    if let Some(ref bar) = progress {
                        bar.inc(1);
                    }
                    failed += 1;
                    if !use_tui {
                        warn!("  skip embed error: {}", e);
                    }
                    continue;
                }
            };

            if let Err(e) = self
                .store
                .upsert_indexed(adapter.indexed_chunks(&prepared), &embeddings)
                .await
            {
                if let Some(ref bar) = progress {
                    bar.println(format!("⚠  skip upsert error in {}: {}", rel_str, e));
                    bar.inc(1);
                } else {
                    warn!("  skip upsert error: {}", e);
                }
                failed += 1;
                continue;
            }

            if let Err(e) = adapter.after_upsert(&path_str, &content, &prepared) {
                if let Some(ref bar) = progress {
                    bar.println(format!("⚠  skip post-index error in {}: {}", rel_str, e));
                    bar.inc(1);
                } else {
                    warn!("  skip post-index error: {}", e);
                }
                failed += 1;
                continue;
            }

            processed += 1;
            total_chunks += chunk_count;
            processed_paths.insert(path_str.clone());
            new_manifest.insert(path_str, hash.clone());

            if let Some(ref bar) = progress {
                bar.inc(1);
            } else {
                info!("  ✓ done ({} chunks)", chunk_count);
            }
        }

        if let Some(bar) = progress {
            bar.finish_and_clear();
        }

        self.save_manifest(&new_manifest).await?;

        Ok(IndexingReport {
            manifest: new_manifest,
            processed_paths,
            total_files: files_with_hashes.len(),
            changed_files: changed_count,
            deleted_files: deleted_files.len(),
            processed_files: processed,
            skipped_files: skipped,
            failed_files: failed,
            total_chunks,
            elapsed: start.elapsed(),
            used_tui: use_tui,
        })
    }

    pub async fn reindex_file<A: IndexingAdapter>(
        &self,
        adapter: &A,
        file_path: &str,
    ) -> Result<Option<usize>> {
        let path = Path::new(file_path);
        if !path.is_file() {
            return Ok(None);
        }

        let relative = path.strip_prefix(self.repo_path).unwrap_or(path);
        if !should_include(relative, self.include, self.exclude) {
            return Ok(None);
        }

        let compas_ignore = CompasIgnore::load(self.repo_path);
        if compas_ignore.is_ignored(relative) || !adapter.supports_path(path) {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(file_path).await?;
        let prepared = match adapter.prepare_file(file_path, &content)? {
            Some(prepared) => prepared,
            None => return Ok(None),
        };

        if let Err(e) = self.store.delete_by_file(file_path).await {
            warn!("failed to delete old chunks for {}: {}", file_path, e);
        }

        let chunk_count = adapter.indexed_chunks(&prepared).len();
        let mut manifest = self.load_manifest();

        if chunk_count == 0 {
            adapter.after_delete(file_path)?;
            manifest.remove(file_path);
            self.save_manifest(&manifest).await?;
            return Ok(Some(0));
        }

        let embeddings = self
            .embed_chunks(adapter.indexed_chunks(&prepared), file_path, None)
            .await?;
        self.store
            .upsert_indexed(adapter.indexed_chunks(&prepared), &embeddings)
            .await?;
        adapter.after_upsert(file_path, &content, &prepared)?;
        manifest.insert(file_path.to_string(), hash_bytes(content.as_bytes()));
        self.save_manifest(&manifest).await?;

        Ok(Some(chunk_count))
    }

    pub async fn delete_file<A: IndexingAdapter>(
        &self,
        adapter: &A,
        file_path: &str,
    ) -> Result<bool> {
        let mut manifest = self.load_manifest();
        let tracked = manifest.contains_key(file_path);
        let path = Path::new(file_path);
        let relative = path.strip_prefix(self.repo_path).unwrap_or(path);
        let compas_ignore = CompasIgnore::load(self.repo_path);

        if !tracked
            && (!should_include(relative, self.include, self.exclude)
                || compas_ignore.is_ignored(relative)
                || !adapter.supports_path(path))
        {
            return Ok(false);
        }

        if let Err(e) = self.store.delete_by_file(file_path).await {
            warn!("failed to delete chunks for {}: {}", file_path, e);
        }

        adapter.after_delete(file_path)?;
        manifest.remove(file_path);
        self.save_manifest(&manifest).await?;

        Ok(true)
    }

    async fn save_manifest(&self, manifest: &HashMap<String, String>) -> Result<()> {
        let manifest_path = self.manifest_path();
        if let Some(parent) = manifest_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let manifest_json = serde_json::to_string_pretty(manifest)?;
        tokio::fs::write(manifest_path, manifest_json).await?;
        Ok(())
    }

    fn load_manifest(&self) -> HashMap<String, String> {
        std::fs::read_to_string(self.manifest_path())
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    fn manifest_path(&self) -> PathBuf {
        self.repo_path.join(".compas").join("manifest.json")
    }

    async fn embed_chunks(
        &self,
        chunks: &[IndexedChunk],
        display_path: &str,
        progress: Option<&ProgressBar>,
    ) -> Result<Vec<Vec<f32>>> {
        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());

        for chunk_batch in chunks.chunks(EMBED_BATCH_SIZE) {
            let texts: Vec<String> = chunk_batch
                .iter()
                .map(|chunk| chunk.content.clone())
                .collect();
            match self.embedder.embed_batch(&texts, EmbedMode::Document).await {
                Ok(batch_embeddings) => embeddings.extend(batch_embeddings),
                Err(e) => {
                    if let Some(bar) = progress {
                        bar.println(format!("⚠  skip embed error in {}: {}", display_path, e));
                    }
                    return Err(e);
                }
            }
        }

        Ok(embeddings)
    }
}
