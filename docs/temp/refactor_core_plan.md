# Core Refactor Implementation Plan

## 1. Plan Control

| Field | Value |
| --- | --- |
| Plan title | Decouple core index/search/store from code-specific metadata |
| Plan type | Refactor |
| Target project/repository | compas-docs |
| Target branch | executor creates branch from current HEAD |
| Authoring model | OpenCode / gpt-5.4 |
| Intended executing model | Lower-capability coding model |
| Date authored | 2026-05-25 |
| Expected completion state | The crate has a generic indexing/search/store core, while current code-search CLI, HTTP, and MCP behavior still works through code-specific adapter layers. |

## 2. Execution Contract

The executing model must follow this plan exactly.

- Do not make architectural decisions not explicitly specified in this plan.
- Do not broaden scope, add convenience features, rename unrelated code, or perform opportunistic cleanup.
- Do not skip validation steps. If a validation step fails, stop and follow the recovery instructions in Section 13.
- Do not mark the plan complete unless every item in Section 14 is satisfied.
- Preserve existing behavior unless this plan explicitly says to change it.
- Preserve user changes and unrelated work in the repository. Do not revert or overwrite files outside this plan.
- If a required file, symbol, command, or behavior differs from this plan, stop at the nearest checkpoint and record the discrepancy using the format in Section 13.4.
- Preserve the current external contracts for `compas index`, `compas serve`, `compas mcp`, HTTP `/search`, HTTP `/graph`, MCP tool names, and the response shape consumed by `scripts/evaluate_compas.py`.
- Do not add any document extractors, Tauri code, folder-picker UI, or document-specific ranking in this plan.

## 3. Context And Goal

### 3.1 Summary

Refactor the current crate so the reusable engine pieces are generic and do not assume code symbols, languages, line numbers, or graphs. The final structure of this refactor must leave indexing, search orchestration, and vector-store persistence generic, while isolating the current code-search behavior behind code-specific adapters and serializers so the existing product continues to work unchanged.

### 3.2 Problem Or Opportunity

The current fork is still the original code-search engine. The core types and flows are hard-coded to code concepts: `Chunk` requires `language`, `symbol`, and line numbers; `EdgeStore` persists those fields directly; `search.rs` imports `Graph` and applies symbol/class/method boosts; `index_repo` and `watch` are built directly around `ChunkerRegistry`, language detection, and graph mutation. That makes the document vertical hard to build because the shared runtime is not actually shared yet.

### 3.3 Intended Behavior Or Outcome

After completion, the crate must have a generic indexed-chunk model, a generic search path, and a generic indexing path that operate only on file path, content, generic kind, metadata, embeddings, manifests, and filters. All code-specific behavior that depends on AST chunkers, symbols, call graphs, or code ranking must live in code-focused modules. The current code-search behavior must still pass its existing CLI, HTTP, and MCP tests.

### 3.4 Links And References

| Type | Reference | Relevance |
| --- | --- | --- |
| Doc | `docs/temp/further dev plans/COMPAS_DOCS.md` | Product and architecture direction for turning compas into a document-search vertical. |
| Doc | `docs/temp/refactor_plan.md` | High-level ordered roadmap; this plan implements item 1. |
| Doc | `README.md` | Describes the current product behavior that this refactor must preserve. |
| Doc | `AGENTS.md` | Defines required validation sequence: `cargo fmt`, `cargo clippy -- -W clippy::all`, `cargo build --release`, `cargo test`. |
| Script | `scripts/evaluate_compas.py` | Depends on the current HTTP `/search` and `/graph` contracts and is a compatibility constraint. |

### 3.5 Prior Attempts Or Related Work

- `docs/temp/refactor_plan.md` is the only identified prior implementation note for this work; it is a one-line roadmap, not an execution-ready plan.
- `docs/temp/further dev plans/COMPAS_DOCS.md` already identifies the target split between reusable core and vertical-specific layers.
- No code-level refactor for this separation was identified by reading `src/models.rs`, `src/store/edge.rs`, `src/search.rs`, `src/server.rs`, `src/mcp/tools.rs`, and `src/bin/compas.rs`.

## 4. Plan Type Requirements

### 4.1 Feature Implementation Details

Not applicable. This plan is a refactor and must preserve current external behavior.

### 4.2 Bug Fix Details

Not applicable. This plan is not scoped to a single defect.

### 4.3 Refactor Details

#### Refactor Objective

Create a generic engine boundary for indexed content so the future document vertical can reuse indexing, store persistence, and search orchestration without inheriting code-only fields or graph-aware ranking. Preserve all current code-search behavior by pushing code-specific data shapes and ranking logic into a dedicated code layer.

#### Behavior Preservation Contract

| Behavior ID | Existing Behavior To Preserve | Verification Method |
| --- | --- | --- |
| R-1 | `compas index` still indexes Dart/Rust repos and produces searchable results. | `cargo test test_init_index_and_search_over_http -- --nocapture` |
| R-2 | HTTP `/search` still returns `results[].chunk.file_path`, `content`, `symbol`, `language`, `line_start`, `line_end`, and `type`. | Update and run `test_init_index_and_search_over_http`; inspect `src/server.rs` serializer path. |
| R-3 | HTTP `/graph` still returns the current graph response for code repos. | Keep graph logic behavior unchanged when a code graph is present; verify by inspection and full `cargo test`. |
| R-4 | MCP tool names remain `search_codebase` and `get_symbol_graph`. | Inspect `src/mcp/tools.rs`; run `cargo test tools_call_search_codebase_returns_edge_results -- --nocapture`. |
| R-5 | Code ranking weights and dedupe behavior remain unchanged. | Move the existing logic without changing constants; run ranking tests after relocation. |
| R-6 | Existing edge shards and legacy payloads remain readable after the refactor. | Add and run `edge_store_reads_legacy_code_payload_without_metadata_map`. |

#### Allowed Structural Changes

- Add `src/indexing.rs` for generic indexing flow and shared manifest/filter helpers.
- Add `src/code/` for code-specific graph, ranking, serializers, and indexing hooks.
- Replace the root `src/search.rs` implementation with a generic raw-search orchestrator.
- Move the current graph implementation from `src/graph.rs` into `src/code/graph.rs`.
- Move the current reranking logic from `src/search.rs` into `src/code/ranking.rs`.
- Replace the root `src/models.rs` contents with generic core models by the end of the plan.
- Update `src/store/mod.rs`, `src/store/edge.rs`, `src/server.rs`, `src/mcp/tools.rs`, `src/mcp/server.rs`, `src/mcp/state.rs`, `src/bin/compas.rs`, and related tests to use the new boundaries.

