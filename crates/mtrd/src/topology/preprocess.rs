use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{MetroTopology, TopologyPosition, validation::TopologyRenderError};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreprocessedTopology {
    pub source: MetroTopology,
    pub nodes: Vec<PreprocessedNode>,
    pub edges: Vec<PreprocessedEdge>,
    pub paths: Vec<PreprocessedPath>,
    #[serde(alias = "neighbour_orders")]
    pub neighbor_orders: Vec<StationNeighborOrder>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum PreprocessedNode {
    Station {
        source_station_index: usize,
        retention_reasons: Vec<RetentionReason>,
    },
    VirtualCrossing {
        position: TopologyPosition,
        incident_edges: Vec<IncidentEdge>,
        continuations: Vec<VirtualContinuation>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreprocessedEdge {
    pub endpoint_a: usize,
    pub endpoint_b: usize,
    pub source_station_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreprocessedPath {
    pub line_index: usize,
    pub path_index: usize,
    pub closed: bool,
    pub traversals: Vec<ReducedTraversal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReducedTraversal {
    pub edge_index: usize,
    pub forward: bool,
    pub source_spans: Vec<SourceSegmentSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSegmentSpan {
    pub segment_index: usize,
    pub start_fraction: f64,
    pub end_fraction: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncidentEdge {
    pub edge_index: usize,
    pub endpoint: EdgeEndpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeEndpoint {
    A,
    B,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VirtualContinuation {
    pub physical_edge_index: usize,
    pub incident_edges: [IncidentEdge; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StationNeighborOrder {
    pub source_station_index: usize,
    #[serde(alias = "neighbour_groups")]
    pub neighbor_groups: Vec<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RetentionReason {
    NonDegreeTwo,
    PathEndpoint {
        line_index: usize,
        path_index: usize,
    },
    Interchange,
    ShortCycle {
        line_index: usize,
        path_index: usize,
    },
    CycleSeed {
        line_index: usize,
        path_index: usize,
    },
    CycleSpacing {
        line_index: usize,
        path_index: usize,
    },
    Policy {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceOccurrence {
    pub(crate) line_index: usize,
    pub(crate) path_index: usize,
    pub(crate) segment_index: usize,
    pub(crate) forward: bool,
}

/// A source path segment involved in a preprocessing diagnostic.
///
/// Numeric indices are zero-based. User-facing error messages display path and
/// segment numbers as one-based values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedOccurrence {
    /// Index of the source line in `MetroTopology::lines`.
    pub line_index: usize,
    /// Stable source line ID.
    pub line_id: String,
    /// Index of the source path within its line.
    pub path_index: usize,
    /// Index of the segment within its source path.
    pub segment_index: usize,
    /// Source traversal's starting station ID.
    pub start_station: String,
    /// Source traversal's ending station ID.
    pub end_station: String,
    /// Whether the occurrence follows the canonical physical-edge orientation.
    pub forward: bool,
}

/// Why an intersection cannot be represented as an automatic virtual crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsupportedIntersectionKind {
    /// Two collinear physical edges overlap over a non-zero interval.
    CollinearOverlap,
    /// More than two physical edges meet at the same non-station point.
    MultiplePhysicalEdges { count: usize },
}

/// A topology validation or geometric preprocessing failure.
#[derive(Debug, PartialEq)]
pub enum TopologyPreprocessError {
    /// The source topology violates the ordinary topology-manifest contract.
    InvalidTopology(TopologyRenderError),

    /// A station lies inside another segment and requires an authoring decision.
    VirtualCrossingDecisionRequired {
        station: String,
        position: TopologyPosition,
        occurrences: Vec<LocatedOccurrence>,
    },

    /// A boundary case cannot be represented by a two-passage virtual crossing.
    UnsupportedTopologyIntersection {
        kind: UnsupportedIntersectionKind,
        position: TopologyPosition,
        occurrences: Vec<LocatedOccurrence>,
    },
}

impl fmt::Display for TopologyPreprocessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTopology(error) => write!(formatter, "invalid metro topology: {error}"),
            Self::VirtualCrossingDecisionRequired {
                station,
                position,
                occurrences,
            } => {
                write!(
                    formatter,
                    "cannot create a virtual crossing at {position:?}: station '{station}' lies strictly inside another segment, so whether it is a branch, transfer, or non-transfer crossing is ambiguous"
                )?;
                write_occurrences(formatter, occurrences)
            }
            Self::UnsupportedTopologyIntersection {
                kind,
                position,
                occurrences,
            } => {
                match kind {
                    UnsupportedIntersectionKind::CollinearOverlap => write!(
                        formatter,
                        "cannot create a virtual crossing at {position:?}: collinear physical edges overlap, so their continuation boundary is ambiguous"
                    )?,
                    UnsupportedIntersectionKind::MultiplePhysicalEdges { count } => write!(
                        formatter,
                        "cannot create a virtual crossing at {position:?}: {count} physical edges meet there; automatic virtual crossings require exactly two"
                    )?,
                }
                write_occurrences(formatter, occurrences)
            }
        }
    }
}

impl Error for TopologyPreprocessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidTopology(error) => Some(error),
            _ => None,
        }
    }
}

fn write_occurrences(
    formatter: &mut fmt::Formatter<'_>,
    occurrences: &[LocatedOccurrence],
) -> fmt::Result {
    formatter.write_str("; involved segments: ")?;
    for (index, occurrence) in occurrences.iter().enumerate() {
        if index > 0 {
            formatter.write_str(", ")?;
        }
        write!(
            formatter,
            "line '{}' path {} segment {} ({} -> {})",
            occurrence.line_id,
            occurrence.path_index + 1,
            occurrence.segment_index + 1,
            occurrence.start_station,
            occurrence.end_station,
        )?;
    }
    Ok(())
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct PolicyRetention {
    pub(crate) station_id: String,
    pub(crate) name: String,
}

#[derive(Debug)]
struct PhysicalEdge {
    a: usize,
    b: usize,
    occurrences: Vec<SourceOccurrence>,
}

#[derive(Debug, Clone, PartialEq)]
struct DetectedCrossing {
    position: TopologyPosition,
    physical_edges: [usize; 2],
    parameters: [f64; 2],
}

#[derive(Debug)]
struct GraphSection {
    a: usize,
    b: usize,
    start_fraction: f64,
    end_fraction: f64,
}

#[derive(Debug)]
struct ContractedGraph {
    edges: Vec<PreprocessedEdge>,
    section_to_reduced: Vec<(usize, bool)>,
    node_to_preprocessed: Vec<Option<usize>>,
}

type SegmentLookup = HashMap<(usize, usize), usize>;

pub fn preprocess_topology(
    topology: MetroTopology,
) -> Result<PreprocessedTopology, TopologyPreprocessError> {
    preprocess_topology_with_policy(topology, &[])
}

pub(crate) fn preprocess_topology_with_policy(
    topology: MetroTopology,
    policies: &[PolicyRetention],
) -> Result<PreprocessedTopology, TopologyPreprocessError> {
    super::validation::validate_topology_structure(&topology)
        .map_err(TopologyPreprocessError::InvalidTopology)?;

    let station_indices: HashMap<&str, usize> = topology
        .stations
        .iter()
        .enumerate()
        .map(|(index, station)| (station.id.as_str(), index))
        .collect();
    let (physical_edges, edge_lookup) = physical_edges(&topology, &station_indices);
    let crossings = classify_intersections(&topology, &physical_edges)?;

    let mut neighbors = vec![Vec::new(); topology.stations.len()];
    for edge in &physical_edges {
        neighbors[edge.a].push(edge.b);
        neighbors[edge.b].push(edge.a);
    }
    for adjacent in &mut neighbors {
        adjacent.sort_unstable();
    }

    let mut reasons = vec![Vec::new(); topology.stations.len()];
    for (station, adjacent) in neighbors.iter().enumerate() {
        if !adjacent.is_empty() && adjacent.len() != 2 {
            reasons[station].push(RetentionReason::NonDegreeTwo);
        }
    }

    let mut services = vec![HashSet::new(); topology.stations.len()];
    for (line_index, line) in topology.lines.iter().enumerate() {
        for (path_index, path) in line.paths.iter().enumerate() {
            for station_id in &path.stations {
                services[station_indices[station_id.as_str()]].insert(line_index);
            }
            if !path.closed {
                for station_id in [&path.stations[0], path.stations.last().unwrap()] {
                    add_reason(
                        &mut reasons[station_indices[station_id.as_str()]],
                        RetentionReason::PathEndpoint {
                            line_index,
                            path_index,
                        },
                    );
                }
            }
        }
    }
    for (station, lines) in services.iter().enumerate() {
        if lines.len() > 1 {
            add_reason(&mut reasons[station], RetentionReason::Interchange);
        }
    }

    for policy in policies {
        if let Some(&station) = station_indices.get(policy.station_id.as_str()) {
            add_reason(
                &mut reasons[station],
                RetentionReason::Policy {
                    name: policy.name.clone(),
                },
            );
        }
    }

    let naturally_retained: Vec<bool> = reasons.iter().map(|reasons| !reasons.is_empty()).collect();
    retain_cycles(
        &topology,
        &station_indices,
        &naturally_retained,
        &mut reasons,
    );
    for station_reasons in &mut reasons {
        station_reasons.sort();
        station_reasons.dedup();
    }
    let retained: Vec<bool> = reasons.iter().map(|reasons| !reasons.is_empty()).collect();

    let graph_node_count = topology.stations.len() + crossings.len();
    let mut node_to_preprocessed = vec![None; graph_node_count];
    let mut nodes = Vec::new();
    for (source_station_index, retention_reasons) in reasons.iter().enumerate() {
        if !neighbors[source_station_index].is_empty() && !retention_reasons.is_empty() {
            node_to_preprocessed[source_station_index] = Some(nodes.len());
            nodes.push(PreprocessedNode::Station {
                source_station_index,
                retention_reasons: retention_reasons.clone(),
            });
        }
    }
    for (crossing_index, crossing) in crossings.iter().enumerate() {
        let graph_node = topology.stations.len() + crossing_index;
        node_to_preprocessed[graph_node] = Some(nodes.len());
        nodes.push(PreprocessedNode::VirtualCrossing {
            position: crossing.position,
            incident_edges: Vec::new(),
            continuations: Vec::new(),
        });
    }

    let mut graph_retained = retained;
    graph_retained.extend(std::iter::repeat_n(true, crossings.len()));
    let (sections, physical_sections) =
        split_physical_edges(&physical_edges, &crossings, topology.stations.len());
    let contracted = contract_sections(
        &sections,
        &graph_retained,
        node_to_preprocessed,
        topology.stations.len(),
    );
    populate_virtual_crossings(
        &mut nodes,
        &crossings,
        &sections,
        &physical_sections,
        &contracted,
        topology.stations.len(),
        &topology,
    );
    let paths = reduced_paths(
        &topology,
        &station_indices,
        &edge_lookup,
        &physical_sections,
        &sections,
        &contracted.section_to_reduced,
    );
    let neighbor_orders = neighbor_orders(&topology, &neighbors);

    Ok(PreprocessedTopology {
        source: topology,
        nodes,
        edges: contracted.edges,
        paths,
        neighbor_orders,
    })
}

fn add_reason(reasons: &mut Vec<RetentionReason>, reason: RetentionReason) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn physical_edges(
    topology: &MetroTopology,
    station_indices: &HashMap<&str, usize>,
) -> (Vec<PhysicalEdge>, SegmentLookup) {
    let mut edges = Vec::<PhysicalEdge>::new();
    let mut lookup = HashMap::new();
    for (line_index, line) in topology.lines.iter().enumerate() {
        for (path_index, path) in line.paths.iter().enumerate() {
            for (segment_index, (start_id, end_id)) in path_segments(path).enumerate() {
                let start = station_indices[start_id];
                let end = station_indices[end_id];
                let key = ordered_pair(start, end);
                let edge_index = *lookup.entry(key).or_insert_with(|| {
                    let index = edges.len();
                    edges.push(PhysicalEdge {
                        a: key.0,
                        b: key.1,
                        occurrences: Vec::new(),
                    });
                    index
                });
                edges[edge_index].occurrences.push(SourceOccurrence {
                    line_index,
                    path_index,
                    segment_index,
                    forward: start == key.0,
                });
            }
        }
    }
    (edges, lookup)
}

fn retain_cycles(
    topology: &MetroTopology,
    station_indices: &HashMap<&str, usize>,
    natural: &[bool],
    reasons: &mut [Vec<RetentionReason>],
) {
    for (line_index, line) in topology.lines.iter().enumerate() {
        for (path_index, path) in line
            .paths
            .iter()
            .enumerate()
            .filter(|(_, path)| path.closed)
        {
            let stations: Vec<usize> = path
                .stations
                .iter()
                .map(|id| station_indices[id.as_str()])
                .collect();
            let m = stations.len();
            if m < 6 {
                for &station in &stations {
                    add_reason(
                        &mut reasons[station],
                        RetentionReason::ShortCycle {
                            line_index,
                            path_index,
                        },
                    );
                }
                continue;
            }

            let mut anchors: Vec<usize> =
                (0..m).filter(|&offset| natural[stations[offset]]).collect();
            if anchors.is_empty() {
                anchors.push(0);
                add_reason(
                    &mut reasons[stations[0]],
                    RetentionReason::CycleSeed {
                        line_index,
                        path_index,
                    },
                );
            }
            let original_anchors = anchors.clone();
            let q = m / 3;
            for (anchor_index, &start) in original_anchors.iter().enumerate() {
                let end = original_anchors[(anchor_index + 1) % original_anchors.len()];
                let distance = if end > start {
                    end - start
                } else {
                    m - start + end
                };
                let intervals = distance.div_ceil(q);
                for offset in balanced_split_offsets(distance, intervals) {
                    let station = stations[(start + offset) % m];
                    add_reason(
                        &mut reasons[station],
                        RetentionReason::CycleSpacing {
                            line_index,
                            path_index,
                        },
                    );
                }
            }
        }
    }
}

fn balanced_split_offsets(distance: usize, intervals: usize) -> Vec<usize> {
    match intervals {
        0 | 1 => Vec::new(),
        2 => vec![distance / 2],
        3 => {
            let base = distance / 3;
            match distance % 3 {
                0 => vec![base, base * 2],
                1 => vec![base, base * 2 + 1],
                2 => vec![base + 1, base * 2 + 1],
                _ => unreachable!(),
            }
        }
        4 => {
            let middle = distance / 2;
            vec![middle / 2, middle, middle + (distance - middle) / 2]
        }
        _ => unreachable!("a valid cycle interval needs at most four parts"),
    }
}

fn split_physical_edges(
    physical_edges: &[PhysicalEdge],
    crossings: &[DetectedCrossing],
    station_count: usize,
) -> (Vec<GraphSection>, Vec<Vec<usize>>) {
    let mut sections = Vec::new();
    let mut physical_sections = vec![Vec::new(); physical_edges.len()];
    for (physical_edge_index, edge) in physical_edges.iter().enumerate() {
        let mut split_points = crossings
            .iter()
            .enumerate()
            .filter_map(|(crossing_index, crossing)| {
                crossing
                    .physical_edges
                    .iter()
                    .position(|candidate| *candidate == physical_edge_index)
                    .map(|side| (crossing.parameters[side], station_count + crossing_index))
            })
            .collect::<Vec<_>>();
        split_points.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));

        let mut start_node = edge.a;
        let mut start_fraction = 0.0;
        for (end_fraction, end_node) in split_points
            .into_iter()
            .chain(std::iter::once((1.0, edge.b)))
        {
            let section_index = sections.len();
            sections.push(GraphSection {
                a: start_node,
                b: end_node,
                start_fraction,
                end_fraction,
            });
            physical_sections[physical_edge_index].push(section_index);
            start_node = end_node;
            start_fraction = end_fraction;
        }
    }
    (sections, physical_sections)
}

fn contract_sections(
    sections: &[GraphSection],
    retained: &[bool],
    node_to_preprocessed: Vec<Option<usize>>,
    station_count: usize,
) -> ContractedGraph {
    let mut adjacent = vec![Vec::<(usize, usize)>::new(); retained.len()];
    for (section_index, section) in sections.iter().enumerate() {
        adjacent[section.a].push((section.b, section_index));
        adjacent[section.b].push((section.a, section_index));
    }
    for neighbors in &mut adjacent {
        neighbors.sort_unstable();
    }

    let mut visited = vec![false; sections.len()];
    let mut edges = Vec::new();
    let mut section_to_reduced = vec![(usize::MAX, false); sections.len()];
    for initial in 0..sections.len() {
        if visited[initial] {
            continue;
        }
        let section = &sections[initial];
        let mut chain_nodes = vec![section.a, section.b];
        let mut chain_sections = vec![initial];
        visited[initial] = true;
        extend_section_chain(
            &mut chain_nodes,
            &mut chain_sections,
            false,
            &adjacent,
            retained,
            &mut visited,
        );
        extend_section_chain(
            &mut chain_nodes,
            &mut chain_sections,
            true,
            &adjacent,
            retained,
            &mut visited,
        );

        let endpoint_a =
            node_to_preprocessed[chain_nodes[0]].expect("a reduced edge endpoint must be retained");
        let endpoint_b = node_to_preprocessed[*chain_nodes.last().unwrap()]
            .expect("a reduced edge endpoint must be retained");
        if endpoint_a > endpoint_b {
            chain_nodes.reverse();
            chain_sections.reverse();
        }
        let endpoint_a = node_to_preprocessed[chain_nodes[0]].unwrap();
        let endpoint_b = node_to_preprocessed[*chain_nodes.last().unwrap()].unwrap();
        let reduced_index = edges.len();
        for (&section_index, pair) in chain_sections.iter().zip(chain_nodes.windows(2)) {
            section_to_reduced[section_index] =
                (reduced_index, sections[section_index].a == pair[0]);
        }
        edges.push(PreprocessedEdge {
            endpoint_a,
            endpoint_b,
            source_station_indices: chain_nodes
                .iter()
                .filter(|node| **node < station_count)
                .copied()
                .collect(),
        });
    }

    ContractedGraph {
        edges,
        section_to_reduced,
        node_to_preprocessed,
    }
}

fn extend_section_chain(
    chain_nodes: &mut Vec<usize>,
    chain_sections: &mut Vec<usize>,
    at_front: bool,
    adjacent: &[Vec<(usize, usize)>],
    retained: &[bool],
    visited: &mut [bool],
) {
    loop {
        let end = if at_front {
            chain_nodes[0]
        } else {
            *chain_nodes.last().unwrap()
        };
        if retained[end] {
            break;
        }
        debug_assert_eq!(adjacent[end].len(), 2);
        let (_, next_section) = adjacent[end]
            .iter()
            .find(|(_, section)| !visited[*section])
            .copied()
            .expect("a contractible station must continue through one unvisited section");
        let section = &adjacent[end]
            .iter()
            .find(|(_, candidate)| *candidate == next_section)
            .unwrap();
        let next_node = section.0;
        visited[next_section] = true;
        if at_front {
            chain_nodes.insert(0, next_node);
            chain_sections.insert(0, next_section);
        } else {
            chain_nodes.push(next_node);
            chain_sections.push(next_section);
        }
    }
}

fn populate_virtual_crossings(
    nodes: &mut [PreprocessedNode],
    crossings: &[DetectedCrossing],
    sections: &[GraphSection],
    physical_sections: &[Vec<usize>],
    contracted: &ContractedGraph,
    station_count: usize,
    topology: &MetroTopology,
) {
    for (crossing_index, crossing) in crossings.iter().enumerate() {
        let graph_node = station_count + crossing_index;
        let preprocessed_node = contracted.node_to_preprocessed[graph_node].unwrap();
        let mut incident_with_bearings = Vec::new();
        let mut continuations = Vec::new();
        for &physical_edge_index in &crossing.physical_edges {
            let incident_sections = physical_sections[physical_edge_index]
                .iter()
                .filter(|&&section_index| {
                    let section = &sections[section_index];
                    section.a == graph_node || section.b == graph_node
                })
                .copied()
                .collect::<Vec<_>>();
            debug_assert_eq!(incident_sections.len(), 2);
            let paired = incident_sections
                .iter()
                .map(|&section_index| incident_edge(graph_node, section_index, contracted))
                .collect::<Vec<_>>();
            continuations.push(VirtualContinuation {
                physical_edge_index,
                incident_edges: [paired[0], paired[1]],
            });
            incident_with_bearings.extend(incident_sections.iter().zip(paired).map(
                |(&section_index, incident)| {
                    let section = &sections[section_index];
                    let other_graph_node = if section.a == graph_node {
                        section.b
                    } else {
                        section.a
                    };
                    let other_position =
                        graph_node_position(other_graph_node, station_count, topology, crossings);
                    (incident, bearing(crossing.position, other_position))
                },
            ));
        }
        incident_with_bearings.sort_by(|left, right| {
            left.1
                .total_cmp(&right.1)
                .then(left.0.edge_index.cmp(&right.0.edge_index))
        });
        nodes[preprocessed_node] = PreprocessedNode::VirtualCrossing {
            position: crossing.position,
            incident_edges: incident_with_bearings
                .into_iter()
                .map(|(incident, _)| incident)
                .collect(),
            continuations,
        };
    }
}

fn incident_edge(
    graph_node: usize,
    section_index: usize,
    contracted: &ContractedGraph,
) -> IncidentEdge {
    let edge_index = contracted.section_to_reduced[section_index].0;
    let node_index = contracted.node_to_preprocessed[graph_node].unwrap();
    let edge = &contracted.edges[edge_index];
    IncidentEdge {
        edge_index,
        endpoint: if edge.endpoint_a == node_index {
            EdgeEndpoint::A
        } else {
            debug_assert_eq!(edge.endpoint_b, node_index);
            EdgeEndpoint::B
        },
    }
}

fn graph_node_position(
    graph_node: usize,
    station_count: usize,
    topology: &MetroTopology,
    crossings: &[DetectedCrossing],
) -> TopologyPosition {
    if graph_node < station_count {
        topology.stations[graph_node].position
    } else {
        crossings[graph_node - station_count].position
    }
}

fn reduced_paths(
    topology: &MetroTopology,
    station_indices: &HashMap<&str, usize>,
    edge_lookup: &SegmentLookup,
    physical_sections: &[Vec<usize>],
    sections: &[GraphSection],
    section_to_reduced: &[(usize, bool)],
) -> Vec<PreprocessedPath> {
    let mut paths = Vec::new();
    for (line_index, line) in topology.lines.iter().enumerate() {
        for (path_index, path) in line.paths.iter().enumerate() {
            let mut traversals: Vec<ReducedTraversal> = Vec::new();
            for (segment_index, (start_id, end_id)) in path_segments(path).enumerate() {
                let start = station_indices[start_id];
                let end = station_indices[end_id];
                let physical_edge_index = edge_lookup[&ordered_pair(start, end)];
                let occurrence_forward = start < end;
                let section_indices: Box<dyn Iterator<Item = &usize>> = if occurrence_forward {
                    Box::new(physical_sections[physical_edge_index].iter())
                } else {
                    Box::new(physical_sections[physical_edge_index].iter().rev())
                };
                for &section_index in section_indices {
                    let section = &sections[section_index];
                    let (edge_index, canonical_section_forward) = section_to_reduced[section_index];
                    let forward = if occurrence_forward {
                        canonical_section_forward
                    } else {
                        !canonical_section_forward
                    };
                    let span = if occurrence_forward {
                        SourceSegmentSpan {
                            segment_index,
                            start_fraction: section.start_fraction,
                            end_fraction: section.end_fraction,
                        }
                    } else {
                        SourceSegmentSpan {
                            segment_index,
                            start_fraction: 1.0 - section.end_fraction,
                            end_fraction: 1.0 - section.start_fraction,
                        }
                    };
                    if let Some(last) = traversals.last_mut()
                        && last.edge_index == edge_index
                        && last.forward == forward
                    {
                        last.source_spans.push(span);
                    } else {
                        traversals.push(ReducedTraversal {
                            edge_index,
                            forward,
                            source_spans: vec![span],
                        });
                    }
                }
            }
            paths.push(PreprocessedPath {
                line_index,
                path_index,
                closed: path.closed,
                traversals,
            });
        }
    }
    paths
}

fn neighbor_orders(
    topology: &MetroTopology,
    neighbors: &[Vec<usize>],
) -> Vec<StationNeighborOrder> {
    let mut orders = Vec::new();
    for (station, adjacent) in neighbors
        .iter()
        .enumerate()
        .filter(|(_, adjacent)| adjacent.len() > 2)
    {
        let origin = topology.stations[station].position;
        let mut ordered = adjacent.clone();
        ordered.sort_by(|&left, &right| {
            bearing(origin, topology.stations[left].position)
                .total_cmp(&bearing(origin, topology.stations[right].position))
                .then(left.cmp(&right))
        });
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for neighbor in ordered {
            if let Some(group) = groups.last_mut()
                && same_bearing(
                    origin,
                    topology.stations[group[0]].position,
                    topology.stations[neighbor].position,
                )
            {
                group.push(neighbor);
            } else {
                groups.push(vec![neighbor]);
            }
        }
        orders.push(StationNeighborOrder {
            source_station_index: station,
            neighbor_groups: groups,
        });
    }
    orders
}

fn bearing(origin: TopologyPosition, point: TopologyPosition) -> f64 {
    (point.y - origin.y).atan2(point.x - origin.x)
}

fn same_bearing(origin: TopologyPosition, a: TopologyPosition, b: TopologyPosition) -> bool {
    let ax = a.x - origin.x;
    let ay = a.y - origin.y;
    let bx = b.x - origin.x;
    let by = b.y - origin.y;
    ax * by - ay * bx == 0.0 && ax * bx + ay * by > 0.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SegmentIntersection {
    None,
    Proper {
        position: TopologyPosition,
        left_parameter: f64,
        right_parameter: f64,
    },
    CollinearOverlap {
        position: TopologyPosition,
    },
}

fn classify_intersections(
    topology: &MetroTopology,
    edges: &[PhysicalEdge],
) -> Result<Vec<DetectedCrossing>, TopologyPreprocessError> {
    for (station_index, station) in topology.stations.iter().enumerate() {
        let mut involved = Vec::new();
        for edge in edges {
            if edge.a != station_index
                && edge.b != station_index
                && point_strictly_inside_segment(
                    station.position,
                    topology.stations[edge.a].position,
                    topology.stations[edge.b].position,
                )
            {
                involved.extend(located_occurrences(topology, edge));
            }
        }
        if !involved.is_empty() {
            for edge in edges
                .iter()
                .filter(|edge| edge.a == station_index || edge.b == station_index)
            {
                involved.extend(located_occurrences(topology, edge));
            }
            sort_occurrences(&mut involved);
            return Err(TopologyPreprocessError::VirtualCrossingDecisionRequired {
                station: station.id.clone(),
                position: station.position,
                occurrences: involved,
            });
        }
    }

    let mut candidates = Vec::new();
    for (left_index, left) in edges.iter().enumerate() {
        for (right_index, right) in edges.iter().enumerate().skip(left_index + 1) {
            if left.a == right.a || left.a == right.b || left.b == right.a || left.b == right.b {
                continue;
            }
            match segment_intersection(
                topology.stations[left.a].position,
                topology.stations[left.b].position,
                topology.stations[right.a].position,
                topology.stations[right.b].position,
            ) {
                SegmentIntersection::None => {}
                SegmentIntersection::Proper {
                    position,
                    left_parameter,
                    right_parameter,
                } => candidates.push(DetectedCrossing {
                    position,
                    physical_edges: [left_index, right_index],
                    parameters: [left_parameter, right_parameter],
                }),
                SegmentIntersection::CollinearOverlap { position } => {
                    let mut occurrences = located_occurrences(topology, left);
                    occurrences.extend(located_occurrences(topology, right));
                    sort_occurrences(&mut occurrences);
                    return Err(TopologyPreprocessError::UnsupportedTopologyIntersection {
                        kind: UnsupportedIntersectionKind::CollinearOverlap,
                        position,
                        occurrences,
                    });
                }
            }
        }
    }

    let mut grouped = Vec::<(TopologyPosition, Vec<usize>)>::new();
    for candidate in candidates {
        if let Some(existing) = grouped
            .iter_mut()
            .find(|(position, _)| *position == candidate.position)
        {
            for edge in candidate.physical_edges {
                if !existing.1.contains(&edge) {
                    existing.1.push(edge);
                }
            }
        } else {
            grouped.push((candidate.position, candidate.physical_edges.to_vec()));
        }
    }
    let mut crossings = Vec::new();
    for (position, mut physical_edges) in grouped {
        physical_edges.sort_unstable();
        if physical_edges.len() > 2 {
            let mut occurrences = physical_edges
                .iter()
                .flat_map(|&edge| located_occurrences(topology, &edges[edge]))
                .collect::<Vec<_>>();
            sort_occurrences(&mut occurrences);
            return Err(TopologyPreprocessError::UnsupportedTopologyIntersection {
                kind: UnsupportedIntersectionKind::MultiplePhysicalEdges {
                    count: physical_edges.len(),
                },
                position,
                occurrences,
            });
        }
        debug_assert_eq!(physical_edges.len(), 2);
        let physical_edges = [physical_edges[0], physical_edges[1]];
        crossings.push(DetectedCrossing {
            position,
            physical_edges,
            parameters: physical_edges
                .map(|edge_index| edge_parameter(topology, &edges[edge_index], position)),
        });
    }
    crossings.sort_by(|left, right| {
        left.physical_edges[0]
            .cmp(&right.physical_edges[0])
            .then(left.parameters[0].total_cmp(&right.parameters[0]))
            .then(left.physical_edges[1].cmp(&right.physical_edges[1]))
            .then(left.parameters[1].total_cmp(&right.parameters[1]))
    });
    Ok(crossings)
}

fn edge_parameter(
    topology: &MetroTopology,
    edge: &PhysicalEdge,
    position: TopologyPosition,
) -> f64 {
    let start = topology.stations[edge.a].position;
    let end = topology.stations[edge.b].position;
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    if dx.abs() >= dy.abs() {
        (position.x - start.x) / dx
    } else {
        (position.y - start.y) / dy
    }
}

fn located_occurrences(topology: &MetroTopology, edge: &PhysicalEdge) -> Vec<LocatedOccurrence> {
    edge.occurrences
        .iter()
        .map(|occurrence| {
            let path = &topology.lines[occurrence.line_index].paths[occurrence.path_index];
            let (start_station, end_station) =
                path_segments(path).nth(occurrence.segment_index).unwrap();
            LocatedOccurrence {
                line_index: occurrence.line_index,
                line_id: topology.lines[occurrence.line_index].id.clone(),
                path_index: occurrence.path_index,
                segment_index: occurrence.segment_index,
                start_station: start_station.into(),
                end_station: end_station.into(),
                forward: occurrence.forward,
            }
        })
        .collect()
}

fn sort_occurrences(occurrences: &mut Vec<LocatedOccurrence>) {
    occurrences.sort_by(|a, b| {
        a.line_index
            .cmp(&b.line_index)
            .then(a.path_index.cmp(&b.path_index))
            .then(a.segment_index.cmp(&b.segment_index))
    });
    occurrences.dedup();
}

fn segment_intersection(
    a: TopologyPosition,
    b: TopologyPosition,
    c: TopologyPosition,
    d: TopologyPosition,
) -> SegmentIntersection {
    let ab = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd = cross(c, d, a);
    let cd_b = cross(c, d, b);
    if ab == 0.0 && ab_d == 0.0 {
        let mut candidates = [a, b, c, d];
        candidates.sort_by(position_cmp);
        let start = candidates[1];
        let end = candidates[2];
        return if start != end
            && point_on_segment(start, a, b)
            && point_on_segment(start, c, d)
            && point_on_segment(end, a, b)
            && point_on_segment(end, c, d)
        {
            SegmentIntersection::CollinearOverlap { position: start }
        } else {
            SegmentIntersection::None
        };
    }
    if ab * ab_d < 0.0 && cd * cd_b < 0.0 {
        let rx = b.x - a.x;
        let ry = b.y - a.y;
        let sx = d.x - c.x;
        let sy = d.y - c.y;
        let denominator = rx * sy - ry * sx;
        let qx = c.x - a.x;
        let qy = c.y - a.y;
        let left_parameter = (qx * sy - qy * sx) / denominator;
        let right_parameter = (qx * ry - qy * rx) / denominator;
        return SegmentIntersection::Proper {
            position: TopologyPosition {
                x: clean_zero(a.x + left_parameter * rx),
                y: clean_zero(a.y + left_parameter * ry),
            },
            left_parameter,
            right_parameter,
        };
    }
    SegmentIntersection::None
}

fn clean_zero(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

fn position_cmp(a: &TopologyPosition, b: &TopologyPosition) -> Ordering {
    a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))
}

fn cross(a: TopologyPosition, b: TopologyPosition, point: TopologyPosition) -> f64 {
    (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x)
}

fn point_on_segment(point: TopologyPosition, a: TopologyPosition, b: TopologyPosition) -> bool {
    cross(a, b, point) == 0.0
        && point.x >= a.x.min(b.x)
        && point.x <= a.x.max(b.x)
        && point.y >= a.y.min(b.y)
        && point.y <= a.y.max(b.y)
}

fn point_strictly_inside_segment(
    point: TopologyPosition,
    a: TopologyPosition,
    b: TopologyPosition,
) -> bool {
    point != a && point != b && point_on_segment(point, a, b)
}

fn ordered_pair(a: usize, b: usize) -> (usize, usize) {
    if a < b { (a, b) } else { (b, a) }
}

fn path_segments(path: &super::TopologyPath) -> impl Iterator<Item = (&str, &str)> {
    let adjacent = path
        .stations
        .windows(2)
        .map(|stations| (stations[0].as_str(), stations[1].as_str()));
    let closing = path.closed.then(|| {
        (
            path.stations.last().unwrap().as_str(),
            path.stations.first().unwrap().as_str(),
        )
    });
    adjacent.chain(closing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TopologyLine, TopologyPath, TopologyStation};

    type TestPath<'a> = (&'a [&'a str], bool);
    type TestLine<'a> = (&'a str, &'a [TestPath<'a>]);

    fn options() -> crate::TopologyOptions {
        serde_yaml::from_str(
            r##"
background: { transparent: true }
labels: { hidden: false }
lines: { width: 8.0 }
stations:
  common:
    fill: { diameter: 18.0, color: { type: unified, value: "#ffffff" } }
    stroke: { width: 2.0, alignment: center, color: { type: follow-line } }
  interchange:
    fill: { width: 18.0, color: "#ffffff" }
    stroke: { width: 2.0, alignment: outside, color: "#000000" }
"##,
        )
        .unwrap()
    }

    fn topology(stations: &[(&str, f64, f64)], lines: &[TestLine<'_>]) -> MetroTopology {
        MetroTopology {
            options: options(),
            stations: stations
                .iter()
                .map(|&(id, x, y)| TopologyStation {
                    id: id.into(),
                    names: [("en".into(), vec![id.into()])].into(),
                    position: TopologyPosition { x, y },
                })
                .collect(),
            lines: lines
                .iter()
                .map(|&(id, paths)| TopologyLine {
                    id: id.into(),
                    names: Default::default(),
                    color: "#000".into(),
                    paths: paths
                        .iter()
                        .map(|&(stations, closed)| TopologyPath {
                            stations: stations.iter().map(|id| (*id).into()).collect(),
                            closed,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn reasons(result: &PreprocessedTopology, station: usize) -> &[RetentionReason] {
        result
            .nodes
            .iter()
            .find_map(|node| match node {
                PreprocessedNode::Station {
                    source_station_index,
                    retention_reasons,
                } if *source_station_index == station => Some(retention_reasons.as_slice()),
                _ => None,
            })
            .unwrap()
    }

    fn retained_stations(result: &PreprocessedTopology) -> Vec<usize> {
        result
            .nodes
            .iter()
            .filter_map(|node| match node {
                PreprocessedNode::Station {
                    source_station_index,
                    ..
                } => Some(*source_station_index),
                PreprocessedNode::VirtualCrossing { .. } => None,
            })
            .collect()
    }

    #[test]
    fn contracts_linear_chain_and_reconstructs_provenance() {
        let source = topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 1.0, 1.0),
                ("C", 2.0, 1.0),
                ("D", 3.0, 0.0),
            ],
            &[("red", &[(&["A", "B", "C", "D"], false)])],
        );
        let result = preprocess_topology(source.clone()).unwrap();

        assert_eq!(result.edges.len(), 1);
        assert_eq!(result.edges[0].source_station_indices, [0, 1, 2, 3]);
        assert_eq!(retained_stations(&result), [0, 3]);
        assert_eq!(
            result.paths[0].traversals[0]
                .source_spans
                .iter()
                .map(|span| span.segment_index)
                .collect::<Vec<_>>(),
            [0, 1, 2],
        );
        assert_eq!(result.source, source);
    }

    #[test]
    fn retains_branch_endpoint_interchange_and_policy_reasons() {
        let source = topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 1.0, 0.0),
                ("C", 2.0, 0.0),
                ("D", 1.0, 1.0),
            ],
            &[
                ("red", &[(&["A", "B", "C"], false), (&["B", "D"], false)]),
                ("blue", &[(&["A", "B"], false)]),
            ],
        );
        let result = preprocess_topology_with_policy(
            source,
            &[PolicyRetention {
                station_id: "B".into(),
                name: "landmark".into(),
            }],
        )
        .unwrap();

        assert!(reasons(&result, 1).contains(&RetentionReason::NonDegreeTwo));
        assert!(
            reasons(&result, 1).contains(&RetentionReason::PathEndpoint {
                line_index: 0,
                path_index: 1
            })
        );
        assert!(reasons(&result, 1).contains(&RetentionReason::Interchange));
        assert!(reasons(&result, 1).contains(&RetentionReason::Policy {
            name: "landmark".into()
        }));
    }

    #[test]
    fn multiple_paths_of_one_line_do_not_create_interchange() {
        let source = topology(
            &[("A", 0.0, 0.0), ("B", 1.0, 0.0), ("C", 2.0, 0.0)],
            &[("red", &[(&["A", "B"], false), (&["B", "C"], false)])],
        );
        let result = preprocess_topology(source).unwrap();
        assert!(!reasons(&result, 1).contains(&RetentionReason::Interchange));
    }

    #[test]
    fn retains_short_cycles_and_balances_large_cycles() {
        let small = topology(
            &[("A", 0.0, 0.0), ("B", 1.0, 0.0), ("C", 0.5, 1.0)],
            &[("ring", &[(&["A", "B", "C"], true)])],
        );
        let result = preprocess_topology(small).unwrap();
        assert!(result.nodes.iter().all(|node| matches!(
            node,
            PreprocessedNode::Station { retention_reasons, .. }
                if matches!(retention_reasons.as_slice(), [RetentionReason::ShortCycle { .. }])
        )));

        let points = (0..12)
            .map(|index| {
                let angle = index as f64 * std::f64::consts::TAU / 12.0;
                (format!("S{index}"), angle.cos(), angle.sin())
            })
            .collect::<Vec<_>>();
        let stations = points
            .iter()
            .map(|(id, x, y)| (id.as_str(), *x, *y))
            .collect::<Vec<_>>();
        let ids = points
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect::<Vec<_>>();
        let result =
            preprocess_topology(topology(&stations, &[("ring", &[(ids.as_slice(), true)])]))
                .unwrap();
        let retained = retained_stations(&result);
        assert_eq!(retained, [0, 4, 8]);
        assert!(reasons(&result, 0).contains(&RetentionReason::CycleSeed {
            line_index: 0,
            path_index: 0
        }));
    }

    #[test]
    fn shared_and_opposite_paths_keep_distinct_traversals() {
        let source = topology(
            &[("A", 0.0, 0.0), ("B", 1.0, 1.0), ("C", 2.0, 0.0)],
            &[
                ("red", &[(&["A", "B", "C"], false)]),
                ("blue", &[(&["C", "B", "A"], false)]),
            ],
        );
        let result = preprocess_topology(source).unwrap();
        assert_eq!(result.edges.len(), 2);
        assert_eq!(result.paths.len(), 2);
        assert_ne!(
            result.paths[0].traversals[0].forward,
            result.paths[1].traversals[1].forward
        );
    }

    #[test]
    fn omits_isolated_station_and_preserves_disconnected_components() {
        let source = topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 1.0, 0.0),
                ("C", 3.0, 0.0),
                ("D", 4.0, 0.0),
                ("X", 9.0, 9.0),
            ],
            &[
                ("red", &[(&["A", "B"], false)]),
                ("blue", &[(&["C", "D"], false)]),
            ],
        );
        let result = preprocess_topology(source).unwrap();
        assert_eq!(result.edges.len(), 2);
        assert!(!retained_stations(&result).contains(&4));
        assert_eq!(result.source.stations.len(), 5);
    }

    #[test]
    fn groups_equal_bearing_neighbors_deterministically() {
        let source = topology(
            &[
                ("O", 0.0, 0.0),
                ("E1", 1.0, 0.0),
                ("E2", 2.0, 0.0),
                ("N", 0.0, 1.0),
                ("W", -1.0, 0.0),
            ],
            &[(
                "red",
                &[(&["E1", "O", "N"], false), (&["E2", "O", "W"], false)],
            )],
        );
        let orders = neighbor_orders(&source, &[vec![1, 2, 3, 4], vec![], vec![], vec![], vec![]]);
        assert_eq!(orders[0].neighbor_groups, [vec![1, 2], vec![3], vec![4]]);

        let error = preprocess_topology(source).unwrap_err();
        assert!(
            matches!(error, TopologyPreprocessError::VirtualCrossingDecisionRequired { station, .. } if station == "E1")
        );
    }

    #[test]
    fn rejects_station_on_segment_and_accepts_proper_intersections() {
        let station_on_segment = topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 2.0, 0.0),
                ("X", 1.0, 0.0),
                ("Y", 1.0, 1.0),
            ],
            &[
                ("red", &[(&["A", "B"], false)]),
                ("blue", &[(&["X", "Y"], false)]),
            ],
        );
        let station_error = preprocess_topology(station_on_segment).unwrap_err();
        let station_message = station_error.to_string();
        assert!(
            matches!(station_error, TopologyPreprocessError::VirtualCrossingDecisionRequired { station, ref occurrences, .. } if station == "X" && occurrences.len() == 2)
        );
        assert!(
            station_message.contains("branch, transfer, or non-transfer crossing is ambiguous")
        );
        assert!(station_message.contains("line 'red' path 1 segment 1 (A -> B)"));
        assert!(station_message.contains("line 'blue' path 1 segment 1 (X -> Y)"));

        let crossing = topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 2.0, 2.0),
                ("C", 0.0, 2.0),
                ("D", 2.0, 0.0),
            ],
            &[
                ("red", &[(&["A", "B"], false)]),
                ("blue", &[(&["C", "D"], false)]),
            ],
        );
        let crossing = preprocess_topology(crossing).unwrap();
        assert_eq!(
            crossing
                .nodes
                .iter()
                .filter(|node| matches!(node, PreprocessedNode::VirtualCrossing { .. }))
                .count(),
            1
        );

        let overlap = topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 3.0, 0.0),
                ("C", 1.0, 0.0),
                ("D", 4.0, 0.0),
            ],
            &[
                ("red", &[(&["A", "B"], false)]),
                ("blue", &[(&["C", "D"], false)]),
            ],
        );
        let overlap_error = preprocess_topology(overlap).unwrap_err();
        assert!(matches!(
            &overlap_error,
            TopologyPreprocessError::VirtualCrossingDecisionRequired { .. }
        ));
        assert!(overlap_error.to_string().contains("is ambiguous"));
    }

    #[test]
    fn preprocessed_structure_is_stable() {
        let source = topology(
            &[("A", 0.0, 0.0), ("B", 1.0, 1.0), ("C", 2.0, 0.0)],
            &[("red", &[(&["A", "B", "C"], false)])],
        );
        let first = preprocess_topology(source.clone()).unwrap();
        let second = preprocess_topology(source).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn preprocessing_does_not_apply_rendering_viewport_limits() {
        let source = topology(
            &[("A", -f64::MAX, 0.0), ("B", f64::MAX, 1.0)],
            &[("red", &[(&["A", "B"], false)])],
        );
        assert!(matches!(
            crate::validate_topology(&source),
            Err(TopologyRenderError::CoordinateRange)
        ));
        assert!(preprocess_topology(source).is_ok());
    }

    #[test]
    fn double_midpoint_balances_ten_and_eleven_station_rings() {
        for size in [10, 11] {
            let stations = (0..size)
                .map(|index| {
                    let angle = index as f64 * std::f64::consts::TAU / size as f64;
                    TopologyStation {
                        id: format!("S{index}"),
                        names: Default::default(),
                        position: TopologyPosition {
                            x: angle.cos(),
                            y: angle.sin(),
                        },
                    }
                })
                .collect::<Vec<_>>();
            let source = MetroTopology {
                options: options(),
                lines: vec![TopologyLine {
                    id: "ring".into(),
                    names: Default::default(),
                    color: "#000".into(),
                    paths: vec![TopologyPath {
                        stations: stations.iter().map(|station| station.id.clone()).collect(),
                        closed: true,
                    }],
                }],
                stations,
            };
            let result = preprocess_topology(source).unwrap();
            let retained = retained_stations(&result);
            let distances = retained
                .iter()
                .zip(retained.iter().cycle().skip(1))
                .map(|(&start, &end)| {
                    if end > start {
                        end - start
                    } else {
                        size - start + end
                    }
                })
                .collect::<Vec<_>>();

            assert_eq!(retained.len(), 4);
            assert!(distances.iter().all(|distance| distance * 3 <= size));
            assert!(distances.iter().max().unwrap() - distances.iter().min().unwrap() <= 1);
        }
    }

    #[test]
    fn reversed_source_path_keeps_canonical_edge_orientation() {
        let stations = &[("A", 0.0, 0.0), ("B", 1.0, 1.0), ("C", 2.0, 0.0)];
        let forward =
            preprocess_topology(topology(stations, &[("red", &[(&["A", "B", "C"], false)])]))
                .unwrap();
        let reverse =
            preprocess_topology(topology(stations, &[("red", &[(&["C", "B", "A"], false)])]))
                .unwrap();

        assert_eq!(
            forward.edges[0].source_station_indices,
            reverse.edges[0].source_station_indices
        );
        assert_ne!(
            forward.paths[0].traversals[0].forward,
            reverse.paths[0].traversals[0].forward
        );
    }

    #[test]
    fn preserves_underlying_validation_error() {
        let mut source = topology(
            &[("A", 0.0, 0.0), ("B", 1.0, 0.0)],
            &[("red", &[(&["A", "B"], false)])],
        );
        source.lines[0].paths[0].stations[1] = "missing".into();

        assert!(matches!(
            preprocess_topology(source),
            Err(TopologyPreprocessError::InvalidTopology(
                TopologyRenderError::UnknownStation { .. }
            ))
        ));
    }

    #[test]
    fn splits_an_isolated_crossing_and_pairs_non_transfer_continuations() {
        let source = topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 2.0, 2.0),
                ("C", 0.0, 2.0),
                ("D", 2.0, 0.0),
            ],
            &[
                ("red", &[(&["A", "B"], false)]),
                ("blue", &[(&["D", "C"], false)]),
            ],
        );
        let result = preprocess_topology(source.clone()).unwrap();
        let repeated = preprocess_topology(source.clone()).unwrap();
        assert_eq!(result, repeated);
        let crossing = result
            .nodes
            .iter()
            .find_map(|node| match node {
                PreprocessedNode::VirtualCrossing {
                    position,
                    incident_edges,
                    continuations,
                } => Some((*position, incident_edges, continuations)),
                PreprocessedNode::Station { .. } => None,
            })
            .unwrap();

        assert_eq!(crossing.0, TopologyPosition { x: 1.0, y: 1.0 });
        assert_eq!(crossing.1.len(), 4);
        assert_eq!(crossing.2.len(), 2);
        assert_eq!(crossing.2[0].physical_edge_index, 0);
        assert_eq!(crossing.2[1].physical_edge_index, 1);
        assert!(
            crossing.2[0]
                .incident_edges
                .iter()
                .all(|edge| !crossing.2[1].incident_edges.contains(edge))
        );
        assert_eq!(result.edges.len(), 4);

        for path in &result.paths {
            assert_eq!(path.traversals.len(), 2);
            let spans = path
                .traversals
                .iter()
                .flat_map(|traversal| &traversal.source_spans)
                .collect::<Vec<_>>();
            assert_eq!(spans.len(), 2);
            assert_eq!(spans[0].segment_index, 0);
            assert_eq!((spans[0].start_fraction, spans[0].end_fraction), (0.0, 0.5));
            assert_eq!((spans[1].start_fraction, spans[1].end_fraction), (0.5, 1.0));
        }

        assert_eq!(
            crate::generate_schematic(&source),
            Err(crate::SchematicGenerationError::StageUnavailable)
        );
    }

    #[test]
    fn accepts_multiple_isolated_crossings_along_one_edge() {
        let source = topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 6.0, 0.0),
                ("C", 2.0, -1.0),
                ("D", 2.0, 1.0),
                ("E", 4.0, -1.0),
                ("F", 4.0, 1.0),
            ],
            &[
                ("red", &[(&["A", "B"], false)]),
                ("blue", &[(&["C", "D"], false)]),
                ("green", &[(&["E", "F"], false)]),
            ],
        );
        let result = preprocess_topology(source).unwrap();
        let crossing_positions = result
            .nodes
            .iter()
            .filter_map(|node| match node {
                PreprocessedNode::VirtualCrossing { position, .. } => Some(*position),
                PreprocessedNode::Station { .. } => None,
            })
            .collect::<Vec<_>>();
        let red_spans = result.paths[0]
            .traversals
            .iter()
            .flat_map(|traversal| &traversal.source_spans)
            .map(|span| (span.start_fraction, span.end_fraction))
            .collect::<Vec<_>>();

        assert_eq!(
            crossing_positions,
            [
                TopologyPosition { x: 2.0, y: 0.0 },
                TopologyPosition { x: 4.0, y: 0.0 },
            ]
        );
        assert_eq!(red_spans.len(), 3);
        assert_eq!(red_spans[0].0, 0.0);
        assert_eq!(red_spans[0].1, 1.0 / 3.0);
        assert_eq!(red_spans[1].0, 1.0 / 3.0);
        assert_eq!(red_spans[1].1, 2.0 / 3.0);
        assert_eq!(red_spans[2].0, 2.0 / 3.0);
        assert_eq!(red_spans[2].1, 1.0);
    }

    #[test]
    fn shared_track_crossing_preserves_every_source_path() {
        let source = topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 2.0, 0.0),
                ("C", 1.0, -1.0),
                ("D", 1.0, 1.0),
            ],
            &[
                ("red", &[(&["A", "B"], false)]),
                ("orange", &[(&["A", "B"], false)]),
                ("blue", &[(&["C", "D"], false)]),
            ],
        );
        let result = preprocess_topology(source).unwrap();

        assert_eq!(
            result
                .nodes
                .iter()
                .filter(|node| matches!(node, PreprocessedNode::VirtualCrossing { .. }))
                .count(),
            1
        );
        assert_eq!(result.paths.len(), 3);
        assert!(result.paths.iter().all(|path| path.traversals.len() == 2));
    }

    #[test]
    fn rejects_three_edges_at_one_point_with_detailed_diagnostic() {
        let source = topology(
            &[
                ("A", -2.0, 0.0),
                ("B", 2.0, 0.0),
                ("C", 0.0, -2.0),
                ("D", 0.0, 2.0),
                ("E", -2.0, -2.0),
                ("F", 2.0, 2.0),
            ],
            &[
                ("red", &[(&["A", "B"], false)]),
                ("blue", &[(&["C", "D"], false)]),
                ("green", &[(&["E", "F"], false)]),
            ],
        );
        let error = preprocess_topology(source).unwrap_err();
        let message = error.to_string();

        assert!(matches!(
            error,
            TopologyPreprocessError::UnsupportedTopologyIntersection {
                kind: UnsupportedIntersectionKind::MultiplePhysicalEdges { count: 3 },
                ref occurrences,
                ..
            } if occurrences.len() == 3
        ));
        assert!(message.contains("3 physical edges meet there"));
        assert!(message.contains("line 'red' path 1 segment 1 (A -> B)"));
        assert!(message.contains("line 'green' path 1 segment 1 (E -> F)"));
    }
}
