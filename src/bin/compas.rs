use async_trait::async_trait;
use clap::{Parser, Subcommand};
use compas::{
    chunker::{
        dart::extract_semantic_references, extract_calls_for_language, language_for_path,
        ChunkerRegistry,
    },
    code::{
        graph::Graph,
        models::{CodeChunk, CodeSearchResult},
        ranking::rerank_code_results,
        CodeRuntime,
    },
    config::AppConfig,
    docs::{indexing::DocumentIndexAdapter, models::DocumentSearchResult},
    docs_backend::{
        add_folder, default_document_config, document_storage_paths, index_folder, list_folders,
        normalize_document_storage, open_document, remove_folder, resolved_store_path,
        reveal_in_finder, search_documents,
    },
    embedder::build_embedder,
    indexing::{Indexer, IndexingAdapter},
    mcp::{self, state::McpAppState},
    models::IndexedChunk,
    search::search_chunks,
    server::{router, AppState, RepoState},
    store::{edge::EdgeStore, Store},
    watcher::{FileWatcher, Handler},
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

const FLUTTER_LIFECYCLE_METHODS: &[&str] = &[
    "build",
    "createState",
    "initState",
    "dispose",
    "didChangeDependencies",
    "didUpdateWidget",
    "didChangeAppLifecycleState",
    "deactivate",
    "activate",
    "reassemble",
    "setState",
    "mount",
    "unmount",
];

const FLUTTER_CALLBACK_SUFFIXES: &[&str] = &[
    "onPressed",
    "onTap",
    "onChanged",
    "onSaved",
    "onSubmitted",
    "onEditingComplete",
];

/// Members that are dispatched by frameworks/runtime, not by direct call sites.
/// These should never be flagged as dead code because the absence of a reference
/// in source is meaningless — Flutter, Dart, json_serializable, etc. call them.
const FRAMEWORK_DISPATCHED_MEMBERS: &[&str] = &[
    // Flutter widget lifecycle
    "build",
    "createState",
    "initState",
    "dispose",
    "didChangeDependencies",
    "didUpdateWidget",
    "didChangeAppLifecycleState",
    "deactivate",
    "activate",
    "reassemble",
    "mount",
    "unmount",
    // InheritedWidget / InheritedNotifier
    "updateShouldNotify",
    "updateShouldNotifyDependent",
    // ChangeNotifier / Listenable
    "notifyListeners",
    "addListener",
    "removeListener",
    // Dart object protocol
    "toString",
    "toJson",
    "fromJson",
    "fromMap",
    "toMap",
    "fromSnapshot",
    "fromDoc",
    "fromDocument",
    "noSuchMethod",
    "hashCode",
    // App entrypoint
    "main",
    // Flutter render/paint hooks
    "paint",
    "shouldRepaint",
    "shouldRebuildSemantics",
    "performLayout",
    "performResize",
    // Stream/Future protocol
    "call",
];

fn is_framework_dispatched(member_name: &str) -> bool {
    if FRAMEWORK_DISPATCHED_MEMBERS.contains(&member_name) {
        return true;
    }
    // operator overloads (==, +, -, [], etc.) are dispatched by the runtime
    if member_name.starts_with("operator ") {
        return true;
    }
    false
}

#[derive(Debug, Clone)]
struct AuditDeclaration {
    file: String,
    display_file: String,
    symbol: String,
    kind: String,
}

#[derive(Debug, Clone)]
struct AuditReference {
    #[allow(dead_code)]
    caller_file: String,
    callee: String,
}

#[derive(Debug, Clone, Default)]
struct AuditFileAnalysis {
    declarations: Vec<AuditDeclaration>,
    references: Vec<AuditReference>,
    file_edges: Vec<(String, String)>,
    key_types: Vec<String>,
}

#[derive(Debug, Clone)]
struct DeadCodeCandidate {
    file: String,
    symbol: String,
    kind: String,
}

#[derive(Parser)]
#[command(name = "compas")]
#[command(about = "Your agent's compa. A local-first context engine for LLM agents.")]
struct Cli {
    #[arg(short, long, default_value = "compas.yaml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize compas.yaml for the current repository
    Init,
    /// Index the repository
    Index {
        /// Optional folder to index directly in document mode
        path: Option<PathBuf>,
    },
    /// Search the local index
    Search {
        /// Natural language search query
        query: String,
        /// Optional folder whose local .compas index should be searched
        #[arg(long)]
        path: Option<PathBuf>,
        /// Maximum number of results to return
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Add a folder to the central document library
    AddFolder { path: PathBuf },
    /// List registered document folders
    ListFolders,
    /// Remove a folder from the central document library
    RemoveFolder { id: String },
    /// Open a file with the default app
    Open { path: PathBuf },
    /// Reveal a file in Finder
    Reveal { path: PathBuf },
    /// Optimize the local edge shard
    Optimize,
    /// Start the HTTP daemon for REST, eval scripts, and multi-repo access
    Serve,
    /// Start the MCP stdio tool server for editors and AI agents
    Mcp,
    /// Watch files and auto-reindex
    Watch,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => init_repo(),
        Commands::Serve => serve().await,
        Commands::Mcp => run_mcp().await,
        Commands::Index { path } => {
            let config = load_command_config(&cli.config, path.as_deref())?;
            if config.index.kind == "document" {
                let target_path = path.unwrap_or_else(|| PathBuf::from(&config.repo.path));
                index_folder(&target_path, config).await?;
                Ok(())
            } else {
                index_repo(config).await
            }
        }
        Commands::Search { query, path, limit } => {
            let config = load_command_config(&cli.config, path.as_deref())?;
            if config.index.kind == "document" && path.is_none() {
                for result in search_documents(&query, None, limit, config).await? {
                    println!(
                        "{}\n  Title: {}\n  Section: {}\n  Page: {}\n  Score: {:.3}\n  Preview: {}\n",
                        result.file_path,
                        result.title,
                        result.section,
                        result.page,
                        result.score,
                        result.preview
                    );
                }
            } else {
                println!("{}", search_repo(config, &query, limit).await?);
            }
            Ok(())
        }
        Commands::AddFolder { path } => {
            let record = add_folder(&path)?;
            println!("Added folder '{}' ({})", record.display_name, record.id);
            Ok(())
        }
        Commands::ListFolders => {
            for folder in list_folders() {
                println!("{}  {}  {}", folder.id, folder.display_name, folder.path);
            }
            Ok(())
        }
        Commands::RemoveFolder { id } => {
            if remove_folder(&id)? {
                println!("Removed folder {}", id);
            } else {
                println!("Folder {} not found", id);
            }
            Ok(())
        }
        Commands::Open { path } => open_document(&path),
        Commands::Reveal { path } => reveal_in_finder(&path),
        Commands::Optimize => {
            let config = AppConfig::load(cli.config.to_str().unwrap())?;
            optimize_repo(config).await
        }
        Commands::Watch => {
            let config = AppConfig::load(cli.config.to_str().unwrap())?;
            watch(config).await
        }
    }
}

fn load_command_config(
    config_path: &Path,
    path_override: Option<&Path>,
) -> anyhow::Result<AppConfig> {
    let mut config = if config_path.exists() {
        AppConfig::load(config_path.to_str().unwrap())?
    } else if let Some(path) = path_override {
        default_document_config(path)
    } else {
        return Err(anyhow::anyhow!(
            "config file '{}' not found",
            config_path.display()
        ));
    };

    if let Some(path) = path_override {
        config.repo.path = path.to_string_lossy().to_string();
    }

    normalize_document_storage(&mut config);

    Ok(config)
}

async fn search_repo(config: AppConfig, query: &str, limit: usize) -> anyhow::Result<String> {
    let embedder = build_embedder(&config.embedder)?;
    let repo_path = std::fs::canonicalize(&config.repo.path)?;
    let store = EdgeStore::new(
        resolved_store_path(&config, &repo_path),
        &config.store.vector_name,
    );
    let raw_results = search_chunks(
        embedder.as_ref(),
        &store,
        query,
        if config.index.kind == "code" {
            limit * 3
        } else {
            limit
        },
        &HashMap::new(),
    )
    .await?;

    match config.index.kind.as_str() {
        "code" => {
            let graph = Arc::new(Graph::new());
            let graph_path = repo_path.join(".compas").join("graph.json");
            if let Err(error) = graph.load(&graph_path) {
                debug!("no existing graph to load for search: {}", error);
            }

            let results = rerank_code_results(graph.as_ref(), raw_results, query, limit);
            Ok(format_code_search_results(&results, &repo_path))
        }
        "document" => {
            let results = raw_results
                .into_iter()
                .map(DocumentSearchResult::try_from)
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(format_document_search_results(&results, &repo_path))
        }
        other => Err(anyhow::anyhow!(
            "unsupported index.kind '{}' ; expected 'code' or 'document'",
            other
        )),
    }
}

fn format_code_search_results(results: &[CodeSearchResult], repo_path: &Path) -> String {
    if results.is_empty() {
        return "No relevant code found.".to_string();
    }

    let mut lines = vec![format!("Found {} relevant code result(s):", results.len())];
    for (index, result) in results.iter().enumerate() {
        let rel_path = Path::new(&result.chunk.file_path)
            .strip_prefix(repo_path)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|_| result.chunk.file_path.clone());
        let preview: String = result.chunk.content.chars().take(240).collect();

        lines.push(format!(
            "\n{}. {}:{}-{}\n   Symbol: {}\n   Score: {:.3}\n   Preview: {}",
            index + 1,
            rel_path,
            result.chunk.line_start,
            result.chunk.line_end,
            result.chunk.symbol,
            result.score,
            preview
        ));
    }

    lines.join("\n")
}

