mod render;

use mtrd::{
    MetroTopology, PreprocessedNode, PreprocessedTopology as InnerPreprocessedTopology,
    TopologyPreprocessError, preprocess_topology,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreprocessedTopology(InnerPreprocessedTopology);

#[derive(Debug, Error)]
pub(crate) enum PreprocessedManifestError {
    #[error("invalid preprocessed topology YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("invalid preprocessed topology JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid preprocessed topology manifest: {0}")]
    InvalidReference(String),
}

impl PreprocessedTopology {
    pub(crate) fn generate(topology: MetroTopology) -> Result<Self, TopologyPreprocessError> {
        preprocess_topology(topology).map(Self)
    }

    pub(crate) fn from_yaml(yaml: &str) -> Result<Self, PreprocessedManifestError> {
        let topology = serde_yaml::from_str(yaml)?;
        validate_references(&topology)?;
        Ok(Self(topology))
    }

    pub(crate) fn to_yaml(&self) -> Result<String, PreprocessedManifestError> {
        serde_yaml::to_string(&self.0)
            .map(inline_yaml_positions)
            .map_err(Into::into)
    }

    pub(crate) fn from_json(json: &str) -> Result<Self, PreprocessedManifestError> {
        let topology = serde_json::from_str(json)?;
        validate_references(&topology)?;
        Ok(Self(topology))
    }

    pub(crate) fn to_json(&self) -> Result<String, PreprocessedManifestError> {
        serde_json::to_string(&self.0).map_err(Into::into)
    }

    pub(crate) fn render_svg(&self) -> String {
        render::render_preprocessed_topology_svg(&self.0)
    }
}

fn validate_references(
    topology: &InnerPreprocessedTopology,
) -> Result<(), PreprocessedManifestError> {
    let node_count = topology.nodes.len();
    let edge_count = topology.edges.len();
    let station_count = topology.source.stations.len();

    for (edge_index, edge) in topology.edges.iter().enumerate() {
        for (name, endpoint) in [
            ("endpoint_a", edge.endpoint_a),
            ("endpoint_b", edge.endpoint_b),
        ] {
            if endpoint >= node_count {
                return Err(invalid_reference(format!(
                    "edges[{edge_index}].{name} refers to node {endpoint}, but there are {node_count} nodes"
                )));
            }
        }
        for &station in &edge.source_station_indices {
            if station >= station_count {
                return Err(invalid_reference(format!(
                    "edges[{edge_index}] refers to source station {station}, but there are {station_count} stations"
                )));
            }
        }
    }

    for (node_index, node) in topology.nodes.iter().enumerate() {
        match node {
            PreprocessedNode::Station {
                source_station_index,
                ..
            } if *source_station_index >= station_count => {
                return Err(invalid_reference(format!(
                    "nodes[{node_index}] refers to source station {source_station_index}, but there are {station_count} stations"
                )));
            }
            PreprocessedNode::VirtualCrossing {
                incident_edges,
                continuations,
                ..
            } => {
                for incident in incident_edges.iter().chain(
                    continuations
                        .iter()
                        .flat_map(|continuation| continuation.incident_edges.iter()),
                ) {
                    if incident.edge_index >= edge_count {
                        return Err(invalid_reference(format!(
                            "nodes[{node_index}] refers to edge {}, but there are {edge_count} edges",
                            incident.edge_index
                        )));
                    }
                }
            }
            _ => {}
        }
    }

    for (path_index, path) in topology.paths.iter().enumerate() {
        let Some(line) = topology.source.lines.get(path.line_index) else {
            return Err(invalid_reference(format!(
                "paths[{path_index}] refers to source line {}, but there are {} lines",
                path.line_index,
                topology.source.lines.len()
            )));
        };
        if path.path_index >= line.paths.len() {
            return Err(invalid_reference(format!(
                "paths[{path_index}] refers to source path {}, but line {} has {} paths",
                path.path_index,
                path.line_index,
                line.paths.len()
            )));
        }
        for traversal in &path.traversals {
            if traversal.edge_index >= edge_count {
                return Err(invalid_reference(format!(
                    "paths[{path_index}] refers to edge {}, but there are {edge_count} edges",
                    traversal.edge_index
                )));
            }
        }
    }

    Ok(())
}

fn invalid_reference(message: String) -> PreprocessedManifestError {
    PreprocessedManifestError::InvalidReference(message)
}

fn inline_yaml_positions(yaml: String) -> String {
    let lines: Vec<_> = yaml.lines().collect();
    let mut output = String::with_capacity(yaml.len());
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        if line.trim() == "position:"
            && let (Some(x), Some(y)) = (lines.get(index + 1), lines.get(index + 2))
            && let (Some(x), Some(y)) = (x.trim().strip_prefix("- "), y.trim().strip_prefix("- "))
        {
            let indentation = &line[..line.len() - line.trim_start().len()];
            output.push_str(indentation);
            output.push_str("position: [");
            output.push_str(x);
            output.push_str(", ");
            output.push_str(y);
            output.push_str("]\n");
            index += 3;
            continue;
        }

        output.push_str(line);
        output.push('\n');
        index += 1;
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtrd::{TopologyLine, TopologyOptions, TopologyPath, TopologyPosition, TopologyStation};

    fn options() -> TopologyOptions {
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

    fn topology(stations: &[(&str, f64, f64)], paths: &[(&str, &[&str])]) -> MetroTopology {
        MetroTopology {
            options: options(),
            stations: stations
                .iter()
                .map(|&(id, x, y)| TopologyStation {
                    id: id.into(),
                    names: Default::default(),
                    position: TopologyPosition { x, y },
                })
                .collect(),
            lines: paths
                .iter()
                .map(|(id, stations)| TopologyLine {
                    id: (*id).into(),
                    names: Default::default(),
                    color: "#f00".into(),
                    paths: vec![TopologyPath {
                        stations: stations.iter().map(|station| (*station).into()).collect(),
                        closed: false,
                    }],
                })
                .collect(),
        }
    }

    #[test]
    fn round_trips_yaml_and_json_and_renders_svg() {
        let topology = PreprocessedTopology::generate(topology(
            &[
                ("A", 0.0, 0.0),
                ("B", 1.0, 1.0),
                ("C", 2.0, 0.0),
                ("D", 1.0, 2.0),
            ],
            &[("red", &["A", "B", "C"]), ("blue", &["D", "B"])],
        ))
        .unwrap();
        let yaml = topology.to_yaml().unwrap();
        let json = topology.to_json().unwrap();

        assert!(yaml.contains("neighbor_orders:"));
        assert!(yaml.contains("neighbor_groups:"));
        assert!(!yaml.contains("neighbour_orders:"));
        assert_eq!(PreprocessedTopology::from_yaml(&yaml).unwrap(), topology);
        assert_eq!(PreprocessedTopology::from_json(&json).unwrap(), topology);
        let alias_yaml = yaml
            .replace("neighbor_orders:", "neighbour_orders:")
            .replace("neighbor_groups:", "neighbour_groups:");
        assert_eq!(
            PreprocessedTopology::from_yaml(&alias_yaml).unwrap(),
            topology
        );
        assert!(topology.render_svg().contains("viewBox=\""));
    }

    #[test]
    fn renders_contracted_stations_and_virtual_crossings() {
        let contracted = PreprocessedTopology::generate(topology(
            &[("A", 0.0, 0.0), ("B", 1.0, 1.0), ("C", 2.0, 0.0)],
            &[("red", &["A", "B", "C"])],
        ))
        .unwrap();
        let crossing = PreprocessedTopology::generate(topology(
            &[
                ("A", -1.0, -1.0),
                ("B", 1.0, 1.0),
                ("C", -1.0, 1.0),
                ("D", 1.0, -1.0),
            ],
            &[("red", &["A", "B"]), ("blue", &["C", "D"])],
        ))
        .unwrap();

        assert!(
            contracted
                .render_svg()
                .contains("data-contracted-station=\"1\"")
        );
        assert!(
            crossing
                .render_svg()
                .contains("data-virtual-crossing=\"true\"")
        );
        assert!(crossing.render_svg().contains("data-continuations="));
    }

    #[test]
    fn rejects_unknown_fields_and_invalid_references() {
        let topology = PreprocessedTopology::generate(topology(
            &[("A", 0.0, 0.0), ("B", 1.0, 1.0)],
            &[("red", &["A", "B"])],
        ))
        .unwrap();
        let yaml = topology.to_yaml().unwrap();
        let unknown = yaml.replacen("source:", "unknown: true\nsource:", 1);
        assert!(PreprocessedTopology::from_yaml(&unknown).is_err());

        let invalid = yaml.replacen("endpoint_a: 0", "endpoint_a: 99", 1);
        assert!(matches!(
            PreprocessedTopology::from_yaml(&invalid),
            Err(PreprocessedManifestError::InvalidReference(_))
        ));
    }
}
