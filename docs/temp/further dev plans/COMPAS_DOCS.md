# compas-docs

> Ask your documents before opening them.

A local-first macOS app that lets you semantically search PDFs, Word docs, Markdown files, notes, transcripts, manuals, and reports — without uploading anything to the cloud.

---

## The reframe

For code, compas says:

> "Don't make agents read the whole repo. Let them ask the repo where the relevant code is."

For documents, compas-docs says:

> "Don't make people or agents open ten PDFs and skim hundreds of pages. Let them ask their document library where the answer probably is."

That is a much broader user base. Developers have codebases. Everyone has documents.

---

## Should this be a fork?

Short answer: **maybe for speed, but not as the long-term architecture.**

Do not permanently fork `compas` and strip things away. That creates two diverging projects that both want the same core:

- local embedding
- chunking and indexing
- file watching
- incremental reindexing
- semantic search
- metadata filtering
- result ranking
- MCP exposure

Instead, the ideal long-term structure is:

```text
compas/
├── compas-core/            # reusable engine
├── compas-code/            # current code-search product
├── compas-docs/            # document-search product
└── apps/
    └── compas-docs-macos/
```

For the first prototype, a fork is totally reasonable. The practical approach:

1. **Prototype fast in a fork.**
2. Prove the user experience.
3. Extract shared pieces back into `compas-core`.

Do not over-architect before the product feels good.

---

## What to remove from current compas

For `compas-docs`, remove or ignore initially:

- AST parsing
- symbol graph
- language-specific chunkers
- call relationship extraction
- graph endpoint
- symbol-specific ranking boosts
- code-only metadata (`symbol`, `kind`, `line_start`, `line_end`)

Keep:

- FastEmbed embedder
- embedded Qdrant Edge vector store
- file walker and hashing
- incremental indexing
- `.compas` local storage model
- search ranking pipeline
- MCP server skeleton
- REST server for debugging
- local-first privacy story

The graph is not gone forever. Replace the code call graph with an optional **document structure graph**:

```text
Document A cites Document B
Section 4 references Appendix C
Markdown file links to another note
PDF page references Figure 3
```

But do not start there. Start with flat semantic search. The structure layer comes later.

---

## The new primitive: document extractors

Today compas has language chunkers. compas-docs needs **document extractors** and **document chunkers**.

```rust
pub trait DocumentExtractor {
    fn supports(&self, path: &Path) -> bool;
    fn extract(&self, path: &Path) -> Result<ExtractedDocument>;
}

pub struct ExtractedDocument {
    pub title: Option<String>,
    pub author: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub pages: Vec<DocumentPage>,
    pub raw_text: String,
}

pub struct DocumentPage {
    pub page_number: Option<usize>,
    pub text: String,
}
```

Chunking becomes independent of extraction:

```rust
pub trait DocumentChunker {
    fn chunk(&self, document: ExtractedDocument) -> Vec<DocumentChunk>;
}

pub struct DocumentChunk {
    pub id: String,
    pub document_id: String,
    pub file_path: PathBuf,
    pub file_name: String,
    pub title: Option<String>,
    pub heading_path: Vec<String>,
    pub page_number: Option<usize>,
    pub text: String,
    pub enriched_text: String,
    pub metadata: DocumentMetadata,
}
```

### Enriched text

For code, compas uses:

```text
{doc_comments}
{filename} {symbol}
{source_code}
```

For documents, use:

```text
Document: Annual Report 2024.pdf
Section: Risk Factors > Supply Chain
Page: 37

{chunk_text}
```

This gives the embedding model context that raw chunk text alone does not provide.

---

## Supported file types

### Phase 1

| Type | Notes |
|---|---|
| `.md` | heading-aware chunking |
| `.txt` | paragraph chunking |
| `.pdf` | text-selectable PDFs only |

### Phase 2

