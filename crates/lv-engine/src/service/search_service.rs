use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use lv_core::uri::VikingUri;
use lv_core::CoreError;

use super::context::RequestContext;
use crate::llm::Embedder;
use crate::storage::Database;
use crate::storage::{LsOptions, VectorMatch, VikingFs};

/// Constants matching upstream HierarchicalRetriever.
const MAX_CONVERGENCE_ROUNDS: usize = 3;
const GLOBAL_SEARCH_TOPK: usize = 10;
const DIRECTORY_SCORE_THRESHOLD: f64 = 0.3;

/// Search service with hierarchical retrieval.
pub struct SearchService {
    viking_fs: VikingFs,
    embedder: Arc<dyn Embedder>,
}

impl SearchService {
    pub fn new(db: Arc<Database>, embedder: Arc<dyn Embedder>) -> Self {
        Self {
            viking_fs: VikingFs::new(db),
            embedder,
        }
    }

    /// Semantic search with hierarchical directory walk.
    pub async fn find(
        &self,
        query: &str,
        target_uri: Option<&str>,
        _ctx: &RequestContext,
        limit: usize,
        score_threshold: Option<f32>,
    ) -> Result<FindResult, CoreError> {
        let embedding = self.embedder.embed(query).await?;
        let scope = target_uri.unwrap_or("viking://");
        let threshold = score_threshold
            .map(|t| t as f64)
            .unwrap_or(DIRECTORY_SCORE_THRESHOLD);

        // 1. GLOBAL SEARCH -- get initial top-k matches
        let global_matches =
            self.viking_fs
                .vector_search(&embedding, scope, GLOBAL_SEARCH_TOPK * 2)?;

        // Collect all results in a map (uri -> best score)
        let mut results: HashMap<String, VectorMatch> = HashMap::new();
        let mut explored_dirs: HashSet<String> = HashSet::new();
        let mut dirs_to_explore: Vec<(String, f64)> = Vec::new();

        for m in &global_matches {
            results
                .entry(m.uri.clone())
                .and_modify(|existing| {
                    if m.score > existing.score {
                        *existing = m.clone();
                    }
                })
                .or_insert_with(|| m.clone());

            // Find parent directory to explore
            if let Ok(uri) = VikingUri::parse(&m.uri) {
                if let Some(parent) = uri.parent() {
                    let parent_str = parent.as_str().to_string();
                    if !explored_dirs.contains(&parent_str) {
                        dirs_to_explore.push((parent_str, m.score));
                    }
                }
            }
        }

        // 2. DIRECTORY WALK -- explore parent directories of top results
        let mut total_searched = global_matches.len() as u64;
        let mut rounds = 1u32;

        for _round in 0..MAX_CONVERGENCE_ROUNDS {
            if dirs_to_explore.is_empty() {
                break;
            }

            let current_dirs: Vec<(String, f64)> = std::mem::take(&mut dirs_to_explore);
            let mut found_new = false;

            for (dir_uri, _parent_score) in &current_dirs {
                if explored_dirs.contains(dir_uri) {
                    continue;
                }
                explored_dirs.insert(dir_uri.clone());

                // List children of this directory
                let parsed = match VikingUri::parse(dir_uri) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let children = self.viking_fs.ls(
                    &parsed,
                    &LsOptions {
                        node_limit: 50,
                        ..Default::default()
                    },
                )?;

                // Score each child via vector search
                for child in &children {
                    if results.contains_key(&child.uri) {
                        continue; // Already scored
                    }

                    // Search specifically for this child's URI
                    let child_matches = self.viking_fs.vector_search(&embedding, &child.uri, 1)?;
                    total_searched += 1;

                    if let Some(cm) = child_matches.into_iter().next() {
                        if cm.score >= threshold {
                            found_new = true;

                            // If this child is a directory, explore it next round
                            if !child.is_leaf {
                                dirs_to_explore.push((child.uri.clone(), cm.score));
                            }

                            results.insert(cm.uri.clone(), cm);
                        }
                    }
                }
            }

            rounds += 1;

            if !found_new {
                break; // Converged -- no new results found
            }
        }

        // 3. FINAL RANKING -- sort by score descending, take top N
        let mut sorted: Vec<VectorMatch> = results.into_values().collect();
        sorted.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(limit);

        Ok(FindResult {
            query: query.to_string(),
            resources: sorted,
            total_searched,
            rounds,
        })
    }
}

#[derive(Debug)]
pub struct FindResult {
    pub query: String,
    pub resources: Vec<VectorMatch>,
    pub total_searched: u64,
    pub rounds: u32,
}
