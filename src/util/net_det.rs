// Deterministic Network (NetDet)
// Implementation translated from Kotlin reference (NetDet.kt) and BINGLE_SPEC.md
// Graph model: two directed trees (upper and lower) connected via roots and an optional middle row.
// Each node has at most one `up` edge and zero or more `down` edges.

use std::collections::{HashSet};

/// Public trait for Deterministic Network (NetDet) operations
pub trait NetDet {
    /// Build the network structure according to number_nodes and tree_order
    fn fill(&mut self);
    /// Compute required depth according to the spec
    fn required_depth(&self) -> usize;
    /// Compute the next-hop set for a flood that originated at `start_origin`,
    /// when executed at `for_node`.
    fn flood(&self, start_origin: usize, for_node: usize) -> HashSet<usize>;
    /// Mean number of edges per node (down edges + 1), or None if fill failed
    fn mean_edges(&self) -> Option<f64>;
    /// Variance of edges per node, or +inf if fill failed
    fn variance_edges(&self) -> f64;
    /// True if the last fill failed
    fn failed(&self) -> bool;
    /// Return the top root node index, if present
    fn root(&self) -> Option<usize>;
}

/// Concrete NetDet graph using index-based adjacency (no self-referential pointers)
#[derive(Debug, Clone)]
pub struct NetDetGraph {
    pub number_nodes: usize,
    pub tree_order: usize,
    fail: bool,
    root_node: Option<usize>,
    /// up[i] is Some(parent_index) or None
    up: Vec<Option<usize>>,
    /// down[i] is list of children indices
    down: Vec<Vec<usize>>,
}

impl NetDetGraph {
    pub fn new(number_nodes: usize, tree_order: usize) -> Self {
        NetDetGraph {
            number_nodes,
            tree_order,
            fail: false,
            root_node: None,
            up: Vec::new(),
            down: Vec::new(),
        }
    }

    /// Return the up neighbour of a node, if any
    pub fn get_up(&self, idx: usize) -> Option<usize> {
        self.up.get(idx).and_then(|o| *o)
    }

    /// Return a copy of the down neighbours list for a node
    pub fn get_down(&self, idx: usize) -> Vec<usize> {
        self.down.get(idx).cloned().unwrap_or_default()
    }

    fn clear(&mut self) {
        self.fail = true;
        self.root_node = None;
        self.up.clear();
        self.down.clear();
    }

    fn set_size(&mut self, n: usize) {
        self.up = vec![None; n];
        self.down = vec![Vec::new(); n];
    }

    fn sum_power(&self, n: usize) -> usize {
        // sum_{k=0..n-1} b^k
        // Using integer formula where possible; we use f64 to keep parity with Kotlin rounding.
        if self.tree_order == 1 { // avoid division by zero in float form
            // When b==1, sum_power(n) = n
            return n;
        }
        let b = self.tree_order as f64;
        (((1.0 - b.powi(n as i32)) / (1.0 - b)).round() as i128).max(0) as usize
    }

    fn inv_sum_power(&self, s: usize) -> f64 {
        // inv for sum_power on half N per Kotlin: log_b(1 + s*b - s)
        let b = self.tree_order as f64;
        if b <= 1.0 {
            return 0.0;
        }
        let arg = 1.0 + (s as f64) * b - (s as f64);
        arg.log(b)
    }

    fn unseen_neighbours(&self, seen: &HashSet<usize>, idx: usize) -> HashSet<usize> {
        let mut res = HashSet::new();
        if idx >= self.up.len() || idx >= self.down.len() {
            return res;
        }
        if let Some(u) = self.up[idx]
            && !seen.contains(&u) { res.insert(u); }
        for &d in &self.down[idx] {
            if !seen.contains(&d) { res.insert(d); }
        }
        res
    }

    fn flood_from(&self, seen: &mut HashSet<usize>, start: usize, for_node: usize, _level: usize) -> HashSet<usize> {
        seen.insert(start);
        let next_nodes = self.unseen_neighbours(seen, start);
        if start == for_node {
            return next_nodes;
        }
        let to_fill: Vec<usize> = next_nodes.iter().copied().filter(|n| !seen.contains(n)).collect();
        for n in &to_fill { seen.insert(*n); }
        to_fill.into_iter().flat_map(|n| self.flood_from(seen, n, for_node, _level + 1)).collect()
    }
}