| Type | Notes |
|---|---|
| `.docx` | unzip + parse XML |
| `.html` | strip tags, structure from headings |
| `.rtf` | basic text extraction |

### Phase 3

| Type | Notes |
|---|---|
| Scanned PDFs | OCR via Apple Vision or Tesseract |
| Apple Notes export | `.enex` or `.html` |
| Notion export | Markdown + media |
| Obsidian vaults | Markdown with wikilinks |
| Email archives | `.mbox`, `.eml` |

### Rust crates / approaches

| Format | Library |
|---|---|
| Markdown | `pulldown-cmark` or heading-aware splitter |
| TXT | direct read |
| PDF | `pdf-extract`, `lopdf`, or `pdfium-render` |
| DOCX | `docx-rs` or unzip + XML parse |
| HTML | `scraper`, `html2text` |
| OCR | Apple Vision framework (macOS only), Tesseract |

Start with text PDFs only. Do not begin with OCR.

---

## Chunking strategy

Chunking quality directly determines search quality. Bad chunks make semantic search feel broken.

### Markdown

Chunk by heading hierarchy:

```markdown
# Title
## Section
### Subsection
body text...
```

Each chunk carries its full heading path: `["Title", "Section", "Subsection"]`.

### DOCX

Use Word heading styles (`Heading 1`, `Heading 2`, `Heading 3`) to determine chunk boundaries.

### PDF

PDFs often have no structure. Strategy:

- split by page first
- then by paragraph boundaries within pages
- target chunk size: **500–900 tokens**
- overlap between chunks: **80–120 tokens**
- always preserve page number in metadata

Every PDF result must say: `Annual Report 2024.pdf, page 37`. Search without citation feels untrustworthy.

---

## Storage model

Each chunk payload in Qdrant:

```json
{
  "chunk_id": "sha256-hash",
  "document_id": "sha256-of-path",
  "file_path": "/Users/alex/Documents/Annual Report 2024.pdf",
  "file_name": "Annual Report 2024.pdf",
  "extension": "pdf",
  "title": "Annual Report 2024",
  "page_start": 37,
  "page_end": 38,
  "heading_path": ["Chapter 2", "Risk Factors", "Supply Chain"],
  "modified_at": "2026-05-24T10:20:00Z",
  "content_hash": "...",
  "text": "actual chunk text",
  "preview": "short snippet for UI display"
}
```

This metadata enables:

- filter by extension
- filter by folder
- filter by date range
- group results by document
- link to exact page/section
- deduplicate many chunks from the same large document

---

## Search ranking

Combine semantic score with post-search boosts:

| Signal | Boost |
|---|---|
| Query term in file name | `+0.10` |
| Query term in title | `+0.12` |
| Query term in heading path | `+0.10` |
| Exact keyword match in chunk text | `+0.05` |
| Recently modified document | small optional boost |
| Same document repeated more than 3× | cap + diversity penalty |

### Result grouping

If a 300-page PDF produces 8 of the top 10 chunks, results should not feel like noise. Group by document:

```text
1. Annual Report 2024.pdf
   Best matches:
   ├── Page 37 — Supply chain risk
   ├── Page 44 — Vendor concentration
   └── Page 51 — Insurance coverage

2. Vendor Agreement - Acme Corp.pdf
   Best matches:
   └── Page 12 — Termination without cause
```

Not ten separate undifferentiated cards.

---

## The macOS app

Two main implementation paths:

### Option A — Tauri app

```text
Rust backend + web UI frontend (React / Svelte)
```

**Pros:**
- reuses Rust code directly — no FFI boundary
- easy to bundle the entire indexing and search engine
- cross-platform if needed later
- faster initial development

**Cons:**
- not fully native macOS feel
- PDF preview and permission handling may need extra care

### Option B — SwiftUI app + Rust core

```text
SwiftUI frontend + Rust indexing/search library or bundled subprocess
```