#### Forbidden Structural Changes

- Do not rename CLI commands, config file names, REST routes, or MCP tool names.
- Do not change the semantic ranking weights or result diversity rules.
- Do not add document extractors, PDF support changes, OCR, or any non-code indexing behavior.
- Do not change dependency versions in `Cargo.toml` unless a build failure makes it strictly necessary; if that happens, stop and file a discrepancy report instead of changing dependencies.
- Do not change `.compas/graph.json` format in this plan.
- Do not change `.compas/manifest.json` format in this plan.

## 5. Scope

### 5.1 In Scope

- Introduce a generic indexed-content model and generic search-hit model.
- Make the store layer persist generic chunks plus metadata instead of hard-coding code-only fields.
- Make the core search flow independent of `Graph` and code-symbol ranking.
- Make the core indexing flow independent of AST parsing and graph mutation by using adapter hooks.
- Isolate code graph, code result serialization, and code reranking in `src/code/`.
- Keep current HTTP and MCP code-search outputs stable through code-specific serializers.
- Add tests for generic payload round-trip and legacy payload compatibility.

### 5.2 Out Of Scope

- Document extractors, document chunkers, and document ranking.
- Tauri, Electron, SwiftUI, or any app-shell work.
- CLI rebrand from `compas` to `compas-docs`.
- Changes to the user-facing `init` prompts, generated `AGENTS.md`, or repo-registration behavior.
- Search result grouping, document citations, or any new UI response shape.

### 5.3 Non-Goals

- Improving search quality.
- Improving indexing performance.
- Reorganizing every code-specific module under `src/code/`; only the modules required for this boundary refactor must move.
- Removing the current code-audit feature.

### 5.4 Success Criteria

| ID | Criterion | Verification |
| --- | --- | --- |
| S-1 | `src/models.rs` defines generic core types and no longer requires `language`, `symbol`, `line_start`, or `line_end` as root model fields. | Inspect `src/models.rs` after Step 5. |
| S-2 | `src/store/mod.rs`, `src/search.rs`, and `src/indexing.rs` do not import graph or chunker logic. | `rg "crate::graph|crate::code::graph|ChunkerRegistry|extract_calls_for_language" src/store/mod.rs src/search.rs src/indexing.rs` returns no matches. |
| S-3 | Current code-search HTTP and MCP behavior still passes automated compatibility tests. | Run tests listed in Sections 11.3 and 14.1. |
| S-4 | Edge payloads round-trip generic metadata and still read legacy code payloads. | Run `edge_store_round_trips_indexed_chunk_metadata` and `edge_store_reads_legacy_code_payload_without_metadata_map`. |
| S-5 | Full hygiene passes. | Run `cargo fmt`, `cargo clippy -- -W clippy::all`, `cargo build --release`, `cargo test`. |

## 6. Current State Analysis

### 6.1 Relevant Files And Responsibilities

| File | Symbol(s) / Section(s) | Current Responsibility | Planned Action |
| --- | --- | --- | --- |
| `src/models.rs` | `Chunk`, `SearchResult`, `SymbolNode` | Root data model is code-shaped and includes symbol, language, and line fields. | Modify heavily; final state becomes generic core models only. |
| `src/store/mod.rs` | `Store` trait | Generic store trait currently exposes code-shaped chunk and search-result types. | Modify. |
| `src/store/edge.rs` | `EdgeStore`, `search_result_from_scored_point`, tests | Persists hard-coded code payload fields and reconstructs code-shaped results. | Modify heavily and add tests. |
| `src/search.rs` | `rerank_results`, tests | Applies code-only ranking rules and depends on `Graph`. | Move logic out; replace with generic raw-search orchestration. |
| `src/graph.rs` | `Graph`, `SymbolNode` | Code call graph implementation. | Move to `src/code/graph.rs`. |
| `src/server.rs` | `RepoState`, `search_handler`, `graph_handler` | HTTP layer embeds queries, queries store, reranks with graph, and returns raw code chunks. | Modify. |
| `src/mcp/state.rs` | `RepoState`, `McpAppState` | MCP runtime state currently assumes every repo has a graph. | Modify. |
| `src/mcp/tools.rs` | `handle_search`, `handle_graph`, `format_search_results` | MCP tools rely on code-shaped search results and mandatory graph state. | Modify. |
| `src/mcp/server.rs` | `handle_request` tests | Test coverage for MCP search still assumes raw code chunks. | Modify tests as needed. |
| `src/bin/compas.rs` | `index_repo`, `serve`, `watch`, `ReindexHandler`, helper functions | Main entrypoint contains the entire indexing/search wiring and mixes generic work with code-specific graph/audit logic. | Modify heavily. |
| `src/chunker/mod.rs` | `ChunkerRegistry`, `language_for_path`, `extract_calls_for_language` | Code-specific extraction API. | Read and keep as code adapter input; do not genericize in this plan. |
| `src/chunker/dart.rs` | `DartChunker` | Produces code chunks with symbol/language/line data. | Update imports if root `Chunk` moves in Step 5. |
| `src/chunker/rust.rs` | `RustChunker` | Produces code chunks with symbol/language/line data. | Update imports if root `Chunk` moves in Step 5. |
| `src/chunker/dart_test.rs` | chunker tests | Asserts fields on code chunk results. | Update if code chunk model moves in Step 5. |
| `src/chunker/rust_test.rs` | chunker tests | Asserts fields on code chunk results. | Update if code chunk model moves in Step 5. |
| `scripts/evaluate_compas.py` | `/search`, `/graph` contract checks | External compatibility consumer of current HTTP API. | Read only; use as compatibility constraint. |

### 6.2 Current Data And Control Flow

