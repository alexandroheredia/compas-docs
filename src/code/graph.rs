use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolNode {
    pub name: String,
    pub file: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub calls: Vec<String>,
    #[serde(rename = "called_by")]
    pub called_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GraphData {
    nodes: HashMap<String, SymbolNode>,
    edges: HashMap<String, Vec<String>>,
}

#[derive(Clone)]
pub struct Graph {
    nodes: Arc<RwLock<HashMap<String, SymbolNode>>>,
    edges: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            edges: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_symbol(&self, name: &str, file: &str, kind: &str) {
        let key = symbol_key(name, file);
        let mut nodes = self.nodes.write().unwrap();
        nodes.insert(
            key,
            SymbolNode {
                name: name.into(),
                file: file.into(),
                kind: kind.into(),
                calls: vec![],
                called_by: vec![],
            },
        );
    }

    pub fn add_call(&self, caller_name: &str, caller_file: &str, callee_name: &str) {
        let key = symbol_key(caller_name, caller_file);
        let mut edges = self.edges.write().unwrap();
        edges.entry(key).or_default().push(callee_name.into());
    }

    fn build_calls(&self, key: &str, edges: &HashMap<String, Vec<String>>) -> Vec<String> {
        edges.get(key).cloned().unwrap_or_default()
    }

    fn build_called_by(
        &self,
        name: &str,
        edges: &HashMap<String, Vec<String>>,
        nodes: &HashMap<String, SymbolNode>,
    ) -> Vec<String> {
        let mut called_by = vec![];
        let bare_name = name.rsplit('.').next().unwrap_or(name);
        for (caller_key, callees) in edges {
            if callees.contains(&name.to_string()) || callees.contains(&bare_name.to_string()) {
                if let Some(caller) = nodes.get(caller_key) {
                    called_by.push(caller.name.clone());
                }
            }
        }
        called_by
    }

    pub fn get(&self, name: &str, file: &str) -> Option<SymbolNode> {
        let key = symbol_key(name, file);
        let nodes = self.nodes.read().unwrap();
        let edges = self.edges.read().unwrap();
        let node = nodes.get(&key)?.clone();

        let calls = self.build_calls(&key, &edges);
        let called_by = self.build_called_by(name, &edges, &nodes);

        Some(SymbolNode {
            calls,
            called_by,
            ..node
        })
    }

    pub fn search(&self, query: &str) -> Vec<SymbolNode> {
        let query_lower = query.to_lowercase();
        let nodes = self.nodes.read().unwrap();
        let edges = self.edges.read().unwrap();
        let mut results = vec![];
        let mut seen = std::collections::HashSet::new();

        for (key, node) in nodes.iter() {
            if node.name.to_lowercase().contains(&query_lower) {
                let calls = self.build_calls(key, &edges);
                let called_by = self.build_called_by(&node.name, &edges, &nodes);
                let enriched = SymbolNode {
                    calls,
                    called_by,
                    ..node.clone()
                };
                seen.insert(key.clone());
                results.push(enriched);
            }
        }

        for caller_key in edges.keys() {
            if caller_key.to_lowercase().contains(&query_lower) && !seen.contains(caller_key) {
                if let Some(node) = nodes.get(caller_key) {
                    let calls = self.build_calls(caller_key, &edges);
                    let called_by = self.build_called_by(&node.name, &edges, &nodes);
                    let enriched = SymbolNode {
                        calls,
                        called_by,
                        ..node.clone()
                    };
                    seen.insert(caller_key.clone());
                    results.push(enriched);
                }
            }
        }

        results
    }

    pub fn create_phantom_nodes(&self) {
        let edges = self.edges.read().unwrap();
        let mut nodes = self.nodes.write().unwrap();
        let mut phantom_callees = std::collections::HashSet::new();

        for callees in edges.values() {
            for callee in callees {
                phantom_callees.insert(callee.clone());
            }
        }

        for callee in phantom_callees {
            let exists = nodes.values().any(|n| n.name == callee);
            if !exists {
                let key = format!("phantom:{}", callee);
                nodes.insert(
                    key,
                    SymbolNode {
                        name: callee.clone(),
                        file: "".into(),
                        kind: "external".into(),
                        calls: vec![],
                        called_by: vec![],
                    },
                );
            }
        }
    }

    pub fn all_nodes(&self) -> HashMap<String, SymbolNode> {
        let nodes = self.nodes.read().unwrap();
        let edges = self.edges.read().unwrap();

        nodes
            .iter()
            .map(|(key, node)| {
                let calls = self.build_calls(key, &edges);
                let called_by = self.build_called_by(&node.name, &edges, &nodes);
                (
                    key.clone(),
                    SymbolNode {
                        calls,
                        called_by,
                        ..node.clone()
                    },
                )
            })
            .collect()
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let data = GraphData {
            nodes: self.all_nodes(),
            edges: self.edges.read().unwrap().clone(),
        };
        let json = serde_json::to_string_pretty(&data)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(&self, path: impl AsRef<Path>) -> Result<()> {
        let data = std::fs::read_to_string(path)?;
        let loaded: GraphData = serde_json::from_str(&data)?;
        let mut nodes = self.nodes.write().unwrap();
        let mut edges = self.edges.write().unwrap();
        *nodes = loaded.nodes;
        *edges = loaded.edges;
        Ok(())
    }

    pub fn remove_by_file(&self, file: &str) {
        let mut nodes = self.nodes.write().unwrap();
        let mut edges = self.edges.write().unwrap();

        let prefix = format!("{}:", file);
        let node_keys: Vec<String> = nodes
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in node_keys {
            nodes.remove(&key);
        }

        let edge_keys: Vec<String> = edges
            .keys()
            .filter(|key| key.starts_with(&prefix))
            .cloned()
            .collect();
        for key in edge_keys {
            edges.remove(&key);
        }
    }
}

fn symbol_key(name: impl AsRef<str>, file: impl AsRef<str>) -> String {
    format!("{}:{}", file.as_ref(), name.as_ref())
}
