# The Lightbulb

> A strategic re-reading of what compas actually is, and where it could go.

---

## 1. What you think you built

A semantic code search engine for AI agents. It saves ~30% of tokens during code exploration by letting the agent **ask** instead of **read**.

That framing is correct, but small. It describes the *first customer*, not the *invention*.

## 2. What you actually built

Strip compas of the word "code" and look at the primitives:

1. **A pluggable chunker** that turns a large, structured corpus into semantic units, preserving the structure (AST → symbol graph).
2. **A local-first embedded vector store** (Qdrant Edge) — no cloud, no API keys, no data leaving the machine.
3. **A relationship graph** layered on top of the vector index ("who calls who"), making results *navigable*, not just *retrievable*.
4. **An MCP surface** that exposes both as first-class tools an agent reaches for *before* it reaches for `cat`/`read`.
5. **A token-economy thesis**: every file an agent opens that it didn't need to open is a bug. The index is the fix.

None of those five things are intrinsically about code. They're about **any large, structured, hyperlinked corpus that an AI agent is currently brute-force reading into its context window.**

That's the lightbulb.

## 3. The reframe

> **compas is not a code search tool. It's a context firewall.**
>
> It's the missing layer between "agents" and "any structured corpus too large to fit in context." Code is vertical #1. There are at least four more verticals with bigger TAM and harder moats.

The codebase use case is actually the *least* defensible: GitHub, Sourcegraph, Cursor, and every IDE vendor will eventually ship something similar. The verticals below are wide open, regulated, and starved of agent-native tooling.

---

## 4. The four high-impact pivots (ranked)

### Pivot A — **compas for Contracts & Legal Discovery** 🥇

**The corpus:** Master service agreements, NDAs, M&A data rooms, regulatory filings, case law. A typical mid-market M&A diligence room is 5,000–50,000 PDFs. A single MSA is a graph of cross-referenced clauses ("subject to Section 4.2", "as defined in Exhibit B").

**The shape match:**
- "Clauses" are your "symbols."
- Cross-references are your "call graph."
- Defined terms are your "imports."
- "Who else references the indemnification cap?" is `get_symbol_graph("Section 8.1")`.

**Why it's a 10x lightbulb:**
- Legal AI is exploding (Harvey, Hebbia, Ironclad) but every existing player is cloud-hosted. Law firms are **terrified** of uploading client docs to OpenAI.
- Local-first is not a feature, it's a **regulatory requirement** (privilege, GDPR, attorney work-product doctrine).
- A junior associate currently burns 4 hours reading every clause to find conflicts. An agent + compas-for-legal does it in 4 minutes.
- Billable-hour economics make ROI trivial to justify. You can charge per-seat what a lawyer bills in 20 minutes.

**Effort to pivot:** Swap the Tree-sitter chunker for a PDF/DOCX clause parser (LexNLP, spaCy, or a fine-tuned segmenter). The graph, embeddings, MCP layer, and token-economy story stay 100% intact.

---

### Pivot B — **compas for Incident Response / Observability** 🥈

**The corpus:** Logs, distributed traces, metrics, runbooks, past post-mortems. During an incident, an on-call SRE's agent currently slurps gigabytes of log lines into context and falls over.

**The shape match:**
- Spans are symbols.
- Parent/child span relationships are the call graph (literally — traces *are* call graphs).
- "Who called this failing service in the last 5 minutes?" is the same primitive.
- Past post-mortems become a semantic retrieval target: "have we seen this signature before?"

**Why it's a lightbulb:**
- Datadog, New Relic, and Honeycomb all charge per-GB ingested. Their "AI assistants" make that worse by re-reading everything.
- Local-first / VPC-resident is the only way Fortune 500 SREs will let an agent near production logs.
- The graph already exists (OpenTelemetry traces) — compas just needs to index it.
- Incident MTTR is a CFO-visible metric. Selling "we cut your incidents from 45min to 12min" is a cold-email any CTO opens.

**Effort to pivot:** New chunker for OTLP/JSON log lines + span tree. Bigger lift than legal (streaming, retention), but the architecture survives.

---

### Pivot C — **compas for Healthcare / EHR** 🥉