1. `src/bin/compas.rs::index_repo` loads config, embedder, edge store, chunker registry, and graph.
2. The same function walks files, hashes contents, applies include/exclude filters, and skips unchanged files.
3. Each changed file is parsed by a code chunker that returns a `Chunk` with `language`, `symbol`, `line_start`, `line_end`, and `kind`.
4. `index_repo` embeds the chunk content, upserts it through `src/store/edge.rs`, and then mutates the graph using chunk symbols and extracted call edges.
5. `src/server.rs::search_handler` and `src/mcp/tools.rs::handle_search` embed the query, call `Store::search`, rerank with `src/search.rs::rerank_results`, and return code-shaped results.
6. `src/server.rs::graph_handler` and `src/mcp/tools.rs::handle_graph` directly query graph state.
7. `src/bin/compas.rs::watch` duplicates parts of the indexing logic for single-file reindex and delete events.

### 6.3 Current Limitations

- `src/models.rs::Chunk` requires code-only fields, so non-code content would need fake `symbol`, `language`, and line-number values.
- `src/store/edge.rs` persists `symbol`, `language`, `line_start`, and `line_end` as first-class payload keys, so the store is not content-agnostic.
- `src/search.rs` depends on `Graph` and code semantics, so the root search module is not reusable.
- `src/bin/compas.rs::index_repo` mixes generic work like walking, hashing, embedding, and manifest handling with code-specific AST and graph work.
- `src/bin/compas.rs::watch` duplicates indexing mechanics instead of reusing a generic indexer.
- `src/mcp/state.rs` and `src/server.rs` both assume every repo has a graph.
- `src/models.rs` and `src/graph.rs` both define `SymbolNode`, but only the graph-local version is actually used.

### 6.4 Existing Tests And Coverage

| Test File | Test Name / Area | What It Covers Today | Gap This Plan Must Address |
| --- | --- | --- | --- |
| `src/store/edge.rs` | `edge_store_upsert_search_delete_and_reload` and related tests | Edge shard lifecycle, filters, invalid ids, vector mismatch, concurrency. | No generic metadata payload round-trip or legacy payload-compat test. |
| `src/search.rs` | `selected_content_boost_matches_experiment`, `normalized_query_terms_strip_punctuation` | Code-ranking constants and token normalization. | Must move intact to code-specific ranking tests. |
| `src/server.rs` | `health_reports_registered_repos_without_touching_store` | `/health` repo reporting only. | No `/search` response-shape test after genericization. |
| `src/mcp/tools.rs` | `search_codebase_reads_from_edge_store_without_daemon_fallback` | MCP search path can read store results directly. | Must continue passing after generic store/search changes. |
| `src/mcp/server.rs` | `tools_call_search_codebase_returns_edge_results` | MCP JSON-RPC search tool behavior. | Must continue passing after generic store/search changes. |
| `src/bin/compas.rs` | `test_init_index_and_search_over_http` | End-to-end index plus HTTP search on a temporary repo. | Must assert the full code-search JSON shape, not just one symbol field. |
| `src/bin/compas.rs` | `test_watch_include_patterns_match_nested_dart_files` | Current include/exclude filtering helper behavior. | Must still pass after helper extraction into `src/indexing.rs`. |
| `src/chunker/dart_test.rs` and `src/chunker/rust_test.rs` | chunker metadata assertions | Code chunkers populate code fields correctly. | May need import/type updates if code chunk model moves to `src/code/`. |

## 7. Target Design

### 7.1 Target Data And Control Flow

1. `src/bin/compas.rs` constructs an embedder, store, and a code indexing adapter, then calls a generic indexer in `src/indexing.rs`.
2. `src/indexing.rs` performs file discovery, include/exclude/ignore checks, manifest loading, changed-file detection, embedding, store upsert, deleted-file cleanup, and manifest persistence without importing chunkers or graphs.
3. The code indexing adapter converts code chunker output into generic indexed chunks and performs code-only hooks such as graph updates.
4. `src/search.rs` performs generic query embedding and raw store search, returning generic `SearchHit` values.
5. `src/code/ranking.rs` accepts generic search hits plus a code graph and produces ranked code search results with the existing boosts and dedupe rules.
6. `src/server.rs` and `src/mcp/tools.rs` serialize those ranked code results back into the current external response shape.
7. `src/code/graph.rs` remains the only graph implementation and is only referenced from code-specific layers.

### 7.2 API, Interface, Or Contract Changes

| Contract | Current | New | Compatibility Notes |
| --- | --- | --- | --- |
| Root core models | `Chunk`, `SearchResult`, unused `models::SymbolNode` | `IndexedChunk`, `SearchHit` only | Internal breaking change by end of plan; external HTTP/MCP shape stays the same via code serializers. |
| `Store::upsert` | Accepts `&[Chunk]` | Accepts `&[IndexedChunk]` | Internal breaking change; migrate all callers before removing wrappers. |
| `Store::search` | Returns `Vec<SearchResult>` | Returns `Vec<SearchHit>` | Internal breaking change; HTTP/MCP adapters convert hits to code results. |
| Root search module | `rerank_results(graph, raw_results, query, limit)` | `search_chunks(embedder, store, query, limit, filters)` | Internal breaking change; code ranking moves to `src/code/ranking.rs`. |
| Graph module path | `src/graph.rs` | `src/code/graph.rs` | Internal path change only. |
| HTTP `/search` response | Code-shaped chunk JSON | Same code-shaped chunk JSON | No external contract change allowed. |
| MCP tools | `search_codebase`, `get_symbol_graph` | Same names and arguments | No external contract change allowed. |

### 7.3 Data Model, Persistence, Or Migration Changes

| Store / File / Schema | Current State | Required Change | Migration / Backfill / Cleanup | Rollback Impact |
| --- | --- | --- | --- | --- |
| Qdrant Edge payload in `src/store/edge.rs` | Top-level code keys: `symbol`, `language`, `type`, `line_start`, `line_end`, `content`, `file_path` | Store generic core keys `chunk_id`, `file_path`, `content`, `kind`, `metadata`; for code chunks, also keep writing current top-level code keys during this refactor | No mandatory migration. New reader must accept both nested `metadata` and legacy top-level code keys. Reindex is optional, not required. | Rollback is low risk because old payload shape remains readable and new writes still include legacy top-level keys for code. |
| `.compas/graph.json` | Current graph JSON format | No format change | None | Rollback unaffected. |
| `.compas/manifest.json` | Current file-hash manifest | No format change | None | Rollback unaffected. |

