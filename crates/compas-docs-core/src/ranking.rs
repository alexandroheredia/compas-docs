use crate::models::RankedHit;
use crate::sqlite::ChunkRow;
use std::collections::HashMap;

const OVERLAP_BOOST: f32 = 0.15;
const LITERAL_BOOST: f32 = 0.10;
const MAX_HYBRID_SCORE: f32 = 1.0 + 1.0 + OVERLAP_BOOST + LITERAL_BOOST;

pub fn merge_hybrid(
    exact: Vec<(String, f32)>,
    semantic: Vec<(String, f32)>,
    query: &str,
    metadata: &HashMap<String, ChunkRow>,
) -> Vec<RankedHit> {
    let mut exact_map: HashMap<String, f32> = exact.into_iter().collect();
    let semantic_map: HashMap<String, f32> = semantic.into_iter().collect();
    let mut chunk_ids: Vec<String> = exact_map.keys().cloned().collect();
    for chunk_id in semantic_map.keys() {
        if !exact_map.contains_key(chunk_id) {
            chunk_ids.push(chunk_id.clone());
        }
    }

    let query_lower = query.to_lowercase();
    let mut ranked = Vec::with_capacity(chunk_ids.len());
    for chunk_id in chunk_ids {
        let exact_score = exact_map.remove(&chunk_id).unwrap_or(0.0);
        let semantic_score = semantic_map.get(&chunk_id).copied().unwrap_or(0.0);
        let overlap_boost = if exact_score > 0.0 && semantic_score > 0.0 {
            OVERLAP_BOOST
        } else {
            0.0
        };
        let literal_boost = metadata
            .get(&chunk_id)
            .map(|row| {
                let title = row.title.to_lowercase();
                let path = row.relative_path.to_lowercase();
                if !query_lower.is_empty()
                    && (title.contains(&query_lower) || path.contains(&query_lower))
                {
                    LITERAL_BOOST
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);

        let combined_score = exact_score + semantic_score + overlap_boost + literal_boost;

        ranked.push(RankedHit {
            chunk_id,
            score: (combined_score / MAX_HYBRID_SCORE).clamp(0.0, 1.0),
        });
    }

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.chunk_id.cmp(&b.chunk_id))
    });
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_search_boosts_overlap_hits() {
        let metadata = HashMap::from([
            (
                "overlap".to_string(),
                ChunkRow {
                    chunk_id: "overlap".to_string(),
                    folder_id: "folder-1".to_string(),
                    document_id: "doc-1".to_string(),
                    absolute_path: "/tmp/policy.md".to_string(),
                    relative_path: "policy.md".to_string(),
                    file_name: "policy.md".to_string(),
                    title: "Policy".to_string(),
                    heading_path: Vec::new(),
                    page_start: None,
                    page_end: None,
                    preview: "preview".to_string(),
                    text: "text".to_string(),
                    embedding: vec![1.0],
                    updated_at: 0,
                },
            ),
            (
                "semantic-only".to_string(),
                ChunkRow {
                    chunk_id: "semantic-only".to_string(),
                    folder_id: "folder-1".to_string(),
                    document_id: "doc-1".to_string(),
                    absolute_path: "/tmp/semantic.md".to_string(),
                    relative_path: "semantic.md".to_string(),
                    file_name: "semantic.md".to_string(),
                    title: "Semantic".to_string(),
                    heading_path: Vec::new(),
                    page_start: None,
                    page_end: None,
                    preview: "preview".to_string(),
                    text: "text".to_string(),
                    embedding: vec![1.0],
                    updated_at: 0,
                },
            ),
        ]);

        let ranked = merge_hybrid(
            vec![("overlap".to_string(), 0.5)],
            vec![
                ("overlap".to_string(), 0.5),
                ("semantic-only".to_string(), 0.8),
            ],
            "policy",
            &metadata,
        );

        assert_eq!(ranked[0].chunk_id, "overlap");
    }

    #[test]
    fn hybrid_scores_are_normalized() {
        let metadata = HashMap::from([(
            "overlap".to_string(),
            ChunkRow {
                chunk_id: "overlap".to_string(),
                folder_id: "folder-1".to_string(),
                document_id: "doc-1".to_string(),
                absolute_path: "/tmp/policy.md".to_string(),
                relative_path: "policy.md".to_string(),
                file_name: "policy.md".to_string(),
                title: "policy".to_string(),
                heading_path: Vec::new(),
                page_start: None,
                page_end: None,
                preview: "preview".to_string(),
                text: "text".to_string(),
                embedding: vec![1.0],
                updated_at: 0,
            },
        )]);

        let ranked = merge_hybrid(
            vec![("overlap".to_string(), 1.0)],
            vec![("overlap".to_string(), 1.0)],
            "policy",
            &metadata,
        );

        assert_eq!(ranked[0].score, 1.0);
    }
}
