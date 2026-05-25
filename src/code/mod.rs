pub mod graph;
pub mod models;
pub mod ranking;

use std::sync::Arc;

pub struct CodeRuntime {
    pub graph: Arc<graph::Graph>,
}