### 7.4 Error Handling And Edge Behavior

| Case | Required Behavior | Verification |
| --- | --- | --- |
| Edge payload lacks nested `metadata` object but has old code fields | Reconstruct generic metadata from legacy top-level payload fields and continue returning a valid search hit | `edge_store_reads_legacy_code_payload_without_metadata_map` |
| Generic search hit is missing required code metadata during code-result serialization | Skip the malformed result, log a warning without chunk content, and continue returning other results | Add targeted serializer test and inspect logs if needed |
| Graph is absent for a repo when code graph lookup is requested | Return an explicit error from `/graph` and `get_symbol_graph` instead of panicking | Add test only if this branch becomes reachable in refactored runtime state |
| Unsupported file type reaches generic indexer | Generic indexer must defer to the adapter; if the adapter says unsupported, skip the file without error | Covered by current include/language gating and post-refactor adapter inspection |
| Delete event for a file with no stored points | Keep current behavior: delete is a best-effort no-op and must not fail the watch loop | Existing edge store lifecycle test plus watch path inspection |

### 7.5 Observability And Diagnostics

| Signal Type | Location | Required Signal | Sensitive Data Rules |
| --- | --- | --- | --- |
| log | `src/indexing.rs` | Reuse current progress and warning logs for skipped files, read failures, embed failures, and upsert failures | Do not log full chunk content or full document/code bodies |
| log | code result serialization path | Add a warning only when a malformed generic hit cannot be converted to a code result | Log chunk id and file path only; do not log chunk content |
| none | metrics/traces | No new metrics or tracing work is required in this plan | N/A |

## 8. Dependencies And Constraints

### 8.1 Upstream Dependencies

| Dependency | Type | Required State / Version | Verification |
| --- | --- | --- | --- |
| Rust toolchain and Cargo | tool | Must build the existing crate and run tests locally | `cargo build --release` |
| `qdrant-edge` | library | Must continue accepting nested JSON payload values and top-level fallback payload keys | Store tests in `src/store/edge.rs` |
| `fastembed` and test embedder | library | Production provider remains unchanged; tests may continue using the deterministic `test` provider | Existing HTTP integration test setup |
| Current chunkers (`src/chunker/*`) | prior work | Must remain behaviorally intact as code-specific adapters | Existing chunker tests and HTTP integration test |
| Existing HTTP/MCP contract | prior work | Must remain stable for current code-search workflows | Existing and added compatibility tests |

### 8.2 Downstream Dependents

| Dependent | How It Depends On This Area | Required Protection |
| --- | --- | --- |
| `src/bin/compas.rs::index_repo` | Depends on chunk model, store API, graph updates, and manifest flow | Preserve end-to-end HTTP integration test |
| `src/bin/compas.rs::watch` | Depends on include/exclude helpers, store API, and graph updates | Reuse generic indexer and keep filtering test passing |
| `src/server.rs` | Depends on store search results and graph access | Preserve `/search` and `/graph` response behavior |
| `src/mcp/tools.rs` | Depends on store search results and graph access | Preserve MCP tool names and text formatting |
| `scripts/evaluate_compas.py` | Reads HTTP `/search` and `/graph` JSON | Preserve field names and result shape |
| `src/chunker/*` tests | Depend on code chunk data fields | Update imports only if code chunk type moves; preserve assertions |

### 8.3 Environmental Constraints

- Work from the repository root `/Users/alexandro/GitHub/compas-docs`.
- Follow the repo hygiene cycle from `AGENTS.md`: `cargo fmt`, `cargo clippy -- -W clippy::all`, `cargo build --release`, `cargo test`.
- Some tests mutate `HOME` and `COMPAS_PORT`; keep those tests serialized and restore environment variables exactly as they already do.
- Do not run git mutation commands.

### 8.4 Security, Privacy, And Compliance Constraints

- Preserve the local-first behavior; do not add any network dependency beyond the existing local test behavior.
- Do not log indexed content or document/code bodies in new warnings.
- Keep the `test` embedder path available for automated tests so they do not need to download models or use external services.

## 9. Risk Assessment

| Risk ID | Risk | Why It Could Happen | Impact | Mitigation In This Plan | Verification |
| --- | --- | --- | --- | --- | --- |
| RISK-1 | HTTP `/search` output shape changes accidentally | Generic models replace code-shaped models internally | Breaks scripts and agent workflows | Use code-specific serializers and update integration test to assert the legacy JSON shape | `test_init_index_and_search_over_http` |
| RISK-2 | Existing edge shards become unreadable | Payload shape changes from hard-coded code fields to generic metadata | Existing local indexes would require forced reindex | Keep legacy top-level code keys on write and add legacy fallback on read | `edge_store_reads_legacy_code_payload_without_metadata_map` |
| RISK-3 | Generic indexer still leaks code-specific logic | `index_repo` currently mixes walking, embedding, chunking, graph, and audit in one function | Future document pipeline would inherit code assumptions | Extract generic indexer and require code-only behavior to live in adapter hooks | Inspection of `src/indexing.rs` plus full tests |
| RISK-4 | Graph/ranking move changes search ordering | `src/search.rs` currently contains code-only constants and logic | Relevance regression | Move logic verbatim into `src/code/ranking.rs`; do not change constants | Ranking tests and HTTP integration test |
| RISK-5 | Large `src/bin/compas.rs` edits introduce regressions | The file currently owns too many responsibilities | Multiple commands or tests could fail at once | Extract one subsystem at a time with step checkpoints | Step-level verification and final hygiene |
| RISK-6 | Duplicate `SymbolNode` definitions cause partial migration mistakes | One lives in `src/models.rs`, another in `src/graph.rs` | Dead code or wrong imports remain | Keep only the graph-local code version by the end of the plan | `rg "struct SymbolNode" src` shows one final definition |

## 10. Implementation Steps

### Step 1: Add generic core models in `src/models.rs`

#### 1.1 Purpose

Create additive generic types first so later store, search, and indexing refactors can migrate incrementally without breaking compilation.

#### 1.2 Files And Symbols

| Path | Symbol / Section | Action |
| --- | --- | --- |
| `src/models.rs` | model definitions and new test module | modify |

#### 1.3 Preconditions

- The repository still contains the current code-shaped `Chunk` and `SearchResult` definitions.

