//! PageRank by power iteration over the symbol edge list.
//!
//! Hand-rolled rather than pulling a graph library: it's ~30 lines, needs no
//! separate graph structure (we already have edges as id pairs), and keeps the
//! damping / dangling-node handling as tunable knobs — the calibration a real
//! graph needs that a black-box call hides.

use std::collections::HashMap;

use skeletree_core::SymbolId;

/// Classic PageRank. `damping` is the follow-a-link probability (0.85 is
/// standard); `iters` power-iteration steps (~20 converges for these sizes).
/// Returns `(symbol, score)` for every node; scores sum to ~1.
pub fn page_rank(
    nodes: &[SymbolId],
    edges: &[(SymbolId, SymbolId)],
    damping: f64,
    iters: usize,
) -> Vec<(SymbolId, f64)> {
    let n = nodes.len();
    if n == 0 {
        return Vec::new();
    }

    let index: HashMap<SymbolId, usize> = nodes.iter().enumerate().map(|(i, &s)| (s, i)).collect();
    let mut out_degree = vec![0usize; n];
    let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); n]; // incoming[i] = sources linking to i

    for &(src, dst) in edges {
        let (Some(&u), Some(&v)) = (index.get(&src), index.get(&dst)) else {
            continue; // edge referencing an unknown node
        };
        out_degree[u] += 1;
        incoming[v].push(u);
    }

    let base = 1.0 / n as f64;
    let mut rank = vec![base; n];
    for _ in 0..iters {
        // Rank held by dangling nodes (no out-links) is spread across all nodes.
        let dangling: f64 = (0..n)
            .filter(|&i| out_degree[i] == 0)
            .map(|i| rank[i])
            .sum();
        let mut next = vec![0.0; n];
        for i in 0..n {
            let inflow: f64 = incoming[i]
                .iter()
                .map(|&j| rank[j] / out_degree[j] as f64)
                .sum();
            next[i] = (1.0 - damping) * base + damping * (inflow + dangling * base);
        }
        rank = next;
    }

    nodes.iter().copied().zip(rank).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: i64) -> SymbolId {
        SymbolId(n)
    }

    #[test]
    fn central_node_ranks_highest() {
        // a -> b, a -> c, b -> c : everything points toward c.
        let nodes = [id(1), id(2), id(3)];
        let edges = [(id(1), id(2)), (id(1), id(3)), (id(2), id(3))];
        let ranks: HashMap<SymbolId, f64> =
            page_rank(&nodes, &edges, 0.85, 40).into_iter().collect();

        assert!(ranks[&id(3)] > ranks[&id(1)]);
        assert!(ranks[&id(3)] > ranks[&id(2)]);
        // Scores form a probability distribution.
        let total: f64 = ranks.values().sum();
        assert!((total - 1.0).abs() < 1e-6, "ranks sum to {total}");
    }

    #[test]
    fn empty_graph_is_empty() {
        assert!(page_rank(&[], &[], 0.85, 20).is_empty());
    }
}