**The corpus:** A single patient's longitudinal record: notes, labs, imaging reports, medication history, prior auths. Often 500+ documents over 20 years.

**The shape match:**
- Encounters are symbols.
- Problem-list entries reference back to the encounters that introduced them — that's a graph.
- Medications cite indications, which cite diagnoses — another graph.
- "When did this patient first mention chest pain?" is exactly `search_codebase`.

**Why it's a lightbulb:**
- HIPAA makes cloud RAG a non-starter for most providers. **Local-first is the entire product.**
- Clinicians spend 30–50% of their day in the EHR. Any tool that compresses chart review is gold.
- Biggest TAM of any pivot here. Hardest regulatory lift.

**Effort to pivot:** FHIR/HL7 chunker, BAA-compliant deployment story. Slower go-to-market, but the deepest moat.

---

### Pivot D — **compas for Data Teams (dbt / SQL lineage)**

**The corpus:** A dbt project. Hundreds of models, each referencing others via `{{ ref() }}`. Plus the warehouse schema (thousands of columns).

**The shape match:**
- Models are symbols. `ref()` calls are the call graph. **The graph is literally already named "lineage" in dbt-speak.**
- "What downstream models break if I change `dim_users.email`?" is `get_symbol_graph`.

**Why it's a lightbulb:**
- Data agents (Hex, Julius, Cursor-for-data) are the fastest-growing AI vertical of 2025.
- dbt has a lineage graph but no MCP, no semantic search, and no token-aware retrieval. Agents currently `cat` every `.sql` file.
- Fastest possible time-to-ship: dbt already gives you the graph as JSON (`manifest.json`). You'd skip steps 1–3 of your current pipeline.

**Effort to pivot:** Trivially small. **This is the cheapest experiment to validate the thesis before betting on legal or healthcare.**

---

## 5. The meta-lightbulb (the real one)

Each pivot above is a vertical. But the *real* invention is horizontal:

> **compas is the SDK for building agent-native, local-first, graph-aware indexes over any structured corpus.**

Imagine the project restructured as:

```
compas-core/        # the index, graph, MCP server, token-economy primitives
compas-code/        # today's product (chunker for code)
compas-legal/       # clause chunker + cross-reference graph
compas-otel/        # span chunker + trace graph
compas-fhir/        # encounter chunker + problem-list graph
compas-dbt/         # model chunker + lineage graph (built in a weekend)
```

Each vertical is a chunker + a graph extractor. **The hard parts you already built — incremental indexing, MCP surface, ranking, dedup, local embeddings, the "ask don't read" doctrine — are reused verbatim.**

This is the same structural move that made:
- **Elasticsearch** (started as a recipe search → became the indexing substrate for everything)
- **LangChain** (started as GPT-3 wrappers → became the agent framework)
- **DuckDB** (started as in-process analytics → became the local-first OLAP substrate)
- **SQLite** (started as a Navy missile control DB → became the most-deployed DB on Earth)

Each of those had a "we built it for X, then realized it was a substrate for everything that looks like X" moment. **You're standing in that moment right now.**

---

## 6. Concrete next step (one week, low risk)

Don't pivot. **Validate the thesis without abandoning code:**

1. Refactor `Chunker` and the graph extractor into a clean trait boundary in `compas-core`.
2. Ship `compas-dbt` as a second chunker over a weekend (manifest.json → done).
3. Post the dbt version on Hacker News / r/dataengineering with the headline:
   > *"I built a local-first semantic index for code. Then I pointed it at dbt and it worked unchanged."*
4. Watch which audience reacts harder — coders or data folks. **That's your signal for which vertical is pulling.**

If the data folks pull, the legal/healthcare pivots become a fundable narrative ("we proved the substrate works across domains"). If they don't, you've still cleanly modularized your code and lost nothing.

---

## 7. The pitch, rewritten

**Before (today):**
> compas is a local semantic search engine for your codebase.

**After (the lightbulb):**
> compas is the context firewall for AI agents. It turns any structured corpus — code, contracts, traces, charts, queries — into a local, graph-aware index that agents query instead of read. Because every file an agent opens that it didn't need to open is a bug.

That second sentence is a category, not a product.