#### 1.4 Exact Change

**Format B: Bounded Discovery Then Deterministic Edit**

1. Inspect `src/models.rs`.
2. Add a new generic `IndexedChunk` struct with exactly these fields: `id: String`, `content: String`, `file_path: String`, `kind: String`, `metadata: std::collections::HashMap<String, serde_json::Value>`.
3. Add a new generic `SearchHit` struct with exactly these fields: `chunk: IndexedChunk`, `score: f32`.
4. Keep the existing code-shaped `Chunk` and `SearchResult` definitions in place for now; do not delete or rename them in this step.
5. Add helper methods on `IndexedChunk` to read metadata values by key as string and as `usize`, because later code serializers will need deterministic access to `symbol`, `language`, `line_start`, and `line_end`.
6. Add a unit test in `src/models.rs` named `indexed_chunk_metadata_helpers_preserve_strings_and_numbers` that stores a string value and a numeric value in metadata and asserts the helper methods return the expected values.

#### 1.5 Required Local Verification

| Verification ID | Command / Inspection | Expected Result |
| --- | --- | --- |
| V1.1 | `cargo test indexed_chunk_metadata_helpers_preserve_strings_and_numbers -- --nocapture` | Exit code 0; the new metadata-helper test passes. |

#### 1.6 Failure Handling For This Step

- New helper test fails: inspect the helper methods in `src/models.rs`, fix metadata type handling, and rerun V1.1.
- Any unrelated existing test fails after this additive step: stop and use Section 13.4, because this step must not change existing behavior.

### Step 2: Add code conversion models and generic store support

#### 2.1 Purpose

Teach the store layer to read and write generic chunks while keeping the legacy code chunk contract available during migration.

#### 2.2 Files And Symbols

| Path | Symbol / Section | Action |
| --- | --- | --- |
| `src/code/mod.rs` | module exports | create |
| `src/code/models.rs` | code conversion structs and helpers | create |
| `src/lib.rs` | module exports | modify |
| `src/store/mod.rs` | `Store` trait | modify |
| `src/store/edge.rs` | `EdgeStore` implementation and tests | modify |

#### 2.3 Preconditions

- V1.1 passed.
- `src/models.rs` contains both the old code-shaped types and the new generic types.

#### 2.4 Exact Change

**Format B: Bounded Discovery Then Deterministic Edit**

1. Create `src/code/mod.rs` and export `models`.
2. Create `src/code/models.rs` with:
   - constants for metadata keys `language`, `symbol`, `line_start`, and `line_end`
   - `CodeChunk` and `CodeSearchResult` structs whose serialized field names exactly match the current code-search JSON shape
   - conversion helpers between the current legacy root `Chunk` type and `CodeChunk`
   - conversion helpers between `CodeChunk` and `IndexedChunk`
   - conversion helpers between the current legacy root `SearchResult` type and `SearchHit` where needed for temporary wrappers
   - a unit test named `code_chunk_try_from_indexed_chunk_preserves_legacy_fields`
3. Update `src/lib.rs` to export the new `code` module.
4. In `src/store/mod.rs`, add temporary generic store methods alongside the legacy code-shaped methods. Use this exact migration rule:
   - add generic methods that operate on `IndexedChunk` and `SearchHit`
   - keep the current legacy methods in this step as default wrappers that convert to and from the generic methods through `src/code/models.rs`
5. In `src/store/edge.rs`, implement the generic methods as the canonical implementation.
6. Change the edge payload writer so it always writes `chunk_id`, `file_path`, `content`, `kind`, and a nested `metadata` object. For compatibility during this refactor, if the chunk metadata contains `language`, `symbol`, `line_start`, or `line_end`, also duplicate those values into the current top-level payload keys.
7. Change the edge payload reader so it reconstructs `IndexedChunk` from the nested `metadata` object when present. If `metadata` is absent, reconstruct the metadata map from the legacy top-level code fields and continue successfully.
8. Add two store tests in `src/store/edge.rs` named `edge_store_round_trips_indexed_chunk_metadata` and `edge_store_reads_legacy_code_payload_without_metadata_map`.

#### 2.5 Required Local Verification

| Verification ID | Command / Inspection | Expected Result |
| --- | --- | --- |
| V2.1 | `cargo test code_chunk_try_from_indexed_chunk_preserves_legacy_fields -- --nocapture` | Exit code 0; code conversion helper preserves legacy fields. |
| V2.2 | `cargo test edge_store_round_trips_indexed_chunk_metadata -- --nocapture` | Exit code 0; nested metadata survives store round-trip. |
| V2.3 | `cargo test edge_store_reads_legacy_code_payload_without_metadata_map -- --nocapture` | Exit code 0; legacy payload fallback works. |

#### 2.6 Failure Handling For This Step

- `qdrant-edge` rejects nested metadata or numeric values: keep metadata in `serde_json::Value`, inspect the exact payload type mismatch, and fix the payload construction instead of stringifying all values.
- Wrapper methods or conversions recurse incorrectly: inspect `src/store/mod.rs` and ensure only the generic methods are canonical in `EdgeStore`.

### Step 3: Move graph and ranking into `src/code/` and make root search generic

#### 3.1 Purpose

Remove graph and code-ranking knowledge from the root search path while keeping current code-search behavior unchanged.

#### 3.2 Files And Symbols

| Path | Symbol / Section | Action |
| --- | --- | --- |
| `src/graph.rs` | `Graph` implementation | rename to `src/code/graph.rs` |
| `src/search.rs` | entire module | replace |
| `src/code/mod.rs` | module exports | modify |
| `src/code/ranking.rs` | moved reranking logic and tests | create |
| `src/server.rs` | `RepoState`, `search_handler`, `graph_handler`, tests | modify |
| `src/mcp/state.rs` | `RepoState` | modify |
| `src/mcp/tools.rs` | `handle_search`, `handle_graph`, formatting, tests | modify |
| `src/mcp/server.rs` | tests | modify |
| `src/bin/compas.rs` | imports and repo loading paths | modify |
| `src/lib.rs` | module exports | modify |

#### 3.3 Preconditions

- V2.1, V2.2, and V2.3 passed.

#### 3.4 Exact Change

**Format B: Bounded Discovery Then Deterministic Edit**

