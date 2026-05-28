# AGENTS.md — Developer Workflow

This file is for AI agents and human collaborators working on the `compas-docs` workspace.

## Critical Rules

### Communication

- Prefer plain language first.
- Explain what changed, why it matters, and what was verified.
- When working on UI, keep the product focus in mind: the main desktop window is `Search` + `Results`, not a dashboard.

### Git Operations

**NEVER perform git operations without explicit user approval.**

- Do NOT run `git commit`, `git push`, `git add`, `git reset`, `git rebase`, or other git mutation commands unless the user explicitly asks for them.
- Do NOT stage files, create branches, or merge without confirmation.
- Writing commit messages is fine, but deliver them as text for the user to copy.
- The user controls git history.

## Workspace Overview

This repo is now a mixed Rust + Tauri + React workspace.

```text
compas-docs/
├── src/                         # Core compas CLI/server/code-search code
│   ├── bin/compas.rs            # CLI entry point
│   ├── chunker/                 # Dart/Rust chunkers + tests
│   ├── code/                    # Code graph/ranking/models
│   ├── docs/                    # In-crate document modules used by CLI/tests
│   ├── docs_backend.rs          # Compatibility wrapper over compas-docs-core
│   ├── indexing.rs              # Code indexing pipeline
│   ├── mcp/                     # MCP server + tools
│   ├── search.rs                # Shared embedding/store search helper
│   ├── server.rs                # HTTP server
│   └── store/edge.rs            # qdrant-edge store for code mode
├── crates/
│   └── compas-docs-core/        # Document-mode backend shared by CLI + Tauri
│       ├── backend.rs           # Folder registry, indexing, search, stats
│       ├── sqlite.rs            # SQLite persistence for document chunks
│       ├── exact.rs             # Tantivy exact search
│       ├── vector.rs            # HNSW semantic search
│       └── ranking.rs           # Hybrid merge/ranking
├── src-tauri/                   # Desktop shell
│   ├── src/lib.rs               # Tauri commands, menu wiring, secondary windows
│   ├── tauri.conf.json          # Tauri config
│   └── capabilities/            # Window capability config
├── app/                         # React/Vite frontend for desktop UI
│   └── src/
│       ├── main.tsx             # Window-mode entrypoint
│       ├── MainWindow.tsx       # Main search/results window
│       ├── LibraryWindow.tsx    # Folder management window
│       ├── StatsWindow.tsx      # Stats window
│       └── styles.css           # Shared desktop UI styling
├── tests/
│   ├── playwright/              # Local browser smoke tests
│   └── README.md                # UI testing notes
├── package.json                 # Root Playwright/Tauri CLI tooling
├── opencode.json                # Local Playwright MCP registration
└── Cargo.toml                   # Workspace root
```

## Current Architecture

### Code Mode

- Code search still uses `qdrant-edge`.
- The code path lives in the root crate under `src/`.
- Search quality is semantic retrieval plus post-search reranking.

### Document Mode

- Document mode no longer uses `qdrant-edge` for storage/search.
- Document mode uses the `compas-docs-core` crate.
- Storage is hybrid:
  - SQLite for chunk/document persistence
  - Tantivy for exact retrieval
  - HNSW for semantic retrieval
  - Hybrid reranking/merge in Rust
- Indexed document data is stored per folder under:
  - `~/.config/compas-docs/indices/<folder-id>/`
- The compatibility `edge-shard/` directory may still exist for path compatibility, but document search does not rely on qdrant-edge.

### Desktop App

- The desktop app is a Tauri v2 shell over the document backend.
- Tauri commands are defined in `src-tauri/src/lib.rs`.
- The desktop app now uses a single main window and switches between:
  - `main`
  - `library`
  - `stats`
- The menu bar routes the main window between these views instead of opening secondary windows.
- The frontend entrypoint in `app/src/main.tsx` resolves which React screen to render based on either:
  - Tauri navigation events in the main window
  - browser fallback query param like `?window=library`

## Build And Validation Commands

### Rust