fn format_document_search_results(results: &[DocumentSearchResult], repo_path: &Path) -> String {
    if results.is_empty() {
        return "No relevant documents found.".to_string();
    }

    let mut lines = vec![format!(
        "Found {} relevant document result(s):",
        results.len()
    )];
    for (index, result) in results.iter().enumerate() {
        let rel_path = Path::new(&result.chunk.file_path)
            .strip_prefix(repo_path)
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|_| result.chunk.file_path.clone());
        let section = if result.chunk.heading_path.is_empty() {
            "(root)".to_string()
        } else {
            result.chunk.heading_path.join(" > ")
        };

        lines.push(format!(
            "\n{}. {}\n   Title: {}\n   Section: {}\n   Page: {}\n   Score: {:.3}\n   Preview: {}",
            index + 1,
            rel_path,
            result.chunk.title,
            section,
            display_page_range(result.chunk.page_start, result.chunk.page_end),
            result.score,
            result.chunk.preview
        ));
    }

    lines.join("\n")
}

fn display_page_range(page_start: Option<usize>, page_end: Option<usize>) -> String {
    match (page_start, page_end) {
        (Some(start), Some(end)) if start != end => format!("{}-{}", start, end),
        (Some(start), _) => start.to_string(),
        _ => "n/a".to_string(),
    }
}

fn init_repo() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let repo_name = cwd.file_name().unwrap_or_default().to_string_lossy();
    let config_path = cwd.join("compas.yaml");

    if config_path.exists() {
        println!("compas.yaml already exists. Delete it first if you want to regenerate.");
        return Ok(());
    }

    // Detect dominant language
    let mut counts: HashMap<String, usize> = HashMap::new();
    for entry in walkdir::WalkDir::new(&cwd).max_depth(3) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        // Skip dependency directories that can skew counts
        let path_str = path.to_string_lossy();
        if path_str.contains("/node_modules/")
            || path_str.contains("/.dart_tool/")
            || path_str.contains("/target/")
            || path_str.contains("/.venv/")
            || path_str.contains("/venv/")
            || path_str.contains("/vendor/")
        {
            continue;
        }
        if let Some(ext) = path.extension() {
            let ext = ext.to_string_lossy().to_string();
            if matches!(
                ext.as_str(),
                "dart" | "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java"
            ) {
                *counts.entry(ext).or_insert(0) += 1;
            }
        }
    }

    let mut dominant = counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .map(|(e, _)| e.as_str());

    // If this is a Flutter project (has pubspec.yaml), prefer Dart even if
    // JS/TS files outnumber it (e.g. node_modules-like contamination).
    let has_pubspec = cwd.join("pubspec.yaml").exists() || cwd.join("pubspec.yml").exists();
    if has_pubspec && counts.get("dart").copied().unwrap_or(0) > 0 {
        dominant = Some("dart");
    }

    let (include, exclude, compasignore_lines) = match dominant {
        Some("dart") => (
            vec!["lib/**/*.dart", "test/**/*.dart"],
            vec!["**/*.g.dart", "build/**", ".dart_tool/**"],
            vec![
                "# Flutter/Dart autogenerated files",
                "**/*.g.dart",
                "**/*.freezed.dart",
                "**/*.gr.dart",
                "**/app_localizations*.dart",
                "",
                "# Build artifacts",
                ".dart_tool/",
                "build/",
                "**/generated/",
            ],
        ),
        Some("ts") | Some("tsx") => (
            vec!["src/**/*.{ts,tsx}", "lib/**/*.{ts,tsx}"],
            vec!["node_modules/**", "dist/**", "build/**"],
            vec![
                "# TypeScript autogenerated files",
                "**/*.d.ts",
                "",
                "# Build artifacts",
                "node_modules/",
                "dist/",
                "build/",
            ],
        ),
        Some("rs") => (
            vec!["src/**/*.rs", "crates/**/*.rs"],
            vec!["target/**"],
            vec!["# Rust build artifacts", "target/", "Cargo.lock"],
        ),
        Some("py") => (
            vec!["**/*.py"],
            vec!["venv/**", ".venv/**", "__pycache__/**"],
            vec![
                "# Python cache and environments",
                "__pycache__/",
                "*.pyc",
                ".pytest_cache/",
                ".venv/",
                "venv/",
            ],
        ),
        Some("go") => (
            vec!["**/*.go"],
            vec!["vendor/**"],
            vec!["# Go vendor and generated", "vendor/", "*.gen.go"],
        ),
        _ => (
            vec!["src/**/*"],
            vec!["build/**", "dist/**"],
            vec!["# Build artifacts", "build/", "dist/"],
        ),
    };

    let yaml = format!(
        r#"repo:
  path: .
  include:
{}
  exclude:
{}

embedder:
  provider: fastembed
  model: nomic-ai/nomic-embed-text-v1.5

store:
  provider: edge
  path: .compas/edge-shard
  vector_name: default

server:
  host: 127.0.0.1
  port: "3001"

index:
  chunk_by: function
  watch: true
"#,
        include
            .iter()
            .map(|s| format!("    - \"{}\"", s))
            .collect::<Vec<_>>()
            .join("\n"),
        exclude
            .iter()
            .map(|s| format!("    - \"{}\"", s))
            .collect::<Vec<_>>()
            .join("\n"),
    );

    std::fs::write(&config_path, yaml)?;

    // Write .compasignore
    let ignore_path = cwd.join(".compasignore");
    if !ignore_path.exists() {
        let ignore_content = compasignore_lines.join("\n") + "\n";
        std::fs::write(&ignore_path, ignore_content)?;
        println!("Created .compasignore in {:?}", cwd);
    } else {
        println!(".compasignore already exists. Skipping.");
    }

    // Register repo in global registry
    let mut registry = compas::config::RepoRegistry::load();
    let repo_name = repo_name.to_lowercase().replace(' ', "_");
    registry.register(&repo_name, &cwd);
    registry.save()?;
    println!("Registered repo '{}' in global registry", repo_name);

    // Write AGENTS.md so agents know to use compas
    let agents_md = r#"# AGENTS.md — Navigation Rules

## MANDATORY RULE

For ANY task where you do not already know the exact file path and line number, your FIRST action MUST be `search_codebase`.

NEVER use `list_dir`, `read_file`, or regex search for initial exploration.
NEVER browse the directory tree to get oriented before searching.
NEVER assume you know where code lives because of file names or folder structure.

## Compas (Local Semantic Search)

This repo is indexed by compas. It finds symbols by natural language meaning and knows the call graph. It is faster and more accurate than manual browsing.

### Tools

| Tool | Use When |
|------|----------|
| `search_codebase` | ALWAYS FIRST. Any time you need to locate, understand, or explore code. |
| `get_symbol_graph` | After search, when you need to trace callers/callees of a specific symbol. |

### Correct Workflow

1. Search: `search_codebase({ query: "...", limit: 10 })`
2. Deepen (optional): `get_symbol_graph({ symbol: "..." })`
3. Read: Open ONLY the exact file(s) and line ranges compas returned

### What NOT to do

WRONG: Reading `lib/foo/bar.dart` because "the logic is probably there."
RIGHT: `search_codebase({ query: "how does X work", limit: 10 })` then read only the confirmed results.

---

Generated by `compas init`. Update or remove this file as needed.
"#;

    let agents_path = cwd.join("AGENTS.md");
    if !agents_path.exists() {
        std::fs::write(&agents_path, agents_md)?;
        println!("Created AGENTS.md in {:?}", cwd);
    } else {
        println!("AGENTS.md already exists. Skipping.");
    }

    // Write compas skill and instructions into .github/ if the folder exists.
    // These are always overwritten so they stay in sync with the latest compas version.
    let github_dir = cwd.join(".github");
    if github_dir.exists() {
        let skill_md = r#"---
name: compas
description: "Codebase semantic search and symbol graph navigation. Use BEFORE opening any file you are not 100% certain about. Triggers on uncertainty: 'I need to find', 'where is', 'which file', 'I'm not sure where', 'let me check', 'I think it's in', 'probably in', 'looks like'. Enforces: query compas first, open only what compas confirms."
---

## MANDATORY RULE

For ANY task where you do not already know the exact file path and line number, your FIRST action MUST be `search_codebase`.

NEVER use `list_dir`, `read_file`, or regex search for initial exploration.
NEVER browse the directory tree to get oriented before searching.
NEVER assume you know where code lives because of file names or folder structure.

## ALWAYS Pass the repo Parameter

This is the most common failure mode. ALWAYS include `repo` in every `search_codebase` and `get_symbol_graph` call.

The MCP server cannot reliably auto-detect which repo you are in because some editors spawn MCP from internal directories, not the workspace root.

If you forget `repo`, you get:

    Error: missing 'repo' parameter and could not auto-detect from cwd.

## Tool: search_codebase

Parameters:
- query (required): Natural language. Describe what you want, not regex.
  Good: "user authentication with password hashing"
  Bad: "class.*Auth" — compas is semantic, not regex
  Bad: "getUserById" — use `get_symbol_graph` for exact symbols
- repo (required): ALWAYS pass this.
- limit (optional): Default 10. Use 15-20 for exploration.
- language (optional): Filter by language, e.g. "dart".

After receiving results:
1. Read the top 3-5 previews
2. Pick the most relevant symbol by score and name
3. Open ONLY that file
4. Do NOT open files that did not appear in results

## Tool: get_symbol_graph

Use AFTER finding a relevant symbol to trace its call chain.

Parameters:
- symbol (required): Symbol name, e.g. "AuthService.authenticate"
- file (optional): File path to disambiguate
- repo (required): ALWAYS pass this

## Two-Step Workflow

Step 1 — Search:
`search_codebase({ query: "how does X work", repo: "...", limit: 15 })`

Step 2 — Deepen (optional):
`get_symbol_graph({ symbol: "X", repo: "..." })`

Step 3 — Read:
Open the most relevant file confirmed by both tools.

## Pitfalls

| Pitfall | Fix |
|---------|-----|
| Forgetting repo parameter | ALWAYS pass repo in every call |
| Opening files based on assumptions | ALWAYS search compas first |
| Using exact symbol names in search_codebase | Use natural language; use get_symbol_graph for exact symbols |
| Skipping the graph | After finding a symbol, check get_symbol_graph for "how does it work?" |

Generated by compas init. Update or remove this file as needed.
"#;

        let skills_dir = github_dir.join("skills").join("compas");
        std::fs::create_dir_all(&skills_dir)?;
        let skill_path = skills_dir.join("SKILL.md");
        std::fs::write(&skill_path, skill_md)?;
        println!("Created .github/skills/compas/SKILL.md");

        let instructions_dir = github_dir.join("instructions");
        std::fs::create_dir_all(&instructions_dir)?;
        let instruction_path = instructions_dir.join("repo-navigation.instructions.md");
        let instructions_md = r#"---
name: repo-navigation
description: "Use when exploring, locating, or understanding code in this repository. Enforces search_codebase as the mandatory first action before any file read, directory listing, or regex search. Covers compas tool usage, correct workflow order, and anti-patterns to avoid."
applyTo: "**"
---

## MANDATORY RULE

For ANY task where you do not already know the exact file path and line number, your FIRST action MUST be `search_codebase`.

NEVER use `list_dir`, `read_file`, or regex search for initial exploration.
NEVER browse the directory tree to get oriented before searching.
NEVER assume you know where code lives because of file names or folder structure.

## Compas (Local Semantic Search)

This repo is indexed by compas. It finds symbols by natural language meaning and knows the call graph. It is faster and more accurate than manual browsing.

### Tools

| Tool | Use When |
|------|----------|
| `search_codebase` | ALWAYS FIRST. Any time you need to locate, understand, or explore code. |
| `get_symbol_graph` | After search, when you need to trace callers/callees of a specific symbol. |

### Correct Workflow

1. Search: `search_codebase({ query: "...", limit: 10 })`
2. Deepen (optional): `get_symbol_graph({ symbol: "..." })`
3. Read: Open ONLY the exact file(s) and line ranges compas returned

### What NOT to do

WRONG: Reading `lib/foo/bar.dart` because "the logic is probably there."
RIGHT: `search_codebase({ query: "how does X work", limit: 10 })` then read only the confirmed results.
"#;
        std::fs::write(&instruction_path, instructions_md)?;
        println!("Created .github/instructions/repo-navigation.instructions.md");
    }

    println!("Created compas.yaml in {:?}", cwd);
    println!("Detected language: {}", dominant.unwrap_or("unknown"));
    println!("\nNext step:");
    println!("  Index repo:  compas index");
    println!("\nThe embedding model downloads once on first index and is cached globally.");
    println!("\nTo make 'compas' available everywhere, copy the binary to your PATH:");
    println!("  cp /path/to/compas/target/release/compas /usr/local/bin/");

    Ok(())
}

