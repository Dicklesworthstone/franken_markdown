//! Stable layers for directed graphs with feedback loops.
//!
//! All acyclic dependencies retain the existing longest-path layers. Within a
//! strongly connected component, source order selects a deterministic forward
//! orientation: self-loops and edges to earlier nodes are feedback for layering
//! only. The original edges remain intact for drawing and outside-lane routing.
//! This is a bounded layout heuristic, not a minimum-feedback or optimal-layout
//! solver. Both component traversals are iterative and visit each edge once.

use super::{FlowEdge, FlowNode};
use std::collections::{BTreeMap, VecDeque};

pub(super) fn assign(
    nodes: &mut BTreeMap<String, FlowNode>,
    node_order: &[String],
    edges: &[FlowEdge],
) {
    let indices: BTreeMap<&str, usize> = node_order
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let mut outgoing = vec![Vec::new(); node_order.len()];
    let mut incoming = vec![Vec::new(); node_order.len()];
    for edge in edges {
        if let (Some(&from), Some(&to)) = (
            indices.get(edge.from.as_str()),
            indices.get(edge.to.as_str()),
        ) && nodes.contains_key(&edge.from)
            && nodes.contains_key(&edge.to)
        {
            outgoing[from].push(to);
            incoming[to].push(from);
        }
    }

    let components = components(&outgoing, &incoming);
    let forward = |from: usize, to: usize| components[from] != components[to] || from < to;
    let mut in_degrees = vec![0usize; node_order.len()];
    for (from, neighbors) in outgoing.iter().enumerate() {
        for &to in neighbors {
            if forward(from, to) {
                in_degrees[to] += 1;
            }
        }
    }

    let mut queue: VecDeque<_> = in_degrees
        .iter()
        .enumerate()
        .filter_map(|(index, &degree)| (degree == 0).then_some(index))
        .collect();
    let mut layers = vec![0usize; node_order.len()];
    while let Some(from) = queue.pop_front() {
        for &to in &outgoing[from] {
            if forward(from, to) {
                layers[to] = layers[to].max(layers[from] + 1);
                in_degrees[to] -= 1;
                if in_degrees[to] == 0 {
                    queue.push_back(to);
                }
            }
        }
    }
    // Condensation edges form a DAG, and the remaining edges inside each
    // component strictly increase source order. Every node therefore receives
    // a layer; no source-order fallback can serialize a cyclic branch or its
    // downstream acyclic neighbors.
    for (id, layer) in node_order.iter().zip(layers) {
        if let Some(node) = nodes.get_mut(id) {
            node.layer = layer;
        }
    }
}

/// Kosaraju's two walks, with explicit stacks so graph depth never consumes the
/// call stack. Numeric component identities have no effect on layout order.
fn components(outgoing: &[Vec<usize>], incoming: &[Vec<usize>]) -> Vec<usize> {
    let mut visited = vec![false; outgoing.len()];
    let mut finished = Vec::with_capacity(outgoing.len());
    let mut path = Vec::new();
    for root in 0..outgoing.len() {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        path.push((root, 0));
        while let Some((node, next)) = path.last_mut() {
            if let Some(&child) = outgoing[*node].get(*next) {
                *next += 1;
                if !visited[child] {
                    visited[child] = true;
                    path.push((child, 0));
                }
            } else {
                finished.push(*node);
                path.pop();
            }
        }
    }

    let mut component = vec![usize::MAX; outgoing.len()];
    let mut pending = Vec::new();
    for root in finished.into_iter().rev() {
        if component[root] != usize::MAX {
            continue;
        }
        component[root] = root;
        pending.push(root);
        while let Some(node) = pending.pop() {
            for &parent in &incoming[node] {
                if component[parent] == usize::MAX {
                    component[parent] = root;
                    pending.push(parent);
                }
            }
        }
    }
    component
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use crate::diagrams::flowchart;

    fn layers(source: &str) -> BTreeMap<String, usize> {
        let mut graph = flowchart::parse(source).unwrap().graph;
        assign(&mut graph.nodes, &graph.order, &graph.edges);
        graph
            .nodes
            .into_iter()
            .map(|(id, node)| (id, node.layer))
            .collect()
    }

    fn expected(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
        pairs
            .iter()
            .map(|&(id, layer)| (id.into(), layer))
            .collect()
    }

    #[test]
    fn feedback_preserves_parallel_branches_and_downstream_dependencies() {
        let source = "flowchart TD\nS-->M; I-->M; M-->O; M-->F; M-->T; \
                      O-->E; F-->E; T-->D; D-->O; E-->L; L-->S; L-->Exit; Exit-->Done";
        assert_eq!(
            layers(source),
            expected(&[
                ("S", 0),
                ("I", 0),
                ("M", 1),
                ("O", 2),
                ("F", 2),
                ("T", 2),
                ("E", 3),
                ("D", 3),
                ("L", 4),
                ("Exit", 5),
                ("Done", 6),
            ])
        );
    }

    #[test]
    fn dag_layers_follow_longest_dependencies_even_against_source_order() {
        assert_eq!(
            layers(
                "flowchart TD\nD; C; B; A; Z; Y; X; \
                    A-->D; A-->B; B-->C; C-->D; B-->C; X-->Y; Y-->Z; X-->Z"
            ),
            expected(&[
                ("A", 0),
                ("B", 1),
                ("C", 2),
                ("D", 3),
                ("X", 0),
                ("Y", 1),
                ("Z", 2),
            ])
        );
    }

    #[test]
    fn distinct_cycles_keep_all_intercomponent_edges_and_disconnected_roots() {
        assert_eq!(
            layers(
                "flowchart TD\nA; B; C; D; E; F; X; Y; Z; \
                    A-->B; B-->A; C-->D; D-->C; E-->F; F-->E; \
                    F-->A; B-->C; B-->C; D-->X; Y-->Y; Y-->Z"
            ),
            expected(&[
                ("E", 0),
                ("F", 1),
                ("A", 2),
                ("B", 3),
                ("C", 4),
                ("D", 5),
                ("X", 6),
                ("Y", 0),
                ("Z", 1),
            ])
        );
    }

    #[test]
    fn longest_admitted_cycle_uses_bounded_iterative_walks() {
        let mut source = String::from("flowchart TD\n");
        for index in 0..256 {
            source.push_str(&format!("N{index}-->N{};", (index + 1) % 256));
        }
        let actual = layers(&source);
        assert_eq!(actual.len(), 256);
        for index in 0..256 {
            assert_eq!(actual[&format!("N{index}")], index);
        }
    }
}