**Pros:**
- native macOS feel
- best integration with Finder, PDFKit, Quick Look, Vision OCR
- easier to pursue App Store with sandbox + security-scoped bookmarks

**Cons:**
- more integration work at the Rust/Swift boundary
- slower initial development

### Recommendation

> Build the first version with **Tauri** unless the native macOS feel is the whole product.  
> If users love it, build a polished SwiftUI shell later.

A practical long-term split:

```text
compas-docs-core   — Rust library / CLI (no UI dependency)
compas-docs-macos  — thin Tauri or SwiftUI shell around the core
```

The CLI must work without the UI. The app is a wrapper.

---

## MVP user experience

1. User opens app.
2. App asks: "Choose folders to index."
3. User selects `~/Documents`, an Obsidian vault, or a project docs folder.
4. App indexes locally. No network required.
5. User types natural language queries:

```text
tax documents mentioning foreign income
PDF where I saved the insurance policy number
notes about vector databases and local embeddings
contract clause about termination without cause
paper that talked about retrieval augmented generation
```

6. Results show:
   - file name
   - page / heading path
   - text snippet
   - open in original app
   - reveal in Finder
   - copy path or citation

### What the MVP does not have

The first version does **not** need chat or question answering.

The core value is:

> Find the right place in your documents instantly.

Chat comes after retrieval is excellent. Adding chat on top of mediocre retrieval produces mediocre answers with a chatbot face on them.

---

## MCP is still a major feature

Even as a macOS app, keep MCP. This turns compas-docs into a **private memory layer** for agents.

```json
{
  "tool": "search_documents",
  "arguments": {
    "query": "Find my prior notes about agent context management",
    "folder": "Documents",
    "limit": 10
  }
}
```

Useful MCP tools:

```text
search_documents         — semantic search across indexed folders
get_document_chunk       — retrieve a specific chunk by ID
open_document_location   — open a file at a specific page
list_indexed_folders     — what is currently indexed
reindex_documents        — trigger reindex of a folder
```

Optional later:

```text
summarize_document
compare_documents
answer_from_documents
```

The killer framing is the same one that made compas useful for code:

> Agents can search your local documents without being allowed to read all of them.

That is the context firewall applied to personal knowledge.

---

## Privacy model

The privacy story belongs front and center in all communications:

- all embeddings generated locally
- all index files stored locally in `.compas-docs/`
- no documents uploaded anywhere
- no API keys required
- no telemetry by default
- indexed folders explicitly chosen by the user
- easy "forget this folder" option
- easy "delete entire index" option

This matters because target documents are typically sensitive:

- tax filings
- contracts and legal agreements
- health records and insurance
- private notes and journals
- company internal documents
- client work

This is the primary differentiation from cloud RAG products. Local-first is not a constraint — it is the value proposition.

---

## Project structure

```text
compas-docs/
├── crates/
│   ├── compas_core/
│   │   ├── embedder/         # FastEmbed, unchanged from compas
│   │   ├── store/            # Qdrant Edge, unchanged
│   │   ├── indexer/          # file walk + hash + incremental
│   │   ├── search/           # ranking + dedup
│   │   └── models.rs
│   │
│   ├── compas_docs_core/
│   │   ├── extractors/
│   │   │   ├── mod.rs        # DocumentExtractor trait
│   │   │   ├── markdown.rs
│   │   │   ├── text.rs
│   │   │   ├── pdf.rs
│   │   │   └── docx.rs
│   │   ├── chunker.rs        # DocumentChunker trait
│   │   ├── indexer.rs        # document-specific indexing loop
│   │   └── models.rs         # DocumentChunk, DocumentMetadata
│   │
│   └── compas_docs_cli/
│       └── main.rs           # init, index, search, serve, mcp
│
├── apps/
│   └── macos/                # Tauri or SwiftUI shell
│
└── docs/
```

### CLI commands

```bash
compas-docs init
compas-docs index ~/Documents
compas-docs index ~/Documents/Projects --watch
compas-docs search "insurance policy renewal date"
compas-docs serve
compas-docs mcp
```