fn is_flutter_boilerplate(symbol: &str) -> bool {
    let method_name = symbol.rsplit('.').next().unwrap_or(symbol);
    FLUTTER_LIFECYCLE_METHODS.contains(&method_name)
}

type MissingDocEntry = (String, String, String, usize);

struct PreparedCodeFile {
    language: &'static str,
    content: String,
    chunks: Vec<CodeChunk>,
    indexed_chunks: Vec<IndexedChunk>,
}

struct CodeIndexAdapter {
    repo_path: PathBuf,
    graph_path: PathBuf,
    graph: Arc<Graph>,
    registry: ChunkerRegistry,
    persist_graph: bool,
    missing_docs: Mutex<Vec<MissingDocEntry>>,
}

impl CodeIndexAdapter {
    fn new(repo_path: PathBuf, graph: Arc<Graph>, persist_graph: bool) -> Self {
        let graph_path = repo_path.join(".compas").join("graph.json");

        Self {
            repo_path,
            graph_path,
            graph,
            registry: ChunkerRegistry::new(),
            persist_graph,
            missing_docs: Mutex::new(Vec::new()),
        }
    }

    fn missing_docs(&self) -> Vec<MissingDocEntry> {
        self.missing_docs.lock().unwrap().clone()
    }

    fn record_missing_docs(&self, file_path: &str, chunks: &[CodeChunk]) {
        let display_path = relative_display_path(&self.repo_path, Path::new(file_path));
        self.missing_docs
            .lock()
            .unwrap()
            .extend(missing_docs_from_chunks(&display_path, chunks));
    }

    fn persist_graph_if_needed(&self) -> anyhow::Result<()> {
        if !self.persist_graph {
            return Ok(());
        }

        if let Some(parent) = self.graph_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.graph.save(&self.graph_path)
    }
}

impl IndexingAdapter for CodeIndexAdapter {
    type PreparedFile = PreparedCodeFile;

    fn supports_path(&self, path: &Path) -> bool {
        language_for_path(path).is_some()
    }

    fn prepare_file(
        &self,
        path: &Path,
        bytes: &[u8],
    ) -> anyhow::Result<Option<Self::PreparedFile>> {
        let Some(language) = language_for_path(path) else {
            return Ok(None);
        };
        let file_path = path.to_string_lossy().to_string();
        let content = std::str::from_utf8(bytes)
            .map_err(|e| anyhow::anyhow!("failed to decode source file as UTF-8: {}", e))?
            .to_string();

        let chunker = self
            .registry
            .get(language)
            .ok_or_else(|| anyhow::anyhow!("no {} chunker available", language))?;
        let chunks = chunker.chunk(&file_path, &content)?;

        for chunk in &chunks {
            let chunk_lines = chunk.line_end.saturating_sub(chunk.line_start);
            if chunk_lines > 200 {
                warn!(
                    "  long {}: {} ({} lines)",
                    chunk.kind, chunk.symbol, chunk_lines
                );
            }
        }

        let indexed_chunks = chunks.iter().cloned().map(IndexedChunk::from).collect();

        Ok(Some(PreparedCodeFile {
            language,
            content,
            chunks,
            indexed_chunks,
        }))
    }

    fn indexed_chunks<'a>(&self, prepared: &'a Self::PreparedFile) -> &'a [IndexedChunk] {
        &prepared.indexed_chunks
    }

    fn after_upsert(&self, file_path: &str, prepared: &Self::PreparedFile) -> anyhow::Result<()> {
        self.graph.remove_by_file(file_path);

        for chunk in &prepared.chunks {
            let base_symbol = strip_part_suffix(&chunk.symbol);
            self.graph
                .add_symbol(&base_symbol, &chunk.file_path, &chunk.kind);
        }

        if let Ok(calls) = extract_calls_for_language(prepared.language, &prepared.content) {
            for (caller, callee) in &calls {
                self.graph.add_symbol(caller, file_path, "method");
                self.graph.add_call(caller, file_path, callee);
            }
        }

        self.record_missing_docs(file_path, &prepared.chunks);
        self.persist_graph_if_needed()
    }

    fn after_delete(&self, file_path: &str) -> anyhow::Result<()> {
        self.graph.remove_by_file(file_path);
        self.persist_graph_if_needed()
    }
}

fn missing_docs_from_chunks(display_file: &str, chunks: &[CodeChunk]) -> Vec<MissingDocEntry> {
    chunks
        .iter()
        .filter(|chunk| {
            !chunk.content.starts_with("///")
                && !chunk.content.starts_with("//!")
                && (chunk.kind == "method"
                    || chunk.kind == "function"
                    || chunk.kind == "constructor")
                && !is_flutter_boilerplate(&chunk.symbol)
        })
        .map(|chunk| {
            (
                display_file.to_string(),
                chunk.symbol.clone(),
                chunk.kind.clone(),
                chunk.line_start,
            )
        })
        .collect()
}

fn relative_display_path(repo_path: &Path, file: &Path) -> String {
    file.strip_prefix(repo_path)
        .unwrap_or(file)
        .to_string_lossy()
        .to_string()
}

fn audit_path_filename(file_path: &str) -> String {
    Path::new(file_path)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string())
}

fn normalize_relative_uri(base_file: &Path, uri: &str) -> String {
    base_file
        .parent()
        .unwrap_or(base_file)
        .join(uri)
        .components()
        .collect::<std::path::PathBuf>()
        .to_string_lossy()
        .to_string()
}

fn package_name(repo_path: &Path) -> Option<String> {
    let pubspec_path = repo_path.join("pubspec.yaml");
    let content = std::fs::read_to_string(pubspec_path).ok()?;
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("name:")
            .map(|name| name.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|name| !name.is_empty())
    })
}

fn normalize_import_uri(
    repo_path: &Path,
    base_file: &Path,
    uri: &str,
    package_name: Option<&str>,
) -> Option<String> {
    if uri.starts_with("dart:") {
        return None;
    }
    if let Some(rest) = uri.strip_prefix("package:") {
        let (package, relative) = rest.split_once('/')?;
        if Some(package) != package_name {
            return None;
        }
        return Some(
            repo_path
                .join("lib")
                .join(relative)
                .to_string_lossy()
                .to_string(),
        );
    }
    Some(
        repo_path
            .join(normalize_relative_uri(base_file, uri))
            .to_string_lossy()
            .to_string(),
    )
}

fn build_audit_analysis(
    repo_path: &Path,
    manifest: &std::collections::HashMap<String, String>,
) -> AuditFileAnalysis {
    let mut analysis = AuditFileAnalysis::default();
    let mut seen_decls = HashSet::new();
    let repo_package_name = package_name(repo_path);

    for path_str in manifest.keys() {
        let file_path = Path::new(path_str);
        let content = match std::fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let display_file = relative_display_path(repo_path, file_path);

        let chunker = compas::chunker::dart::DartChunker;
        let chunks = match compas::chunker::Chunker::chunk(&chunker, path_str, &content) {
            Ok(chunks) => chunks,
            Err(_) => continue,
        };

        for chunk in chunks {
            let symbol = strip_part_suffix(&chunk.symbol);
            let is_filename_placeholder =
                chunk.kind == "declaration" && symbol == audit_path_filename(path_str);
            if chunk.kind == "file" || symbol.contains("unknown") || is_filename_placeholder {
                continue;
            }
            if seen_decls.insert(format!("{}:{}:{}", path_str, symbol, chunk.kind)) {
                analysis.declarations.push(AuditDeclaration {
                    file: path_str.clone(),
                    display_file: display_file.clone(),
                    symbol,
                    kind: chunk.kind,
                });
            }
        }

        if let Ok(semantic) = extract_semantic_references(&content) {
            analysis
                .references
                .extend(
                    semantic
                        .references
                        .into_iter()
                        .map(|(_, callee)| AuditReference {
                            caller_file: path_str.clone(),
                            callee,
                        }),
                );
            analysis.key_types.extend(semantic.key_types);
            analysis
                .file_edges
                .extend(semantic.import_uris.into_iter().filter_map(|uri| {
                    normalize_import_uri(repo_path, file_path, &uri, repo_package_name.as_deref())
                        .map(|target| (path_str.clone(), target))
                }));
        }
    }

    analysis.key_types.sort();
    analysis.key_types.dedup();
    analysis.file_edges.sort();
    analysis.file_edges.dedup();
    analysis
}