```bash
# Development build
cargo build

# Release build
cargo build --release

# Tests
cargo test

# Lint
cargo clippy -- -W clippy::all

# Format
cargo fmt

# Check Tauri crate only
cargo check -p app
```

### Frontend / Desktop Tooling

Use Bun for frontend commands.

```bash
# Frontend build
"/Users/alexandro/.bun/bin/bun" run build

# App dev server (inside app/)
"/Users/alexandro/.bun/bin/bun" run dev

# Tauri desktop dev from repo root
bunx tauri dev
```

### Local UI Smoke Tests

The repo includes fully local Playwright browser smoke tests.

```bash
# Install local Chromium once
npm run playwright:install

# Run smoke tests
npm run test:ui
```

These tests exercise the browser fallback routes:

- `/?window=main`
- `/?window=library`
- `/?window=stats`

## Hygiene Rule

After code changes, run the full relevant validation cycle and iterate until it passes.

For search-quality changes, also run the evaluator benchmark before and after the change and compare the score:

```bash
python3 scripts/evaluate_document_search.py --binary ./target/release/compas --cases scripts/document_eval_cases.example.json
```

Treat that evaluator score as the regression/improvement baseline for document-search experiments. If you change ranking, chunking, extraction, or embedding behavior, include the before/after score in your summary.
The evaluator also writes a timestamped run artifact under `.evals/document-search/` so future comparisons do not depend on chat history.

For most Rust or desktop-app changes, use:

1. `cargo fmt`
2. `cargo clippy -- -W clippy::all`
3. `cargo build --release`
4. `cargo test`
5. `cargo check -p app`
6. `"/Users/alexandro/.bun/bin/bun" run build` in `app/`
7. `npm run test:ui` when frontend/UI behavior changed

For a docs-only change, do not run unnecessary builds.

## Tauri Notes

- Use `bunx tauri dev` from the repo root.
- Do **not** assume `cargo tauri dev` exists globally.
- `src-tauri/tauri.conf.json` already uses:
  - `beforeDevCommand: cd app && bun run dev`
  - `beforeBuildCommand: cd app && bun run build`
- Window capabilities currently allow:
  - `main`

## Frontend Notes

- The main window should stay focused on `Search` and `Results`.
- Do not reintroduce dashboard clutter into the main window.
- Avoid decorative gradients, purple-heavy “AI app” styling, or filler copy.
- Component titles should do the explanatory work; avoid redundant subtitles unless they serve a real need.
- When testing in browser mode, Tauri APIs are unavailable by design. The UI should degrade cleanly for Playwright smoke tests.

## Config And Storage Notes

- Root config is still `compas.yaml` for CLI/server behavior.
- `AppConfig::load()` defaults:
  - code mode store provider: `edge`
  - document mode store provider: `document-hybrid`
- Document library root defaults to:
  - `~/.config/compas-docs`
- Repo registry for code repos remains:
  - `~/.config/compas/repos.json`

## MCP And Opencode Notes

- The repo includes `opencode.json` registering a local Playwright MCP server:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "playwright": {
      "type": "local",
      "enabled": true,
      "command": ["npx", "@playwright/mcp"]
    }
  }
}
```

- Opencode must be restarted after changes to `opencode.json`.

## Search Ranking Notes

Code search quality is currently based on semantic retrieval plus reranking.

Current notable signals include:

- query token in symbol name
- query token in file path
- class/method preference
- graph caller/callee token matches
- private helper penalty

Document search uses the document-hybrid backend and has separate ranking behavior in `compas-docs-core`.

## Common Pitfalls

- Do not assume document mode still uses qdrant-edge. It does not.
- Do not assume the Tauri app compiles the root `compas` crate directly. It depends on `compas-docs-core`.
- Do not use browser-only smoke tests as proof that native Tauri menu behavior works; they validate the React screens and single-window view routing, not OS menu integration details.
- Do not break browser fallback mode by calling Tauri APIs unconditionally in React components.
- Do not add git operations unless the user explicitly asks.

## When Updating Docs

- Keep `AGENTS.md` aligned with the actual workspace structure.
- If you change validation commands, testing workflow, MCP config, or Tauri window behavior, update this file in the same task.