impl NetDet for NetDetGraph {
    fn fill(&mut self) {
        // Basic validation
        if self.number_nodes == 0 || self.tree_order < 1 {
            self.clear();
            return;
        }
        if self.number_nodes == 1 {
            self.set_size(1);
            self.root_node = Some(0);
            // up[0] remains None; down[0] empty
            self.fail = false;
            return;
        }

        let depth = self.required_depth();

        self.set_size(self.number_nodes);

        // Build upper tree (ascending indices)
        let mut n = 0usize;
        let mut current_row: Vec<usize> = Vec::new();
        let mut prev_row: Option<Vec<usize>> = None;
        for row in 0..depth {
            let row_capacity = (self.tree_order as f64).powi(row as i32).ceil() as usize;
            while current_row.len() < row_capacity {
                if row == 0 {
                    // root
                    debug_assert_eq!(n, 0);
                    self.root_node = Some(n);
                    // up[0] remains None for now
                    current_row.push(n);
                } else {
                    debug_assert!(n > 0);
                    let prev = prev_row.as_ref().expect("prev_row should exist for row>0");
                    let up_idx = (current_row.len() * prev.len()) / row_capacity;
                    let parent = prev[up_idx];
                    self.up[n] = Some(parent);
                    self.down[parent].push(n);
                    current_row.push(n);
                }
                n += 1;
                if n >= self.number_nodes { break; }
            }
            prev_row = Some(current_row.clone());
            current_row.clear();
        }

        let outer_nodes = n; // count in upper tree
        let last_top = prev_row.clone().unwrap_or_default();
        let middle_nodes = self.number_nodes.saturating_sub(outer_nodes * 2);

        // Build lower tree (descending indices)
        n = self.number_nodes - 1;
        current_row.clear();
        prev_row = None;
        for row in 0..depth {
            let row_capacity = (self.tree_order as f64).powi(row as i32).ceil() as usize;
            while current_row.len() < row_capacity {
                if row == 0 {
                    debug_assert_eq!(n, self.number_nodes - 1);
                    // lower root points up to top root; top root's up set to lower root
                    let top_root = self.root_node.expect("top root set");
                    self.up[n] = Some(top_root);
                    self.up[top_root] = Some(n);
                    current_row.push(n);
                } else {
                    debug_assert!(n < (self.number_nodes - 1));
                    let prev = prev_row.as_ref().expect("prev_row should exist for row>0");
                    let up_idx = (current_row.len() * prev.len()) / row_capacity;
                    let parent = prev[up_idx];
                    self.up[n] = Some(parent);
                    self.down[parent].push(n);
                    current_row.push(n);
                }
                if n == 0 { break; }
                n -= 1;
            }
            prev_row = Some(current_row.clone());
            current_row.clear();
        }

        let last_bottom = prev_row.clone().unwrap_or_default();
        let middle_row_capacity = (self.tree_order as f64).powi(depth as i32) as usize;
        if middle_nodes > middle_row_capacity {
            // fail path
            self.clear();
            return;
        }

        // Build middle row
        for idx in 0..middle_nodes {
            let node_index = outer_nodes + idx;
            let outer_node_index = (idx * last_bottom.len()) / middle_row_capacity;
            let top_parent = last_top[outer_node_index];
            let bottom_leaf = last_bottom[outer_node_index];

            self.up[node_index] = Some(top_parent);
            self.down[node_index].push(bottom_leaf);
            self.down[top_parent].push(node_index);
            self.down[bottom_leaf].push(node_index);
        }

        // Ensure leaf connectivity symmetry for empty bottom leaves
        for (idx, bottom_node) in last_bottom.iter().copied().enumerate() {
            if self.down[bottom_node].is_empty() {
                let top_leaf = last_top[idx];
                self.down[bottom_node].push(top_leaf);
                self.down[top_leaf].push(bottom_node);
            }
        }

        self.fail = false;
    }

    fn required_depth(&self) -> usize {
        let tree_depth = self.inv_sum_power(self.number_nodes / 2) as usize;
        let middle_row_capacity = (self.tree_order as f64).powi(tree_depth as i32) as usize;
        let middle_nodes = self.number_nodes.saturating_sub(self.sum_power(tree_depth) * 2);
        if middle_nodes > middle_row_capacity { tree_depth + 1 } else { tree_depth }
    }

    fn flood(&self, start_origin: usize, for_node: usize) -> HashSet<usize> {
        let mut seen: HashSet<usize> = HashSet::new();
        self.flood_from(&mut seen, start_origin, for_node, 0)
    }

    fn mean_edges(&self) -> Option<f64> {
        if self.fail { return None; }
        if self.number_nodes == 0 { return None; }
        let mut sum = 0usize;
        for i in 0..self.number_nodes {
            // per Kotlin: down.size + 1
            sum += self.down.get(i).map(|v| v.len()).unwrap_or(0) + 1;
        }
        Some(sum as f64 / self.number_nodes as f64)
    }

    fn variance_edges(&self) -> f64 {
        if self.fail { return f64::INFINITY; }
        let mean = match self.mean_edges() { Some(m) => m, None => return f64::INFINITY };
        let mut acc = 0.0f64;
        for i in 0..self.number_nodes {
            let e = (self.down.get(i).map(|v| v.len()).unwrap_or(0) + 1) as f64;
            let d = e - mean;
            acc += d * d;
        }
        acc / (self.number_nodes as f64)
    }

    fn failed(&self) -> bool { self.fail }

    fn root(&self) -> Option<usize> { self.root_node }
}