#[allow(dead_code)]
fn reachable_files(
    manifest: &std::collections::HashMap<String, String>,
    analysis: &AuditFileAnalysis,
) -> HashSet<String> {
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    for path in manifest.keys() {
        adjacency.entry(path.clone()).or_default();
    }
    for reference in &analysis.references {
        if let Some(target) = resolve_reference_file(&reference.callee, &analysis.declarations) {
            adjacency
                .entry(reference.caller_file.clone())
                .or_default()
                .push(target);
        }
    }
    for (source, target) in &analysis.file_edges {
        adjacency
            .entry(source.clone())
            .or_default()
            .push(target.clone());
        adjacency.entry(target.clone()).or_default();
    }

    let mut inbound_counts: HashMap<String, usize> = HashMap::new();
    for (source, targets) in &adjacency {
        inbound_counts.entry(source.clone()).or_insert(0);
        for target in targets {
            *inbound_counts.entry(target.clone()).or_insert(0) += 1;
        }
    }

    let mut queue = VecDeque::new();
    let mut reachable = HashSet::new();
    for path in manifest.keys() {
        if inbound_counts.get(path).copied().unwrap_or(0) == 0 {
            queue.push_back(path.clone());
        }
    }

    while let Some(path) = queue.pop_front() {
        if !reachable.insert(path.clone()) {
            continue;
        }
        if let Some(targets) = adjacency.get(&path) {
            for target in targets {
                if manifest.contains_key(target) {
                    queue.push_back(target.clone());
                }
            }
        }
    }

    reachable
}

#[allow(dead_code)]
fn resolve_reference_file(callee: &str, declarations: &[AuditDeclaration]) -> Option<String> {
    declarations
        .iter()
        .find(|decl| decl.symbol == callee || decl.symbol.rsplit('.').next() == Some(callee))
        .map(|decl| decl.file.clone())
}

fn classify_dead_code_candidates(analysis: &AuditFileAnalysis) -> Vec<DeadCodeCandidate> {
    // Build a global bare-name set of every callee referenced anywhere.
    // If a name appears in any reference, every declaration with that bare name
    // is considered live. This is intentionally permissive: false negatives
    // (missing a real dead method that shares a name with a live one) are far
    // less costly than the false positives we get from precise matching when
    // Dart's dynamic dispatch hides receiver types.
    let mut referenced_names: HashSet<String> = HashSet::new();
    for reference in &analysis.references {
        // Record both the full callee and its last segment.
        referenced_names.insert(reference.callee.clone());
        if let Some(bare) = reference.callee.rsplit('.').next() {
            referenced_names.insert(bare.to_string());
        }
    }

    let key_types: HashSet<String> = analysis.key_types.iter().cloned().collect();
    let mut candidates = vec![];

    for declaration in &analysis.declarations {
        // Skip declaration-level wrappers and types — we only care about callable members.
        if matches!(
            declaration.kind.as_str(),
            "class" | "mixin" | "extension" | "enum" | "file" | "typedef"
        ) {
            continue;
        }

        // Skip filename-placeholder declarations from the chunker.
        if declaration.kind == "declaration"
            && declaration.symbol == audit_path_filename(&declaration.file)
        {
            continue;
        }
        if declaration.symbol.contains("unknown") {
            continue;
        }

        let member_name = declaration
            .symbol
            .rsplit('.')
            .next()
            .unwrap_or(&declaration.symbol);
        let enclosing_type = declaration
            .symbol
            .split('.')
            .next()
            .unwrap_or(&declaration.symbol);

        // Hard suppressions: framework dispatch.
        if is_framework_dispatched(member_name) {
            continue;
        }

        // operator == / hashCode are framework-dispatched anyway, but as a belt-and-braces
        // also suppress them whenever the enclosing class is used as a Map/Set key.
        if (member_name == "operator ==" || member_name == "hashCode")
            && key_types.contains(enclosing_type)
        {
            continue;
        }

        // Skip private members entirely. They are by definition internal to the file
        // and the audit produces too much noise on `_buildRow`-style helpers.
        if member_name.starts_with('_') {
            continue;
        }

        // Skip getters and setters. They are read via property syntax that we cannot
        // reliably distinguish from random identifier reads, so the signal is weak.
        if declaration.kind == "getter_setter" {
            continue;
        }

        // Skip constructors named like codegen entrypoints (fromJson, fromMap, etc.).
        // These are matched above in is_framework_dispatched, but the constructor symbol
        // is `Class.fromJson` so check the member_name explicitly too.
        if matches!(declaration.kind.as_str(), "constructor")
            && matches!(
                member_name,
                "fromJson" | "fromMap" | "fromSnapshot" | "fromDoc" | "fromDocument" | "fromString"
            )
        {
            continue;
        }

        // Skip the unnamed default constructor (symbol like `Class.Class`) when the class
        // itself is referenced anywhere by name. A class being instantiated (`Foo()`)
        // shows up in references as the bare name `Foo`.
        if declaration.kind == "constructor"
            && member_name == enclosing_type
            && referenced_names.contains(enclosing_type)
        {
            continue;
        }

        // Skip Flutter callback-style members (onPressed, onTap, etc.) — these are passed
        // as widget parameters and dispatched by the framework.
        if FLUTTER_CALLBACK_SUFFIXES.contains(&member_name) {
            continue;
        }

        // Final liveness check: is the bare name referenced anywhere?
        if referenced_names.contains(member_name) {
            continue;
        }

        candidates.push(DeadCodeCandidate {
            file: declaration.display_file.clone(),
            symbol: declaration.symbol.clone(),
            kind: declaration.kind.clone(),
        });
    }

    candidates.sort_by(|a, b| a.file.cmp(&b.file).then(a.symbol.cmp(&b.symbol)));
    candidates
}

async fn index_repo(config: AppConfig) -> anyhow::Result<()> {
    let embedder = build_embedder(&config.embedder)?;
    let repo_path = std::fs::canonicalize(&config.repo.path)?;
    let store = Arc::new(EdgeStore::new(
        resolved_store_path(&config, &repo_path),
        &config.store.vector_name,
    ));
    store.init(embedder.dimensions()).await?;

    if config.index.kind == "document" {
        let storage = document_storage_paths(&repo_path);
        let adapter = DocumentIndexAdapter::new();
        let indexer = Indexer::new(
            &repo_path,
            &config.repo.include,
            &config.repo.exclude,
            store.as_ref(),
            embedder.as_ref(),
        )
        .with_manifest_path(&storage.manifest_path);
        let report = indexer.index_repo(&adapter).await?;

        print_index_summary(&repo_path, &report, None, None);

        println!("Optimizing edge shard...");
        let optimized = store.optimize()?;
        if optimized {
            println!("✓ Edge shard optimized");
        } else {
            println!("✓ Edge shard already optimized");
        }

        return Ok(());
    }
    if config.index.kind != "code" {
        return Err(anyhow::anyhow!(
            "unsupported index.kind '{}' ; expected 'code' or 'document'",
            config.index.kind
        ));
    }

    let graph = Arc::new(Graph::new());
    let graph_path = repo_path.join(".compas").join("graph.json");
    if let Err(e) = graph.load(&graph_path) {
        debug!("no existing graph to load: {}", e);
    }

    let adapter = CodeIndexAdapter::new(repo_path.clone(), Arc::clone(&graph), false);
    let indexer = Indexer::new(
        &repo_path,
        &config.repo.include,
        &config.repo.exclude,
        store.as_ref(),
        embedder.as_ref(),
    );
    let report = indexer.index_repo(&adapter).await?;

    let mut chunks_without_docs = adapter.missing_docs();
    chunks_without_docs.extend(scan_missing_docs_for_manifest(
        &repo_path,
        &report.manifest,
        &report.processed_paths,
    ));

    graph.create_phantom_nodes();
    if !report.used_tui {
        info!("created phantom nodes for external symbols");
    }

    tokio::fs::create_dir_all(graph_path.parent().unwrap())
        .await
        .ok();
    graph.save(&graph_path)?;

    let dart_manifest: std::collections::HashMap<String, String> = report
        .manifest
        .iter()
        .filter(|(path, _)| language_for_path(Path::new(path)) == Some("dart"))
        .map(|(path, hash)| (path.clone(), hash.clone()))
        .collect();

    let audit_analysis = build_audit_analysis(&repo_path, &dart_manifest);
    let dead_code_candidates = classify_dead_code_candidates(&audit_analysis);

    generate_audit(
        graph.as_ref(),
        &dead_code_candidates,
        &chunks_without_docs,
        report.processed_files,
        report.total_chunks,
        report.failed_files,
    );

    print_index_summary(
        &repo_path,
        &report,
        Some(chunks_without_docs.len()),
        Some(dead_code_candidates.len()),
    );

    println!("Optimizing edge shard...");
    let optimized = store.optimize()?;
    if optimized {
        println!("✓ Edge shard optimized");
    } else {
        println!("✓ Edge shard already optimized");
    }

    Ok(())
}