1. Rename `src/graph.rs` to `src/code/graph.rs` without changing graph behavior or JSON persistence format.
2. Move the current reranking implementation from `src/search.rs` into a new file `src/code/ranking.rs`, preserving constants, dedupe behavior, and tests exactly.
3. Replace the root `src/search.rs` with a generic search function that:
   - embeds the query
   - calls the generic store search
   - returns raw `SearchHit` values
   - does not import `Graph` or inspect code metadata
4. Update `src/code/mod.rs` and `src/lib.rs` to export `graph` and `ranking` under `code`.
5. Change `src/server.rs` and `src/mcp/state.rs` runtime repo state so graph is code-specific state, not a mandatory root-level field. Use either a `code: Option<...>` field or an equivalent explicit code sub-struct, but do not leave `graph` as a mandatory generic field.
6. Update `src/server.rs::search_handler` and `src/mcp/tools.rs::handle_search` to call the generic root search function, then pass the raw hits through `src/code/ranking.rs`, then serialize through `src/code/models.rs` so the outward JSON/text shape stays the same.
7. Update `src/server.rs::graph_handler` and `src/mcp/tools.rs::handle_graph` to read graph state only from the code-specific runtime field.
8. Add a server test named `search_handler_returns_legacy_code_chunk_shape_from_generic_hits` that asserts the HTTP search response still includes `symbol`, `language`, `line_start`, `line_end`, and `type`.

#### 3.5 Required Local Verification

| Verification ID | Command / Inspection | Expected Result |
| --- | --- | --- |
| V3.1 | `cargo test selected_content_boost_matches_experiment -- --nocapture` | Exit code 0; ranking constants survived the move unchanged. |
| V3.2 | `cargo test search_handler_returns_legacy_code_chunk_shape_from_generic_hits -- --nocapture` | Exit code 0; HTTP serializer preserves code chunk fields. |
| V3.3 | `cargo test tools_call_search_codebase_returns_edge_results -- --nocapture` | Exit code 0; MCP search tool still works. |

#### 3.6 Failure Handling For This Step

- Search output shape changes: fix `src/code/models.rs` serialization, not the HTTP contract.
- Graph lookup fails because state moved: inspect repo state construction in `src/bin/compas.rs`, restore graph loading into the code-specific field, and rerun V3.2 and V3.3.

### Step 4: Extract a generic indexer and reuse it from `index_repo` and `watch`

#### 4.1 Purpose

Remove code-specific logic from the core indexing flow so file walking, hashing, manifests, embedding, and store writes become reusable.

#### 4.2 Files And Symbols

| Path | Symbol / Section | Action |
| --- | --- | --- |
| `src/indexing.rs` | generic indexer types and functions | create |
| `src/lib.rs` | module exports | modify |
| `src/bin/compas.rs` | `index_repo`, `watch`, `ReindexHandler`, helper functions, tests | modify |
| `src/chunker/mod.rs` | inspect only | inspect |
| `src/watcher.rs` | inspect only | inspect |

#### 4.3 Preconditions

- V3.1, V3.2, and V3.3 passed.

#### 4.4 Exact Change

**Format B: Bounded Discovery Then Deterministic Edit**

1. Create `src/indexing.rs` with:
   - `CompasIgnore`
   - `hash_bytes`
   - `should_include`
   - a generic indexing adapter trait that accepts file paths and contents and returns generic indexed chunks
   - generic indexer functions or a struct that own full-repo indexing, single-file reindex, deleted-file cleanup, and manifest persistence
2. The generic indexer must own only generic mechanics: file walking, include/exclude/ignore checks, manifest loading/saving, unchanged-file detection, embedding, store delete/upsert, and statistics.
3. Keep graph mutation, AST call extraction, `strip_part_suffix`, dead-code analysis, and audit generation out of `src/indexing.rs`.
4. In `src/bin/compas.rs`, implement a code adapter that uses the existing chunkers and graph logic to plug into the generic indexer.
5. Update `index_repo` to call the generic indexer through that adapter instead of owning the entire file loop itself.
6. Update `ReindexHandler::on_change` and `on_delete` to reuse the generic indexer’s single-file and delete paths instead of duplicating chunk/delete/embed/upsert/graph code.
7. Move `hash_bytes`, `CompasIgnore`, and `should_include` out of `src/bin/compas.rs` into `src/indexing.rs`. Update the current include/exclude test to import the new location.
8. Preserve the current audit generation and current CLI output text in `src/bin/compas.rs`; do not move audit logic unless required for compilation.

#### 4.5 Required Local Verification

| Verification ID | Command / Inspection | Expected Result |
| --- | --- | --- |
| V4.1 | `cargo test test_watch_include_patterns_match_nested_dart_files -- --nocapture` | Exit code 0; include/exclude behavior is preserved after helper extraction. |
| V4.2 | `cargo test test_init_index_and_search_over_http -- --nocapture` | Exit code 0; the generic indexer still supports end-to-end indexing and HTTP search. |

#### 4.6 Failure Handling For This Step

- Full indexing test fails: compare the new generic indexer flow to the old `index_repo` logic and restore any missing manifest, delete, or embedding step in `src/indexing.rs`.
- Watch helper test fails: inspect the moved `should_include` logic and make it byte-for-byte equivalent to the previous semantics.

### Step 5: Remove transitional legacy root models and finalize boundaries

#### 5.1 Purpose

Complete the refactor by making the generic types canonical in the core and moving the remaining code-shaped types fully into the code-specific layer.

#### 5.2 Files And Symbols

| Path | Symbol / Section | Action |
| --- | --- | --- |
| `src/models.rs` | legacy root `Chunk`, `SearchResult`, `SymbolNode` | modify/delete |
| `src/code/models.rs` | canonical code chunk/result models | modify |
| `src/store/mod.rs` | final generic trait API | modify |
| `src/store/edge.rs` | remove temporary wrapper implementation | modify |
| `src/chunker/dart.rs` | imports/types | modify if needed |
| `src/chunker/rust.rs` | imports/types | modify if needed |
| `src/chunker/dart_test.rs` | imports/types | modify if needed |
| `src/chunker/rust_test.rs` | imports/types | modify if needed |
| `src/server.rs` | final imports/types | modify |
| `src/mcp/tools.rs` | final imports/types | modify |
| `src/mcp/server.rs` | final imports/types | modify |
| `src/bin/compas.rs` | final imports/types and integration test assertions | modify |

