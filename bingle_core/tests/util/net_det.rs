use std::collections::HashSet;

use bingle_core::util::net_det::{NetDet, NetDetGraph};

fn set_eq(a: &HashSet<usize>, b: &HashSet<usize>) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(x))
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn depth_small_examples() {
    // For small N and b, just verify non-decreasing and reasonable values
    let cases = vec![
        (1usize, 2usize, 0usize),
        (2, 2, 1),
        (3, 2, 1),
        (4, 2, 1),
        (5, 2, 2),
        (9, 2, 2),
    ];
    for (n, b, expected_min) in cases {
        let g = NetDetGraph::new(n, b);
        let d = g.required_depth();
        assert!(
            d >= expected_min,
            "n={}, b={}, d={} < {}",
            n,
            b,
            d,
            expected_min
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn fill_edge_case_n1() {
    let mut g = NetDetGraph::new(1, 3);
    g.fill();
    assert!(!g.failed());
    assert_eq!(g.root(), Some(0));
    assert_eq!(g.mean_edges(), Some(1.0));
    assert!(g.variance_edges() >= 0.0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn fill_and_flood_small_graph() {
    let mut g = NetDetGraph::new(10, 3);
    g.fill();
    assert!(!g.failed());
    // Validate root exists
    let root = g.root().expect("root present");
    // Validate that all node indices are within range and relations are consistent
    for i in 0..g.number_nodes {
        if let Some(up) = g.get_up(i) {
            assert!(up < g.number_nodes);
        }
        for d in g.get_down(i) {
            assert!(d < g.number_nodes);
        }
    }

    // For any node, flood(start=start, forNode=start) should forward to its neighbors
    for i in 0..g.number_nodes {
        let res = g.flood(i, i);
        let mut exp = HashSet::new();
        if let Some(u) = g.get_up(i) {
            exp.insert(u);
        }
        for d in g.get_down(i) {
            exp.insert(d);
        }
        assert!(
            set_eq(&res, &exp),
            "node {}: res={:?} exp={:?}",
            i,
            res,
            exp
        );
    }

    // Spot check flood propagation: starting at root, after one hop we should forward to all neighbors
    let res_root = g.flood(root, root);
    assert!(!res_root.is_empty());
}
