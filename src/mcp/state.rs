use crate::code::CodeRuntime;
use crate::embedder::Embedder;
use crate::store::Store;
use std::collections::HashMap;
use std::sync::Arc;

pub struct RepoState {
    pub store: Arc<dyn Store>,
    pub code: Option<CodeRuntime>,
    pub embedder: Arc<dyn Embedder>,
}

pub struct McpAppState {
    pub repos: HashMap<String, RepoState>,
    pub default_repo: Option<String>,
}