#### 5.3 Preconditions

- V4.1 and V4.2 passed.
- All callers are already able to use generic store/search/indexing paths.

#### 5.4 Exact Change

**Format B: Bounded Discovery Then Deterministic Edit**

1. Inspect the repo with `rg "\bChunk\b|\bSearchResult\b|models::SymbolNode" src`.
2. Replace all remaining uses of the root code-shaped `Chunk` and `SearchResult` with `src/code/models.rs::CodeChunk` and `CodeSearchResult`, or with `IndexedChunk` and `SearchHit` where the caller is part of the generic core.
3. Update `src/chunker/dart.rs`, `src/chunker/rust.rs`, and their tests to use the canonical code-specific chunk type if they still depend on the old root type.
4. Remove the temporary legacy root `Chunk` and `SearchResult` definitions from `src/models.rs`.
5. Remove the unused duplicate `SymbolNode` definition from `src/models.rs`; the only remaining `SymbolNode` must be the code-graph version in `src/code/graph.rs`.
6. Remove the temporary legacy wrapper methods from `src/store/mod.rs` and `src/store/edge.rs`. The final canonical store API must expose only generic `IndexedChunk` and `SearchHit`.
7. Strengthen `test_init_index_and_search_over_http` so it asserts the returned search JSON still contains `chunk.symbol`, `chunk.language`, `chunk.line_start`, `chunk.line_end`, and `chunk.type`.
8. Update any root module exports and imports so `src/lib.rs` no longer re-exports or references removed legacy root model symbols.

#### 5.5 Required Local Verification

| Verification ID | Command / Inspection | Expected Result |
| --- | --- | --- |
| V5.1 | `cargo test test_init_index_and_search_over_http -- --nocapture` | Exit code 0; final end-to-end compatibility still holds. |
| V5.2 | `rg "crate::graph|crate::code::graph|ChunkerRegistry|extract_calls_for_language" src/store/mod.rs src/search.rs src/indexing.rs` | No matches. |
| V5.3 | `rg "struct SymbolNode" src` | Exactly one match in `src/code/graph.rs`. |

#### 5.6 Failure Handling For This Step

- Compilation breaks after removing legacy root types: restore only the minimal removed alias or conversion needed, migrate the remaining caller, then remove the temporary compatibility again before proceeding.
- `rg` checks still show graph or chunker imports in generic modules: move that logic into `src/code/` or back into the CLI adapter layer; do not accept the leak.

## 11. Testing Requirements

### 11.1 Test Strategy

| Level | Required? | Reason | Files / Commands |
| --- | --- | --- | --- |
| Unit | Yes | Core model conversion, edge payload compatibility, and ranking behavior must be locked down directly. | `src/models.rs`, `src/code/models.rs`, `src/store/edge.rs`, `src/code/ranking.rs`; use targeted `cargo test <name> -- --nocapture` commands. |
| Integration | Yes | The refactor is only successful if index -> store -> search -> HTTP/MCP still works together. | `cargo test test_init_index_and_search_over_http -- --nocapture`, `cargo test tools_call_search_codebase_returns_edge_results -- --nocapture` |
| End-to-end | No | There is no separate app or deployment surface in this plan beyond the automated integration paths already in the crate. | N/A |
| Static analysis / typecheck / lint | Yes | This refactor changes core traits, module paths, and many imports. | `cargo fmt`, `cargo clippy -- -W clippy::all`, `cargo build --release` |
| Manual verification | No | Automated compatibility tests are sufficient for this refactor and avoid dependence on an external repo. | N/A |

### 11.2 Tests To Add Or Modify

| Test ID | Test File | Test Name | Initial State / Fixture | Action | Expected Result |
| --- | --- | --- | --- | --- | --- |
| T-1 | `src/models.rs` | `indexed_chunk_metadata_helpers_preserve_strings_and_numbers` | Create one `IndexedChunk` with string and numeric metadata entries | Read metadata through helper methods | String and numeric helpers return exact expected values |
| T-2 | `src/code/models.rs` | `code_chunk_try_from_indexed_chunk_preserves_legacy_fields` | Create one generic `IndexedChunk` containing code metadata keys | Convert it to a code chunk | `symbol`, `language`, `line_start`, `line_end`, `type`, and `file_path` all match the legacy shape |
| T-3 | `src/store/edge.rs` | `edge_store_round_trips_indexed_chunk_metadata` | Temp shard plus one generic indexed chunk with metadata map | Upsert and search through generic store API | Returned hit preserves metadata and core fields |
| T-4 | `src/store/edge.rs` | `edge_store_reads_legacy_code_payload_without_metadata_map` | Insert a legacy payload shape into the edge shard without a nested metadata object | Read it through the new generic search path | Returned generic hit reconstructs metadata correctly |
| T-5 | `src/server.rs` | `search_handler_returns_legacy_code_chunk_shape_from_generic_hits` | Fake store returns generic `SearchHit` values with code metadata | Call `search_handler` | JSON response still includes legacy code chunk keys |
| T-6 | `src/bin/compas.rs` | `test_init_index_and_search_over_http` | Existing temporary repo integration fixture | Expand assertions on the HTTP response | Search result JSON contains `symbol`, `language`, `line_start`, `line_end`, and `type` |

### 11.3 Existing Tests That Must Continue Passing

| Command | Purpose | Expected Passing Result |
| --- | --- | --- |
| `cargo test edge_store_upsert_search_delete_and_reload -- --nocapture` | Protects edge shard lifecycle behavior | Exit code 0 |
| `cargo test search_codebase_reads_from_edge_store_without_daemon_fallback -- --nocapture` | Protects MCP search read path | Exit code 0 |
| `cargo test tools_call_search_codebase_returns_edge_results -- --nocapture` | Protects MCP JSON-RPC tool path | Exit code 0 |
| `cargo test health_reports_registered_repos_without_touching_store -- --nocapture` | Protects `/health` runtime state reporting | Exit code 0 |
| `cargo test test_watch_include_patterns_match_nested_dart_files -- --nocapture` | Protects include/exclude filtering behavior | Exit code 0 |
| `cargo test test_init_index_and_search_over_http -- --nocapture` | Protects end-to-end indexing and HTTP search behavior | Exit code 0 |