fn print_index_summary(
    repo_path: &Path,
    report: &compas::indexing::IndexingReport,
    missing_docs: Option<usize>,
    dead_code_count: Option<usize>,
) {
    let secs = report.elapsed.as_secs();
    let mins = secs / 60;
    let rem_secs = secs % 60;
    let time_str = if mins > 0 {
        format!("{}m {:02}s", mins, rem_secs)
    } else {
        format!("{}s", rem_secs)
    };

    let repo_name = repo_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());

    println!();
    println!("     @@@@@@@   @@@@@@   @@@@@@@@@@   @@@@@@@    @@@@@@    @@@@@@   ");
    println!("     @@@@@@@@  @@@@@@@@  @@@@@@@@@@@  @@@@@@@@  @@@@@@@@  @@@@@@@   ");
    println!("     !@@       @@!  @@@  @@! @@! @@!  @@!  @@@  @@!  @@@  !@@       ");
    println!("     !@!       !@!  @!@  !@! !@! !@!  !@!  @!@  !@!  @!@  !@!       ");
    println!("     !@!       @!@  !@!  @!! !!@ @!@  @!@@!@!   @!@!@!@!  !!@@!!    ");
    println!("     !!!       !@!  !!!  !@!   ! !@!  !!@!!!    !!!@!!!!   !!@!!!   ");
    println!("     :!!       !!:  !!!  !!:     !!:  !!:       !!:  !!!       !:!  ");
    println!("     :!:       :!:  !:!  :!:     :!:  :!:       :!:  !:!      !:!   ");
    println!("      ::: :::  ::::: ::  :::     ::    ::       ::   :::  :::: ::   ");
    println!("      :: :: :   : :  :    :      :     :         :   : :  :: : :    ");
    println!();
    println!("    {} indexed in {}", repo_name, time_str);
    println!();
    println!("    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!(
        "     {} changed  ·  {} skipped  ·  {} deleted",
        report.processed_files, report.skipped_files, report.deleted_files
    );
    println!(
        "     {} chunks  ·  {} failed",
        report.total_chunks, report.failed_files
    );
    println!("    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if let (Some(missing_docs), Some(dead_code_count)) = (missing_docs, dead_code_count) {
        let graph_path = repo_path.join(".compas").join("graph.json");
        println!();
        println!("    ⚠️  {} symbols missing doc comments", missing_docs);
        println!("    🪦  {} dead code candidates", dead_code_count);
        println!();
        println!("    📊 Graph  → {}", graph_path.display());
        println!("    📋 Audit  → .compas/audit.md");
        println!();
    }

    if !report.used_tui {
        info!("indexing complete");
    }
}

async fn optimize_repo(config: AppConfig) -> anyhow::Result<()> {
    let repo_path = std::fs::canonicalize(&config.repo.path)?;
    let optimized = optimize_edge_shard(&config)?;

    if optimized {
        println!("Optimized edge shard for {}", repo_path.display());
    } else {
        println!(
            "Edge shard for {} did not require optimization",
            repo_path.display()
        );
    }

    Ok(())
}

fn optimize_edge_shard(config: &AppConfig) -> anyhow::Result<bool> {
    let repo_path = std::fs::canonicalize(&config.repo.path)?;
    let store = EdgeStore::new(
        resolved_store_path(config, &repo_path),
        &config.store.vector_name,
    );
    store.optimize()
}

async fn run_mcp() -> anyhow::Result<()> {
    let registry = compas::config::RepoRegistry::load();

    if registry.repos.is_empty() {
        println!("No repos registered. Run 'compas init' in a repository first.");
        return Ok(());
    }

    let mut repos = HashMap::new();
    let mut embedder_cache: HashMap<
        compas::config::EmbedderConfig,
        Arc<dyn compas::embedder::Embedder>,
    > = HashMap::new();
    for (name, path) in registry.list() {
        let config_path = std::path::Path::new(path).join("compas.yaml");
        if !config_path.exists() {
            warn!(
                "compas.yaml not found for repo '{}' at {}, skipping",
                name, path
            );
            continue;
        }

        let config = match AppConfig::load(config_path.to_str().unwrap()) {
            Ok(c) => c,
            Err(e) => {
                warn!("failed to load config for repo '{}': {}", name, e);
                continue;
            }
        };

        let repo_path = std::fs::canonicalize(path)?;
        let embedder = match embedder_cache.get(&config.embedder) {
            Some(e) => Arc::clone(e),
            None => {
                let e = build_embedder(&config.embedder)?;
                embedder_cache.insert(config.embedder.clone(), Arc::clone(&e));
                e
            }
        };
        let edge_store = Arc::new(EdgeStore::new(
            resolved_store_path(&config, &repo_path),
            &config.store.vector_name,
        ));
        // MCP startup only loads repo descriptors. The shard is opened on demand
        // when a tool call actually targets this repo.
        let store: Arc<dyn compas::store::Store> = edge_store;
        let code = if config.index.kind == "code" {
            let graph = Arc::new(Graph::new());
            let graph_path = repo_path.join(".compas").join("graph.json");
            if let Err(e) = graph.load(&graph_path) {
                warn!("no existing graph loaded for repo '{}': {}", name, e);
            }
            Some(CodeRuntime { graph })
        } else {
            None
        };

        repos.insert(
            name.clone(),
            compas::mcp::state::RepoState {
                store,
                code,
                embedder,
            },
        );
        info!("loaded repo '{}' for MCP", name);
    }

    if repos.is_empty() {
        println!("No valid repos could be loaded.");
        return Ok(());
    }

    let default_repo = if repos.len() == 1 {
        repos.keys().next().cloned()
    } else {
        None
    };

    let state = Arc::new(McpAppState {
        repos,
        default_repo,
    });

    mcp::server::run_stdio_server(state).await
}

