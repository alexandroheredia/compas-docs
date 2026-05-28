#!/usr/bin/env python3
"""
Evaluate document-search quality against a real indexed folder.

The script uses `compas search --format json --path <folder>` so it measures the
same document-library search path the current app uses.

Usage:
    python3 scripts/evaluate_document_search.py \
      --binary ./target/release/compas \
      --cases scripts/document_eval_cases.example.json

Optional:
    python3 scripts/evaluate_document_search.py \
      --binary ./target/release/compas \
      --cases my-cases.json \
      --json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import pathlib
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from dataclasses import dataclass
from typing import Any


DEFAULT_LIMIT = 10
DEFAULT_LOG_DIR = pathlib.Path(".evals/document-search")

DIFFICULTY_WEIGHT = {
    "easy": 1.0,
    "medium": 1.25,
    "hard": 1.5,
}


@dataclass
class EvalCase:
    name: str
    query: str
    expected_files: list[str]
    domain: str | None
    language: str | None
    difficulty: str
    description: str | None = None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, help="Path to compas binary")
    parser.add_argument("--cases", required=True, help="Path to evaluation cases JSON")
    parser.add_argument("--config", help="Optional compas config path")
    parser.add_argument(
        "--log-dir",
        help="Directory for persisted evaluation run logs (default: .evals/document-search)",
    )
    parser.add_argument(
        "--no-log",
        action="store_true",
        help="Do not persist this evaluation run to the run-log directory",
    )
    parser.add_argument("--json", action="store_true", help="Print machine-readable summary")
    return parser.parse_args()


def load_cases(path: pathlib.Path) -> tuple[pathlib.Path, int, list[EvalCase]]:
    payload = json.loads(path.read_text())
    folder = pathlib.Path(payload["folder"]).expanduser()
    if not folder.is_absolute():
        folder = (path.parent / folder).resolve()
    limit = int(payload.get("limit", DEFAULT_LIMIT))
    cases = [
        EvalCase(
            name=item["name"],
            query=item["query"],
            expected_files=item["expected_files"],
            domain=item.get("domain"),
            language=item.get("language"),
            difficulty=item.get("difficulty", "medium"),
            description=item.get("description"),
        )
        for item in payload["cases"]
    ]
    return folder, limit, cases


def run_search(
    binary: pathlib.Path,
    folder: pathlib.Path,
    query: str,
    limit: int,
    config_path: pathlib.Path | None,
) -> dict[str, Any]:
    command = [
        str(binary),
    ]
    if config_path is not None:
        command.extend(["--config", str(config_path)])
    command.extend([
        "search",
        "--path",
        str(folder),
        "--limit",
        str(limit),
        "--format",
        "json",
        query,
    ])
    start = time.perf_counter()
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    elapsed_ms = (time.perf_counter() - start) * 1000.0
    if result.returncode != 0:
        stderr = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"search command failed: {stderr}")
    payload = json.loads(result.stdout)
    payload["latency_ms"] = round(elapsed_ms, 1)
    return payload


def file_sha256(path: pathlib.Path | None) -> str | None:
    if path is None or not path.exists() or not path.is_file():
        return None
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            hasher.update(chunk)
    return hasher.hexdigest()


def repo_root() -> pathlib.Path:
    return pathlib.Path(__file__).resolve().parent.parent


def git_commit(workdir: pathlib.Path) -> str | None:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=workdir,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        return None
    return result.stdout.strip() or None


def binary_version(binary: pathlib.Path) -> str | None:
    result = subprocess.run(
        [str(binary), "--help"],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        return None
    first_line = result.stdout.splitlines()[0] if result.stdout else ""
    return first_line or None


def build_run_metadata(
    *,
    binary: pathlib.Path,
    cases_path: pathlib.Path,
    config_path: pathlib.Path | None,
    folder: pathlib.Path,
    limit: int,
) -> dict[str, Any]:
    root = repo_root()
    timestamp = datetime.now(timezone.utc).replace(microsecond=0)
    return {
        "timestamp_utc": timestamp.isoformat().replace("+00:00", "Z"),
        "repo_root": str(root),
        "git_commit": git_commit(root),
        "binary": {
            "path": str(binary.resolve()),
            "sha256": file_sha256(binary),
            "banner": binary_version(binary),
        },
        "cases": {
            "path": str(cases_path.resolve()),
            "sha256": file_sha256(cases_path),
        },
        "config": {
            "path": str(config_path.resolve()) if config_path else None,
            "sha256": file_sha256(config_path),
        },
        "corpus": {
            "path": str(folder.resolve()),
        },
        "limit": limit,
        "environment": {
            "COMPAS_DOCS_HOME": os.environ.get("COMPAS_DOCS_HOME"),
            "COMPAS_FASTEMBED_BATCH_SIZE": os.environ.get("COMPAS_FASTEMBED_BATCH_SIZE"),
            "OMP_NUM_THREADS": os.environ.get("OMP_NUM_THREADS"),
            "CODEINDEX__EMBEDDER__PROVIDER": os.environ.get(
                "CODEINDEX__EMBEDDER__PROVIDER"
            ),
            "CODEINDEX__EMBEDDER__MODEL": os.environ.get("CODEINDEX__EMBEDDER__MODEL"),
            "CODEINDEX__EMBEDDER__QUERY_PREFIX": os.environ.get(
                "CODEINDEX__EMBEDDER__QUERY_PREFIX"
            ),
            "CODEINDEX__EMBEDDER__DOC_PREFIX": os.environ.get(
                "CODEINDEX__EMBEDDER__DOC_PREFIX"
            ),
        },
    }


def persist_run(log_dir: pathlib.Path, payload: dict[str, Any]) -> pathlib.Path:
    log_dir.mkdir(parents=True, exist_ok=True)
    timestamp = payload["run"]["timestamp_utc"].replace(":", "-")
    run_path = log_dir / f"{timestamp}.json"
    run_path.write_text(json.dumps(payload, indent=2) + "\n")

    latest_path = log_dir / "latest.json"
    latest_path.write_text(json.dumps(payload, indent=2) + "\n")

    index_path = log_dir / "index.jsonl"
    index_entry = {
        "timestamp_utc": payload["run"]["timestamp_utc"],
        "run_file": run_path.name,
        "git_commit": payload["run"].get("git_commit"),
        "binary_sha256": payload["run"]["binary"].get("sha256"),
        "cases_sha256": payload["run"]["cases"].get("sha256"),
        "config_sha256": payload["run"]["config"].get("sha256"),
        "embedder_provider": payload["run"]["environment"].get(
            "CODEINDEX__EMBEDDER__PROVIDER"
        ),
        "embedder_model": payload["run"]["environment"].get(
            "CODEINDEX__EMBEDDER__MODEL"
        ),
        "embedder_query_prefix": payload["run"]["environment"].get(
            "CODEINDEX__EMBEDDER__QUERY_PREFIX"
        ),
        "embedder_doc_prefix": payload["run"]["environment"].get(
            "CODEINDEX__EMBEDDER__DOC_PREFIX"
        ),
        "summary": payload["summary"],
    }
    with index_path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(index_entry) + "\n")

    return run_path


def normalize_path(value: str) -> str:
    return value.replace("\\", "/")


def score_case(case: EvalCase, payload: dict[str, Any]) -> dict[str, Any]:
    results = payload.get("results", [])
    deduped_results = dedupe_by_file_path(results)
    found_files = [normalize_path(item["file_path"]) for item in deduped_results]
    matched_ranks: list[int] = []
    expected_hits: list[dict[str, Any]] = []

    for expected in case.expected_files:
        expected_norm = normalize_path(expected)
        rank = None
        for index, file_path in enumerate(found_files, start=1):
            if file_path == expected_norm or file_path.endswith(expected_norm):
                rank = index
                matched_ranks.append(index)
                break
        expected_hits.append(
            {
                "file": expected,
                "found": rank is not None,
                "rank": rank,
            }
        )

    recall = len(matched_ranks) / max(len(case.expected_files), 1)
    top1 = 1.0 if 1 in matched_ranks else 0.0
    mrr = 0.0 if not matched_ranks else max(1.0 / rank for rank in matched_ranks)
    ndcg = dcg(matched_ranks) / dcg(list(range(1, len(case.expected_files) + 1)))
    score = (0.45 * recall) + (0.20 * top1) + (0.20 * mrr) + (0.15 * ndcg)

    return {
        "name": case.name,
        "query": case.query,
        "domain": case.domain,
        "language": case.language,
        "difficulty": case.difficulty,
        "description": case.description,
        "latency_ms": payload["latency_ms"],
        "result_count": len(results),
        "unique_result_count": len(deduped_results),
        "top_results": [
            {
                "file_path": item["file_path"],
                "title": item["title"],
                "score": item["score"],
            }
            for item in deduped_results[:5]
        ],
        "expected_hits": expected_hits,
        "metrics": {
            "recall": round(recall, 3),
            "top1": round(top1, 3),
            "mrr": round(mrr, 3),
            "ndcg": round(ndcg, 3),
            "score": round(score, 3),
        },
    }


def dcg(ranks: list[int]) -> float:
    value = 0.0
    for rank in ranks:
        value += 1.0 / math.log2(rank + 1)
    return value


def dedupe_by_file_path(results: list[dict[str, Any]]) -> list[dict[str, Any]]:
    deduped = []
    seen = set()
    for item in results:
        path = normalize_path(item["file_path"])
        if path in seen:
            continue
        seen.add(path)
        deduped.append(item)
    return deduped


def summarize(cases: list[dict[str, Any]]) -> dict[str, Any]:
    overall_score = statistics.mean(case["metrics"]["score"] for case in cases)
    overall_latency = statistics.mean(case["latency_ms"] for case in cases)

    by_difficulty: dict[str, list[dict[str, Any]]] = {"easy": [], "medium": [], "hard": []}
    for case in cases:
        by_difficulty.setdefault(case["difficulty"], []).append(case)

    difficulty_summary = {}
    for difficulty, items in by_difficulty.items():
        if not items:
            continue
        difficulty_summary[difficulty] = {
            "count": len(items),
            "score": round(statistics.mean(item["metrics"]["score"] for item in items), 3),
            "recall": round(statistics.mean(item["metrics"]["recall"] for item in items), 3),
            "top1": round(statistics.mean(item["metrics"]["top1"] for item in items), 3),
        }

    by_domain: dict[str, list[dict[str, Any]]] = {}
    for case in cases:
        domain = case.get("domain") or "unlabeled"
        by_domain.setdefault(domain, []).append(case)

    domain_summary = {}
    for domain, items in sorted(by_domain.items()):
        domain_summary[domain] = {
            "count": len(items),
            "score": round(statistics.mean(item["metrics"]["score"] for item in items), 3),
            "recall": round(statistics.mean(item["metrics"]["recall"] for item in items), 3),
            "top1": round(statistics.mean(item["metrics"]["top1"] for item in items), 3),
        }

    by_language: dict[str, list[dict[str, Any]]] = {}
    for case in cases:
        language = case.get("language") or "unlabeled"
        by_language.setdefault(language, []).append(case)

    language_summary = {}
    for language, items in sorted(by_language.items()):
        language_summary[language] = {
            "count": len(items),
            "score": round(statistics.mean(item["metrics"]["score"] for item in items), 3),
            "recall": round(statistics.mean(item["metrics"]["recall"] for item in items), 3),
            "top1": round(statistics.mean(item["metrics"]["top1"] for item in items), 3),
        }

    weighted_total = 0.0
    weighted_divisor = 0.0
    for case in cases:
        weight = DIFFICULTY_WEIGHT.get(case["difficulty"], 1.0)
        weighted_total += case["metrics"]["score"] * weight
        weighted_divisor += weight

    return {
        "score": round(overall_score, 3),
        "weighted_score": round(weighted_total / max(weighted_divisor, 1.0), 3),
        "avg_latency_ms": round(overall_latency, 1),
        "by_difficulty": difficulty_summary,
        "by_domain": domain_summary,
        "by_language": language_summary,
    }


def print_human(summary: dict[str, Any], cases: list[dict[str, Any]]) -> None:
    print("=" * 72)
    print("DOCUMENT SEARCH EVALUATION")
    print("=" * 72)
    print()
    print(f"Overall score:   {summary['score']:.3f}")
    print(f"Weighted score:  {summary['weighted_score']:.3f}")
    print(f"Avg latency:     {summary['avg_latency_ms']:.1f}ms")
    print()

    for difficulty in ["easy", "medium", "hard"]:
        row = summary["by_difficulty"].get(difficulty)
        if not row:
            continue
        print(
            f"{difficulty:>6}: score={row['score']:.3f}  recall={row['recall']:.3f}  "
            f"top1={row['top1']:.3f}  cases={row['count']}"
        )
    print()

    for domain, row in summary["by_domain"].items():
        print(
            f"{domain:>8}: score={row['score']:.3f}  recall={row['recall']:.3f}  "
            f"top1={row['top1']:.3f}  cases={row['count']}"
        )
    print()

    for language, row in summary["by_language"].items():
        print(
            f"{language:>8}: score={row['score']:.3f}  recall={row['recall']:.3f}  "
            f"top1={row['top1']:.3f}  cases={row['count']}"
        )
    print()

    for case in cases:
        score = case["metrics"]["score"]
        mark = "OK" if score >= 0.70 else "WARN"
        domain = case.get("domain") or "unlabeled"
        language = case.get("language") or "unlabeled"
        print(f"[{mark}] {case['name']} ({domain}, {language}, {case['difficulty']})")
        print(f"  Query:   {case['query']}")
        print(
            f"  Score:   {score:.3f}  Recall: {case['metrics']['recall']:.3f}  "
            f"MRR: {case['metrics']['mrr']:.3f}  Latency: {case['latency_ms']:.1f}ms"
        )
        for hit in case["expected_hits"]:
            rank = hit["rank"] if hit["rank"] is not None else "not found"
            print(f"  Expect:  {hit['file']} -> {rank}")
        for result in case["top_results"][:3]:
            print(
                f"  Top:     {result['file_path']}  "
                f"(score={result['score']:.3f}, title={result['title']})"
            )
        print()

    print("Targets:")
    print("  score >= 0.70  usable baseline")
    print("  score >= 0.80  good quality")
    print("  score >= 0.90  strong quality")


def main() -> int:
    args = parse_args()
    binary = pathlib.Path(args.binary).expanduser()
    cases_path = pathlib.Path(args.cases).expanduser()
    config_path = pathlib.Path(args.config).expanduser() if args.config else None
    log_dir = pathlib.Path(args.log_dir).expanduser() if args.log_dir else (repo_root() / DEFAULT_LOG_DIR)
    folder, limit, cases = load_cases(cases_path)

    if not binary.exists():
        raise SystemExit(f"binary not found: {binary}")
    if config_path is not None and not config_path.exists():
        raise SystemExit(f"config not found: {config_path}")
    if not folder.exists():
        raise SystemExit(f"folder not found: {folder}")

    scored_cases = []
    for case in cases:
        payload = run_search(binary, folder, case.query, limit, config_path)
        scored_cases.append(score_case(case, payload))

    summary = summarize(scored_cases)
    run_metadata = build_run_metadata(
        binary=binary,
        cases_path=cases_path,
        config_path=config_path,
        folder=folder,
        limit=limit,
    )
    output = {
        "run": run_metadata,
        "folder": str(folder),
        "limit": limit,
        "summary": summary,
        "cases": scored_cases,
    }

    run_path = None
    if not args.no_log:
        run_path = persist_run(log_dir, output)

    if args.json:
        print(json.dumps(output, indent=2))
    else:
        print_human(summary, scored_cases)
        if run_path is not None:
            print()
            print(f"Run log: {run_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