### 11.4 Edge Cases To Cover

| Edge Case | Coverage Method | Expected Result |
| --- | --- | --- |
| Legacy edge payload has no nested metadata object | T-4 | Generic search hit is reconstructed correctly |
| Generic chunk metadata contains numbers and strings | T-1 and T-3 | Helper methods and store round-trip preserve both correctly |
| Code serializer receives generic hit with code metadata | T-2 and T-5 | Legacy code response fields are emitted unchanged |
| Include/exclude filtering still works after helper extraction | Existing test `test_watch_include_patterns_match_nested_dart_files` | Filtering behavior is unchanged |
| Full index-to-search flow still works after generic indexer extraction | T-6 | Temporary repo indexes and `/search` returns compatible results |

### 11.5 Test Data And Fixtures

Not applicable. All required tests can continue using temporary directories, inline chunk values, and the existing deterministic test embedder.

## 12. Documentation, Configuration, And Generated Artifacts

### 12.1 Documentation Updates

| Document | Required Change | Reason |
| --- | --- | --- |
| `README.md` | No documentation change required | This refactor must preserve existing external behavior; user-facing docs would not change yet. |
| `docs/temp/further dev plans/COMPAS_DOCS.md` | No documentation change required | This plan implements the architecture direction already described there. |

### 12.2 Configuration Updates

| Config File / Setting | Required Change | Deployment / Runtime Impact |
| --- | --- | --- |
| None | No configuration change required | Existing `compas.yaml` and environment variables remain unchanged |

### 12.3 Generated Artifacts

| Artifact | Source Command | Edit Directly? | Expected Result |
| --- | --- | --- | --- |
| None | N/A | No | No generated artifacts change in this plan |

## 13. Rollback And Recovery

### 13.1 Safe Checkpoints

| Checkpoint | Reached After | Safe State Description | Validation Command |
| --- | --- | --- | --- |
| C-1 | Step 1 | Generic core models are additive only; existing behavior is untouched | `cargo test indexed_chunk_metadata_helpers_preserve_strings_and_numbers -- --nocapture` |
| C-2 | Step 2 | Store supports generic payloads while legacy wrappers still exist | `cargo test edge_store_round_trips_indexed_chunk_metadata -- --nocapture` |
| C-3 | Step 3 | Generic search and code-specific ranking are separated; HTTP/MCP search still works | `cargo test tools_call_search_codebase_returns_edge_results -- --nocapture` |
| C-4 | Step 4 | Generic indexer is in use and end-to-end index/search behavior still works | `cargo test test_init_index_and_search_over_http -- --nocapture` |
| C-5 | Step 5 | Final boundaries are complete and generic core no longer depends on graph or chunkers | `cargo test test_init_index_and_search_over_http -- --nocapture` |

### 13.2 Step Failure Recovery

- If a verification command fails, do not continue to the next implementation step.
- First inspect the failing output and compare it to the expected result listed for that verification.
- If the failure is caused by the step just performed, revert only that step's changes and retry the step once.
- If the failure is caused by pre-existing unrelated work, stop and report the discrepancy using Section 13.4.
- If the failure source cannot be determined within 2 attempts, stop and report the discrepancy using Section 13.4.

### 13.3 Rollback Procedure

1. Identify the files changed in the current failed step only.
2. Reverse only those file edits using a reverse patch or by manually restoring the prior contents of those files; do not use destructive repo-wide commands.
3. Remove any new files created only by the failed step if they are not referenced by successful earlier checkpoints.
4. Rerun the checkpoint command for the last completed safe checkpoint to confirm the repo is back in a known-good state.

### 13.4 Discrepancy Report Format

```markdown
## Plan Discrepancy Report

Step: <step number and title>
Expected: <what the plan said would be true>
Actual: <what the executor observed>
Files inspected: <paths>
Commands run: <commands and exit statuses>
User/unrelated changes detected: <yes/no/unknown; details>
Recommended next action: <specific recommendation without making unauthorized changes>
```

## 14. Final Verification And Completion Checklist

### 14.1 Final Verification Commands

| Order | Command | Expected Result |
| --- | --- | --- |
| 1 | `cargo fmt` | Exit code 0 and source files are formatted |
| 2 | `cargo clippy -- -W clippy::all` | Exit code 0 with no remaining lint errors |
| 3 | `cargo build --release` | Exit code 0 and release build completes |
| 4 | `cargo test` | Exit code 0 and full test suite passes |

### 14.2 Completion Checklist

- [ ] Plan type-specific requirements in Section 4 are satisfied.
- [ ] All in-scope items in Section 5.1 are complete.
- [ ] No out-of-scope items in Section 5.2 were implemented.
- [ ] Every implementation step in Section 10 was completed in order.
- [ ] Every local verification in Section 10 passed before proceeding to the next step.
- [ ] All tests listed in Section 11.2 were added or updated exactly as specified.
- [ ] All commands listed in Section 11.3 and Section 14.1 pass.
- [ ] Edge cases listed in Section 11.4 are covered.
- [ ] Documentation, configuration, and generated artifact requirements in Section 12 are complete.
- [ ] Security, privacy, and compliance constraints in Section 8.4 are satisfied.
- [ ] No unrelated files were modified.
- [ ] The final diff matches the intended files and symbols in this plan.
- [ ] Rollback instructions in Section 13 remain accurate after implementation.

### 14.3 Final Response Requirements For Executing Model

When the plan is complete, report:

- One sentence summary of what changed
- List of files changed
- Tests and commands run with pass/fail results
- Any deviations from the plan, or `No deviations`
- Any follow-up work intentionally left out of scope

## 15. Appendix

### 15.1 Full Before / After Examples

Not applicable.

### 15.2 Reference Logs Or Outputs

Not applicable.

### 15.3 Additional Execution Notes

- Keep `src/chunker/` in its current directory during this plan. It is still code-specific, but physically moving it under `src/code/` is intentionally out of scope for this first refactor.
- If the final implementation can preserve legacy edge payload compatibility without writing duplicate top-level code keys, stop and file a discrepancy report before changing the compatibility strategy. This plan assumes duplicate top-level write-through during the transition to minimize reindex risk.