The CLI comes first. The app wraps it.

---

## Development roadmap

### Milestone 1 — CLI proof of concept

Goal: prove that search quality is worth packaging.

Scope:
- `.md`, `.txt`, text-based `.pdf`
- folder walk + content hashing
- incremental reindex (skip unchanged files)
- semantic search
- results include file path + page number
- no app, no MCP, no REST server yet

```bash
compas-docs index ~/Documents/test-corpus
compas-docs search "documents about local-first AI tools"
```

### Milestone 2 — Search quality

Add:
- heading-aware Markdown chunking
- better PDF paragraph boundaries
- result grouping by document
- file name / title / heading boosts
- snippets with matched terms highlighted
- diversity cap (max 3 results per document by default)

This is where the product starts feeling genuinely useful.

### Milestone 3 — macOS shell

Build a minimal app:
- choose folders to index
- indexing progress indicator
- search input
- grouped result list with snippets
- open file in default app
- reveal in Finder

Target feel: Raycast meets Spotlight with semantic understanding.

### Milestone 4 — MCP

Expose MCP tools:
- `search_documents`
- `get_document_chunk`
- `open_document_location`

Agents can now query the user's local document library as a tool.

### Milestone 5 — Richer formats

Add:
- `.docx`
- `.html`
- scanned PDF OCR (Apple Vision)
- Obsidian vault improvements (wikilink awareness)
- Apple Notes export

### Milestone 6 — Ask mode

Only after retrieval feels excellent, add citation-first Q&A:

```text
Q: What is the termination notice period in the Acme vendor contract?

A: According to "Vendor Agreement - Acme Corp.pdf", page 12 (Section 8.2):
   "Either party may terminate this agreement with 30 days written notice..."
```

The answer must always cite its source. No citation, no trust.

---

## What to build first

Build a minimal CLI inside or beside the current repo. Validate before packaging.

A first prototype only needs:

1. file walker (exists in compas)
2. Markdown extractor (pulldown-cmark + heading splitter)
3. TXT extractor (trivial)
4. PDF text extractor (pdf-extract crate)
5. document chunker (heading-aware / page-aware)
6. existing FastEmbed embedder (zero changes)
7. existing Qdrant Edge store (minor schema changes)
8. simplified search with grouping

Once `compas-docs search "my insurance policy renewal date"` returns the right page of the right PDF instantly — the app is obvious, and the MCP surface is obvious.

The prototype is probably **one or two weekends** without OCR and advanced DOCX.

---

## Positioning

Do not pitch this as:

> "A RAG app for your documents."

That category is crowded and shapeless.

Pitch it as:

| Framing | Why it works |
|---|---|
| **Private semantic Spotlight** | Familiar mental model, immediately understood |
| **Local-first search for what your files mean** | Separates from file-name search and cloud tools |
| **Ask your documents before opening them** | Mirrors the compas doctrine, memorable |

The last one is the strongest because it is the exact same philosophy applied to a new domain:

- For code: *Ask the codebase before opening files.*
- For documents: *Ask your library before opening files.*

Same invention. Bigger market.

---

## Relationship to compas-core

`compas-docs` is not a distraction from the larger vision. It is the **second data point that proves compas-core is real**.

- `compas-code` proves the idea works for structured, AST-parseable text (code).
- `compas-docs` proves the idea works for unstructured, human-written text (everything else).

If both verticals share 80% of the same engine, then `compas-core` is no longer a hypothesis. It is the product.

```text
compas-core
├── compas-code    → developers, AI agents, editors
└── compas-docs    → researchers, lawyers, analysts, writers, everyone
```

The graph layer — call graph for code, citation/reference graph for documents — becomes a per-vertical plugin. The embedding, storage, ranking, MCP surface, and local-first runtime are shared.

That is the substrate. That is the lightbulb.