async fn serve() -> anyhow::Result<()> {
    let registry = compas::config::RepoRegistry::load();

    if registry.repos.is_empty() {
        println!("No repos registered. Run 'compas init' in a repository first.");
        return Ok(());
    }

    let mut repos = HashMap::new();
    let mut embedder_cache: HashMap<
        compas::config::EmbedderConfig,
        Arc<dyn compas::embedder::Embedder>,
    > = HashMap::new();
    for (name, path) in registry.list() {
        let config_path = std::path::Path::new(path).join("compas.yaml");
        if !config_path.exists() {
            warn!(
                "compas.yaml not found for repo '{}' at {}, skipping",
                name, path
            );
            continue;
        }

        let config = match AppConfig::load(config_path.to_str().unwrap()) {
            Ok(c) => c,
            Err(e) => {
                warn!("failed to load config for repo '{}': {}", name, e);
                continue;
            }
        };

        let repo_path = std::fs::canonicalize(path)?;
        let embedder = match embedder_cache.get(&config.embedder) {
            Some(e) => Arc::clone(e),
            None => {
                let e = build_embedder(&config.embedder)?;
                embedder_cache.insert(config.embedder.clone(), Arc::clone(&e));
                e
            }
        };
        let edge_store = Arc::new(EdgeStore::new(
            resolved_store_path(&config, &repo_path),
            &config.store.vector_name,
        ));
        // Daemon startup only loads repo descriptors. The shard is opened on
        // demand when a request actually targets this repo.
        let store: Arc<dyn compas::store::Store> = edge_store;
        let code = if config.index.kind == "code" {
            let graph = Arc::new(Graph::new());
            let graph_path = repo_path.join(".compas").join("graph.json");
            if let Err(e) = graph.load(&graph_path) {
                warn!("no existing graph loaded for repo '{}': {}", name, e);
            }
            Some(CodeRuntime { graph })
        } else {
            None
        };

        repos.insert(
            name.clone(),
            RepoState {
                config,
                store,
                code,
                embedder,
            },
        );
        info!("loaded repo '{}' from {}", name, path);
    }

    if repos.is_empty() {
        println!("No valid repos could be loaded.");
        return Ok(());
    }

    let default_repo = if repos.len() == 1 {
        repos.keys().next().cloned()
    } else {
        None
    };

    let state = Arc::new(AppState {
        repos,
        default_repo,
    });

    // Use a fixed port for the global daemon (ignore per-repo config)
    let host = std::env::var("COMPAS_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("COMPAS_PORT").unwrap_or_else(|_| "3001".into());

    // Auto-restart: if port is in use, kill the existing compas process
    let addr = format!("{}:{}", host, port);
    if let Ok(output) = std::process::Command::new("lsof")
        .args(["-i", &format!(":{}", port), "-t"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for pid_str in stdout.lines() {
            if let Ok(pid) = pid_str.parse::<u32>() {
                let my_pid = std::process::id();
                if pid != my_pid {
                    println!("Port {} is in use by PID {}. Restarting...", port, pid);
                    let _ = std::process::Command::new("kill").arg(pid_str).output();
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    let app = router(state.clone());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!(
        "compas daemon listening on {} (serving {} repo(s))",
        listener.local_addr()?,
        state.repos.len()
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn watch(config: AppConfig) -> anyhow::Result<()> {
    if config.index.kind == "document" {
        return Err(anyhow::anyhow!(
            "document watch mode is not implemented yet"
        ));
    }

    let repo_path = std::fs::canonicalize(&config.repo.path)?;
    let embedder = build_embedder(&config.embedder)?;
    let edge_store = Arc::new(EdgeStore::new(
        resolved_store_path(&config, &repo_path),
        &config.store.vector_name,
    ));
    edge_store.init(embedder.dimensions()).await?;
    let store: Arc<dyn compas::store::Store> = edge_store;
    let embedder: Arc<dyn compas::embedder::Embedder> = embedder;
    let graph = Arc::new(Graph::new());
    let graph_path = repo_path.join(".compas").join("graph.json");
    if let Err(e) = graph.load(&graph_path) {
        debug!("no existing graph to load: {}", e);
    }

    let watch_path = repo_path.clone();
    let handler = ReindexHandler {
        config,
        repo_path,
        store,
        embedder,
        graph,
    };
    FileWatcher::watch(watch_path, handler).await
}

struct ReindexHandler {
    config: AppConfig,
    repo_path: PathBuf,
    store: Arc<dyn compas::store::Store>,
    embedder: Arc<dyn compas::embedder::Embedder>,
    graph: Arc<Graph>,
}

#[async_trait]
impl Handler for ReindexHandler {
    async fn on_change(&self, file_path: &str) {
        info!("reindexing {}", file_path);

        let adapter = CodeIndexAdapter::new(self.repo_path.clone(), Arc::clone(&self.graph), true);
        let indexer = Indexer::new(
            &self.repo_path,
            &self.config.repo.include,
            &self.config.repo.exclude,
            self.store.as_ref(),
            self.embedder.as_ref(),
        );

        match indexer.reindex_file(&adapter, file_path).await {
            Ok(Some(chunk_count)) => {
                if chunk_count == 0 {
                    info!("no chunks found in {}", file_path);
                } else {
                    info!("reindexed {} ({} chunks)", file_path, chunk_count);
                }
            }
            Ok(None) => {}
            Err(e) => warn!("reindex failed for {}: {}", file_path, e),
        }
    }

    async fn on_delete(&self, file_path: &str) {
        info!("deleting {}", file_path);

        let adapter = CodeIndexAdapter::new(self.repo_path.clone(), Arc::clone(&self.graph), true);
        let indexer = Indexer::new(
            &self.repo_path,
            &self.config.repo.include,
            &self.config.repo.exclude,
            self.store.as_ref(),
            self.embedder.as_ref(),
        );

        match indexer.delete_file(&adapter, file_path).await {
            Ok(true) => info!("deleted {}", file_path),
            Ok(false) => {}
            Err(e) => warn!("delete failed for {}: {}", file_path, e),
        }
    }
}

fn generate_audit(
    _graph: &Graph,
    dead_code: &[DeadCodeCandidate],
    missing_docs: &[(String, String, String, usize)],
    files: usize,
    chunks: usize,
    failed: usize,
) {
    // Deduplicate missing docs by base symbol (strip _pN suffix from split chunks).
    // A large function split into _p1, _p2, _p3 should only appear once in the audit.
    let mut seen_symbols: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut deduped_docs: Vec<(String, String, String, usize)> = vec![];
    for (file, symbol, kind, line) in missing_docs.iter() {
        let base = strip_part_suffix(symbol);
        if seen_symbols.insert(format!("{}:{}", file, base)) {
            deduped_docs.push((file.clone(), base, kind.clone(), *line));
        }
    }
    let mut missing_docs = deduped_docs;
    missing_docs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut md = String::new();
    md.push_str("# Compas Codebase Audit\n\n");
    md.push_str(&format!(
        "**Files indexed:** {} | **Chunks:** {} | **Failed:** {}\n\n",
        files, chunks, failed
    ));
    md.push_str(&format!(
        "**Symbols missing doc comments:** {} | **Dead code candidates:** {}\n\n",
        missing_docs.len(),
        dead_code.len()
    ));

    // Doc comments section — formatted as a self-contained AI agent prompt
    md.push_str("---\n\n");
    md.push_str("# PROMPT FOR YOUR AGENT: Add Dart Doc Comments\n\n");
    md.push_str(
        "Copy this entire section below to your AI agent. Do not edit the prompt itself.\n\n",
    );
    md.push_str("## Role\n\n");
    md.push_str("You are a **Dart code documentation specialist**. Your sole task is to add `///` doc comments to the functions listed below.\n\n");
    md.push_str("## Why This Matters\n\n");
    md.push_str("These functions are **invisible to semantic search** because they lack `///` doc comments.\n");
    md.push_str("This codebase is indexed by an AI search engine that embeds doc comments alongside code.\n");
    md.push_str("When a developer searches \"where is the login authentication logic?\", the engine matches\n");
    md.push_str("their query against these doc comments. **No comment = no match = the function might as well not exist.**\n\n");
    md.push_str("## How to Write Good Doc Comments\n\n");
    md.push_str(
        "Write **1-3 lines** maximum. Include keywords a developer would actually search for.\n\n",
    );
    md.push_str("**Examples:**\n");
    md.push_str("```dart\n");
    md.push_str(
        "/// Authenticates the user against the OAuth provider and stores the JWT token.\n",
    );
    md.push_str("/// Called during app startup and after token refresh.\n");
    md.push_str("Future<void> authenticateUser() async { ... }\n");
    md.push_str("```\n\n");
    md.push_str("```dart\n");
    md.push_str("/// Converts a Supabase JSON map into a [User] model.\n");
    md.push_str("/// Handles null safety and default values for optional fields.\n");
    md.push_str("factory User.fromSupabase(Map<String, dynamic> json) { ... }\n");
    md.push_str("```\n\n");
    md.push_str("**Rules for wording:**\n");
    md.push_str("- Use the **domain vocabulary** from the codebase (e.g., \"auth\", \"cache\", \"payment\")\n");
    md.push_str("- Mention **what the function returns** for getters and factory constructors\n");
    md.push_str("- Mention **when/where it is called** if it's a lifecycle or callback method\n");
    md.push_str(
        "- Mention **side effects** (e.g., \"updates local cache\", \"writes to Supabase\")\n",
    );
    md.push_str("- **Do NOT** describe implementation details (\"loops over list\", \"uses a forEach\")\n\n");
    md.push_str("## CRITICAL CONSTRAINTS — DO NOT VIOLATE\n\n");
    md.push_str("1. **DO NOT modify any code.** Only add `///` lines immediately before the function declaration.\n");
    md.push_str("2. **DO NOT change signatures, logic, imports, or formatting.**\n");
    md.push_str("3. **DO NOT use `//` comments.** Only `///` doc comments are indexed.\n");
    md.push_str(
        "4. **DO NOT add comments inside function bodies.** Only top-of-function doc comments.\n",
    );
    md.push_str("5. **DO NOT delete, move, or rename any functions.**\n");
    md.push_str("6. **Keep each comment under 200 characters.** Prefer 1-2 lines.\n");
    md.push_str("7. **DO NOT add comments to Flutter `build()` methods** unless they contain complex business logic.\n\n");
    md.push_str("## IMPORTANT — TAKE THIS SERIOUSLY\n\n");
    md.push_str("This is a **production codebase**. Vague, generic, or placeholder comments (e.g., \"This method does something\")\n");
    md.push_str("will get you fired.\n\n");
    md.push_str("A very important search tool will depend on these comments, you will prioritize quality over speed. No one will be expecting for you finish fast, take your time.\n");
    md.push_str("Write comments that **you** would find useful 6 months from now when you're debugging at 2am.\n\n");
    md.push_str("If you are unsure what a function does, infer from:\n");
    md.push_str("- Its name and parameters\n");
    md.push_str("- The class it belongs to\n");
    md.push_str("- Other functions called inside its body\n");
    md.push_str("- The file it lives in (e.g., `book_service.dart` implies database/network operations)\n\n");
    md.push_str("---\n\n");
    md.push_str("## Functions Requiring Doc Comments\n\n");
    md.push_str("Work through this list file by file. For each entry, locate the function at the given line and add a `///` doc comment.\n\n");
    if missing_docs.is_empty() {
        md.push_str("*All methods and functions have doc comments!* 🎉\n\n");
    } else {
        md.push_str("| File | Symbol | Kind | Approx. Line |\n");
        md.push_str("|------|--------|------|-------------|\n");
        for (file, symbol, kind, line) in &missing_docs {
            md.push_str(&format!(
                "| {} | {} | {} | ~{} |\n",
                file, symbol, kind, line
            ));
        }
        md.push('\n');
    }
    md.push_str("---\n\n");

    // Dead code section
    md.push_str("## Potentially Dead Code\n\n");
    md.push_str(
        "These declarations have no inbound semantic references after framework dispatch,\n",
    );
    md.push_str(
        "Dart protocol, getter, and codegen suppressions. Each entry is worth a human review;\n",
    );
    md.push_str("private helpers, lifecycle methods, and serialization hooks are excluded.\n\n");
    if dead_code.is_empty() {
        md.push_str("*No dead code candidates found!* 🎉\n");
    } else {
        md.push_str("| File | Symbol | Kind |\n");
        md.push_str("|------|--------|------|\n");
        for candidate in dead_code {
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                candidate.file, candidate.symbol, candidate.kind
            ));
        }
    }

    // Write to .compas/audit.md
    if let Ok(repo_path) = std::fs::canonicalize(std::env::current_dir().unwrap_or_default()) {
        let audit_path = repo_path.join(".compas").join("audit.md");
        if let Ok(dir) = audit_path
            .parent()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no parent"))
        {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(&audit_path, &md) {
            Ok(_) => info!("wrote audit report to .compas/audit.md"),
            Err(e) => warn!("failed to write audit report: {}", e),
        }
    }
}

fn scan_missing_docs_for_manifest(
    repo_path: &Path,
    manifest: &HashMap<String, String>,
    processed_paths: &HashSet<String>,
) -> Vec<MissingDocEntry> {
    let registry = ChunkerRegistry::new();
    let mut missing_docs = Vec::new();

    for path_str in manifest.keys() {
        if processed_paths.contains(path_str) {
            continue;
        }

        let path = Path::new(path_str);
        let Some(language) = language_for_path(path) else {
            continue;
        };
        let Some(chunker) = registry.get(language) else {
            continue;
        };

        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let chunks = match chunker.chunk(path_str, &content) {
            Ok(chunks) => chunks,
            Err(_) => continue,
        };

        let display_path = relative_display_path(repo_path, path);
        missing_docs.extend(missing_docs_from_chunks(&display_path, &chunks));
    }

    missing_docs
}

/// Strip the `_pN` suffix added by the chunker when splitting large chunks.
/// This ensures graph symbols match the names returned by extract_calls.
fn strip_part_suffix(symbol: &str) -> String {
    // Match patterns like `foo_p1`, `bar_p12` at the end of the symbol
    if let Some(pos) = symbol.rfind("_p") {
        let suffix = &symbol[pos + 2..];
        if suffix.parse::<u32>().is_ok() {
            return symbol[..pos].to_string();
        }
    }
    symbol.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use compas::indexing::should_include;
    use compas::{docs::models::DocumentSearchResult, search::search_chunks};
    use serde_json::Value;
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
        match test_lock().lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("compas-cli-test-{name}-{nanos}"))
    }

    async fn wait_for_server(port: &str) {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{port}/health");
        for _ in 0..50 {
            if let Ok(resp) = client.get(&url).send().await {
                if resp.status().is_success() {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("server did not become ready on port {port}");
    }

    fn decl(file: &str, symbol: &str, kind: &str) -> AuditDeclaration {
        AuditDeclaration {
            file: file.into(),
            display_file: file.into(),
            symbol: symbol.into(),
            kind: kind.into(),
        }
    }

    fn reference(caller_file: &str, callee: &str) -> AuditReference {
        AuditReference {
            caller_file: caller_file.into(),
            callee: callee.into(),
        }
    }

    #[test]
    fn test_bare_name_match_credits_all_same_named_declarations() {
        // If `loadUsers` is called anywhere, every declaration named `loadUsers`
        // is considered live — even if the call is on a variable whose type we
        // cannot resolve (the common Dart case with Riverpod providers).
        let analysis = AuditFileAnalysis {
            declarations: vec![
                decl(
                    "lib/management/service_user_management.dart",
                    "UserManagementService.loadUsers",
                    "method",
                ),
                decl(
                    "lib/admin/service_admin.dart",
                    "AdminService.loadUsers",
                    "method",
                ),
            ],
            references: vec![reference(
                "lib/management/provider_user_management.dart",
                "loadUsers",
            )],
            file_edges: vec![],
            key_types: vec![],
        };

        let candidates = classify_dead_code_candidates(&analysis);
        assert!(
            candidates.is_empty(),
            "Expected bare-name match to credit all `loadUsers` declarations, got: {candidates:?}"
        );
    }

    #[test]
    fn test_lifecycle_methods_are_hard_suppressed() {
        let analysis = AuditFileAnalysis {
            declarations: vec![
                decl("lib/screen.dart", "MyScreen.createState", "method"),
                decl("lib/screen.dart", "_MyScreenState.initState", "method"),
                decl("lib/screen.dart", "_MyScreenState.build", "method"),
                decl(
                    "lib/widget.dart",
                    "MyInherited.updateShouldNotify",
                    "method",
                ),
            ],
            references: vec![],
            file_edges: vec![],
            key_types: vec![],
        };

        let candidates = classify_dead_code_candidates(&analysis);
        assert!(
            candidates.is_empty(),
            "Expected lifecycle methods to be suppressed, got: {candidates:?}"
        );
    }

    #[test]
    fn test_main_function_is_suppressed() {
        let analysis = AuditFileAnalysis {
            declarations: vec![decl("lib/main.dart", "main", "function")],
            references: vec![],
            file_edges: vec![],
            key_types: vec![],
        };

        let candidates = classify_dead_code_candidates(&analysis);
        assert!(
            candidates.is_empty(),
            "Expected `main` to be suppressed, got: {candidates:?}"
        );
    }

    #[test]
    fn test_codegen_protocol_methods_are_suppressed() {
        let analysis = AuditFileAnalysis {
            declarations: vec![
                decl("lib/model.dart", "User.fromJson", "constructor"),
                decl("lib/model.dart", "User.toJson", "method"),
                decl("lib/model.dart", "User.toString", "method"),
                decl("lib/model.dart", "User.hashCode", "getter_setter"),
                decl("lib/model.dart", "User.operator ==", "method"),
            ],
            references: vec![],
            file_edges: vec![],
            key_types: vec![],
        };

        let candidates = classify_dead_code_candidates(&analysis);
        assert!(
            candidates.is_empty(),
            "Expected protocol/codegen methods to be suppressed, got: {candidates:?}"
        );
    }

    #[test]
    fn test_private_members_are_suppressed() {
        let analysis = AuditFileAnalysis {
            declarations: vec![
                decl("lib/widget.dart", "MyWidget._buildRow", "method"),
                decl("lib/util.dart", "_internalHelper", "function"),
            ],
            references: vec![],
            file_edges: vec![],
            key_types: vec![],
        };

        let candidates = classify_dead_code_candidates(&analysis);
        assert!(
            candidates.is_empty(),
            "Expected private members to be suppressed, got: {candidates:?}"
        );
    }

    #[test]
    fn test_default_constructor_skipped_when_class_referenced() {
        // `HolidayService()` shows up in references as just `HolidayService`,
        // and the default constructor is keyed as `HolidayService.HolidayService`.
        let analysis = AuditFileAnalysis {
            declarations: vec![decl(
                "lib/service.dart",
                "HolidayService.HolidayService",
                "constructor",
            )],
            references: vec![reference("lib/provider.dart", "HolidayService")],
            file_edges: vec![],
            key_types: vec![],
        };

        let candidates = classify_dead_code_candidates(&analysis);
        assert!(
            candidates.is_empty(),
            "Expected default constructor to be alive when class is referenced, got: {candidates:?}"
        );
    }

    #[test]
    fn test_truly_unreferenced_method_is_flagged() {
        let analysis = AuditFileAnalysis {
            declarations: vec![decl("lib/abandoned.dart", "AbandonedService.run", "method")],
            references: vec![reference("lib/other.dart", "somethingElse")],
            file_edges: vec![],
            key_types: vec![],
        };

        let candidates = classify_dead_code_candidates(&analysis);
        assert_eq!(
            candidates.len(),
            1,
            "Expected exactly one candidate, got: {candidates:?}"
        );
        assert_eq!(candidates[0].symbol, "AbandonedService.run");
    }

    #[test]
    fn test_filename_placeholder_declarations_are_skipped() {
        let analysis = AuditFileAnalysis {
            declarations: vec![AuditDeclaration {
                file: "lib/core/providers/provider_auth.dart".into(),
                display_file: "lib/core/providers/provider_auth.dart".into(),
                symbol: "provider_auth.dart".into(),
                kind: "declaration".into(),
            }],
            references: vec![],
            file_edges: vec![],
            key_types: vec![],
        };

        let candidates = classify_dead_code_candidates(&analysis);
        assert!(
            candidates.is_empty(),
            "Expected no candidates, got: {candidates:?}"
        );
    }

    #[test]
    fn test_getter_setter_kind_is_skipped() {
        let analysis = AuditFileAnalysis {
            declarations: vec![decl("lib/svc.dart", "Service.someGetter", "getter_setter")],
            references: vec![],
            file_edges: vec![],
            key_types: vec![],
        };

        let candidates = classify_dead_code_candidates(&analysis);
        assert!(
            candidates.is_empty(),
            "Expected getter_setter to be suppressed, got: {candidates:?}"
        );
    }

    #[test]
    fn test_normalize_import_uri_returns_manifest_style_absolute_paths() {
        let repo_path = Path::new("/repo");
        let base_file = Path::new("/repo/lib/main.dart");

        assert_eq!(
            normalize_import_uri(
                repo_path,
                base_file,
                "package:tyoajanseuranta/core/providers/provider_auth.dart",
                Some("tyoajanseuranta"),
            ),
            Some("/repo/lib/core/providers/provider_auth.dart".to_string())
        );
        assert_eq!(
            normalize_import_uri(repo_path, base_file, "auth/screen_login.dart", None),
            Some("/repo/lib/auth/screen_login.dart".to_string())
        );
    }

    #[test]
    fn test_reachable_files_follow_package_local_imports() {
        // Reachability is no longer used by dead-code classification, but the
        // helper is kept around for potential future "orphan files" reporting.
        let analysis = AuditFileAnalysis {
            declarations: vec![],
            references: vec![],
            file_edges: vec![(
                "/repo/lib/main.dart".into(),
                "/repo/lib/core/providers/provider_auth.dart".into(),
            )],
            key_types: vec![],
        };

        let manifest = HashMap::from([
            ("/repo/lib/main.dart".to_string(), "hash1".to_string()),
            (
                "/repo/lib/core/providers/provider_auth.dart".to_string(),
                "hash2".to_string(),
            ),
        ]);
        let reachable = reachable_files(&manifest, &analysis);

        assert!(reachable.contains("/repo/lib/main.dart"));
        assert!(reachable.contains("/repo/lib/core/providers/provider_auth.dart"));
    }

    #[tokio::test]
    async fn test_init_index_and_search_over_http() {
        let _guard = lock_tests();

        let repo_dir = unique_temp_path("repo");
        let home_dir = unique_temp_path("home");
        std::fs::create_dir_all(repo_dir.join("lib")).unwrap();
        std::fs::create_dir_all(&home_dir).unwrap();
        std::fs::write(
            repo_dir.join("lib").join("auth_service.dart"),
            r#"class AuthService {
  Future<String> login(String email, String password) async {
    return email;
  }
}
"#,
        )
        .unwrap();

        let original_dir = std::env::current_dir().unwrap();
        let original_home = std::env::var_os("HOME");
        let original_port = std::env::var_os("COMPAS_PORT");

        let port = ((SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
            % 1000)
            + 31000)
            .to_string();

        std::env::set_current_dir(&repo_dir).unwrap();
        std::env::set_var("HOME", &home_dir);
        std::env::set_var("COMPAS_PORT", &port);

        init_repo().unwrap();

        let config_path = repo_dir.join("compas.yaml");
        let config_text = std::fs::read_to_string(&config_path).unwrap();
        let updated = config_text
            .replace("provider: fastembed", "provider: test")
            .replace("model: nomic-ai/nomic-embed-text-v1.5", "model: test");
        std::fs::write(&config_path, updated).unwrap();

        let config = AppConfig::load(config_path.to_str().unwrap()).unwrap();
        index_repo(config).await.unwrap();

        let serve_handle = tokio::spawn(async { serve().await.unwrap() });
        wait_for_server(&port).await;

        let response: Value =
            reqwest::get(format!("http://127.0.0.1:{port}/search?q=authentication"))
                .await
                .unwrap()
                .json()
                .await
                .unwrap();

        let results = response["results"].as_array().unwrap();
        assert!(
            !results.is_empty(),
            "expected search results, got {response}"
        );
        let first = &results[0]["chunk"];
        assert!(
            first["symbol"].is_string(),
            "expected chunk.symbol in {response}"
        );
        assert!(
            first["language"].is_string(),
            "expected chunk.language in {response}"
        );
        assert!(
            first["line_start"].is_number(),
            "expected chunk.line_start in {response}"
        );
        assert!(
            first["line_end"].is_number(),
            "expected chunk.line_end in {response}"
        );
        assert!(
            first["type"].is_string(),
            "expected chunk.type in {response}"
        );
        let symbols: Vec<&str> = results
            .iter()
            .filter_map(|result| result["chunk"]["symbol"].as_str())
            .collect();
        assert!(
            symbols.contains(&"AuthService") || symbols.contains(&"AuthService.login"),
            "expected auth symbols in results, got {symbols:?}"
        );

        serve_handle.abort();
        let _ = serve_handle.await;

        std::env::set_current_dir(original_dir).unwrap();
        match original_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match original_port {
            Some(value) => std::env::set_var("COMPAS_PORT", value),
            None => std::env::remove_var("COMPAS_PORT"),
        }

        std::fs::remove_dir_all(&repo_dir).unwrap();
        std::fs::remove_dir_all(&home_dir).unwrap();
    }

    #[tokio::test]
    async fn test_optimize_edge_shard_succeeds_for_initialized_repo() {
        let repo_dir = unique_temp_path("optimize");
        std::fs::create_dir_all(repo_dir.join(".compas")).unwrap();

        let config = AppConfig {
            repo: compas::config::RepoConfig {
                path: repo_dir.to_string_lossy().to_string(),
                include: vec!["lib/**/*.dart".into()],
                exclude: vec![],
            },
            embedder: compas::config::EmbedderConfig {
                provider: "fastembed".into(),
                model: "nomic-ai/nomic-embed-text-v1.5".into(),
                query_prefix: None,
                doc_prefix: None,
            },
            store: compas::config::StoreConfig {
                provider: "edge".into(),
                path: ".compas/edge-shard".into(),
                vector_name: "default".into(),
            },
            server: compas::config::ServerConfig {
                host: "127.0.0.1".into(),
                port: "3001".into(),
            },
            index: compas::config::IndexConfig {
                kind: "code".into(),
                chunk_by: "function".into(),
                watch: true,
            },
        };

        let store = EdgeStore::new(repo_dir.join(&config.store.path), &config.store.vector_name);
        store.init(4).await.unwrap();
        drop(store);

        let _ = optimize_edge_shard(&config).unwrap();

        std::fs::remove_dir_all(&repo_dir).unwrap();
    }

    #[tokio::test]
    async fn test_index_document_repo_and_search_chunks() {
        let _guard = lock_tests();

        let repo_dir = unique_temp_path("document-repo");
        let library_dir = unique_temp_path("document-repo-library");
        std::fs::create_dir_all(repo_dir.join("docs")).unwrap();

        std::fs::write(
            repo_dir.join("docs").join("guide.md"),
            "# Auth Guide\n\nAuthentication overview.\n\n## Cache\n\nCache login tokens safely.\n",
        )
        .unwrap();
        std::fs::write(
            repo_dir.join("docs").join("notes.txt"),
            "Authentication notes.\n\nCache notes.",
        )
        .unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join("documents")
                .join("text-selectable-two-pages.pdf"),
            repo_dir.join("docs").join("reference.pdf"),
        )
        .unwrap();

        std::fs::write(
            repo_dir.join("compas.yaml"),
            r#"repo:
  path: .
  include:
    - "**/*.md"
    - "**/*.txt"
    - "**/*.pdf"
  exclude: []

embedder:
  provider: test
  model: test

store:
  provider: edge
  path: .compas/edge-shard
  vector_name: default

server:
  host: 127.0.0.1
  port: "3001"

index:
  kind: document
  chunk_by: function
  watch: false
"#,
        )
        .unwrap();

        let _cwd_guard = CurrentDirGuard::set(&repo_dir);
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);

        let config = load_command_config(&repo_dir.join("compas.yaml"), None).unwrap();
        let storage = document_storage_paths(&repo_dir);
        index_repo(config.clone()).await.unwrap();

        let embedder = build_embedder(&config.embedder).unwrap();
        let store = EdgeStore::new(&storage.store_path, &config.store.vector_name);
        let hits = search_chunks(
            embedder.as_ref(),
            &store,
            "authentication",
            10,
            &HashMap::new(),
        )
        .await
        .unwrap();

        assert!(!hits.is_empty(), "expected document hits");

        let results: Vec<DocumentSearchResult> = hits
            .into_iter()
            .map(DocumentSearchResult::try_from)
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(!results.is_empty());
        assert!(results
            .iter()
            .all(|result| !result.chunk.file_name.is_empty()));
        assert!(results
            .iter()
            .all(|result| { ["md", "txt", "pdf"].contains(&result.chunk.extension.as_str()) }));
        assert!(
            results.iter().any(|result| {
                !result.chunk.heading_path.is_empty() || result.chunk.page_start.is_some()
            }),
            "expected at least one result with heading path or page metadata"
        );
        assert!(!repo_dir.join(".compas").exists());
        assert!(!repo_dir.join(".compas").join("graph.json").exists());
        assert!(!repo_dir.join(".compas").join("audit.md").exists());
        assert!(storage.store_path.exists());
        assert!(storage.manifest_path.exists());
        std::fs::remove_dir_all(&repo_dir).unwrap();
        if library_dir.exists() {
            std::fs::remove_dir_all(library_dir).unwrap();
        }
    }

    #[tokio::test]
    async fn test_document_watch_mode_is_explicitly_unsupported() {
        let repo_dir = unique_temp_path("document-watch");
        std::fs::create_dir_all(&repo_dir).unwrap();

        let config = AppConfig {
            repo: compas::config::RepoConfig {
                path: repo_dir.to_string_lossy().to_string(),
                include: vec!["**/*.md".into()],
                exclude: vec![],
            },
            embedder: compas::config::EmbedderConfig {
                provider: "test".into(),
                model: "test".into(),
                query_prefix: None,
                doc_prefix: None,
            },
            store: compas::config::StoreConfig {
                provider: "edge".into(),
                path: ".compas/edge-shard".into(),
                vector_name: "default".into(),
            },
            server: compas::config::ServerConfig {
                host: "127.0.0.1".into(),
                port: "3001".into(),
            },
            index: compas::config::IndexConfig {
                kind: "document".into(),
                chunk_by: "function".into(),
                watch: false,
            },
        };

        let error = watch(config).await.unwrap_err();
        assert_eq!(
            error.to_string(),
            "document watch mode is not implemented yet"
        );

        std::fs::remove_dir_all(&repo_dir).unwrap();
    }

    #[test]
    fn test_load_command_config_builds_default_document_mode_for_folder_override() {
        let repo_dir = unique_temp_path("document-default-config");
        let library_dir = unique_temp_path("document-default-config-library");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);

        let config = load_command_config(&repo_dir.join("missing.yaml"), Some(&repo_dir)).unwrap();
        let storage = document_storage_paths(&repo_dir);

        assert_eq!(config.index.kind, "document");
        assert_eq!(config.repo.path, repo_dir.to_string_lossy());
        assert_eq!(config.repo.include, vec!["**/*.md", "**/*.txt", "**/*.pdf"]);
        assert_eq!(Path::new(&config.store.path), storage.store_path.as_path());

        std::fs::remove_dir_all(&repo_dir).unwrap();
        if library_dir.exists() {
            std::fs::remove_dir_all(library_dir).unwrap();
        }
    }

    #[tokio::test]
    async fn test_search_repo_returns_document_results_for_folder_mode() {
        let _guard = lock_tests();

        let repo_dir = unique_temp_path("document-search");
        let library_dir = unique_temp_path("document-search-library");
        std::fs::create_dir_all(repo_dir.join("docs")).unwrap();
        std::fs::write(
            repo_dir.join("docs").join("policy.md"),
            "# Insurance Policy\n\nRenewal date is January 15.\n",
        )
        .unwrap();

        let mut config = default_document_config(&repo_dir);
        config.embedder.provider = "test".into();
        config.embedder.model = "test".into();

        let _cwd_guard = CurrentDirGuard::set(&repo_dir);
        let _env_guard = EnvVarGuard::set("COMPAS_DOCS_HOME", &library_dir);
        let storage = document_storage_paths(&repo_dir);
        index_repo(config.clone()).await.unwrap();
        add_folder(&repo_dir).unwrap();

        let output = search_repo(config, "renewal date", 5).await.unwrap();

        assert!(
            output.contains("Found 1 relevant document result"),
            "{output}"
        );
        assert!(output.contains("policy.md"), "{output}");
        assert!(output.contains("Page: n/a"), "{output}");
        assert!(output.contains("Renewal date is January 15."), "{output}");
        assert!(!repo_dir.join(".compas").exists());
        assert!(storage.store_path.exists());
        assert!(storage.manifest_path.exists());

        std::fs::remove_dir_all(&repo_dir).unwrap();
        if library_dir.exists() {
            std::fs::remove_dir_all(library_dir).unwrap();
        }
    }

    struct CurrentDirGuard {
        original: PathBuf,
    }

    impl CurrentDirGuard {
        fn set(path: &Path) -> Self {
            let original = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { original }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
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
    fn test_watch_include_patterns_match_nested_dart_files() {
        assert!(should_include(
            Path::new("lib/services/auth_service.dart"),
            &["lib/**/*.dart".into()],
            &[]
        ));
        assert!(!should_include(
            Path::new("build/generated/auth_service.g.dart"),
            &["lib/**/*.dart".into()],
            &["**/*.g.dart".into(), "build/**".into()]
        ));
    }
}
