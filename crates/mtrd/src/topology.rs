mod layout;
mod options;
pub(crate) mod preprocess;
mod render;
mod validation;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::{LocalizedNames, SchematicManifest, manifest_format::canonicalize_yaml};

pub use options::{
    TopologyBackgroundOptions, TopologyCartesianAxes, TopologyCommonStationFill,
    TopologyCommonStationOptions, TopologyCommonStationStroke, TopologyCoordinateOptions,
    TopologyGeographicAxes, TopologyInterchangeStationFill, TopologyInterchangeStationOptions,
    TopologyInterchangeStationStroke, TopologyLabelOptions, TopologyLength, TopologyLineOptions,
    TopologyOptions, TopologyScale, TopologyStationColor, TopologyStationOptions,
    TopologyStrokeAlignment, TopologyValueError,
};
pub use preprocess::{
    EdgeEndpoint, IncidentEdge, LocatedOccurrence, PreprocessedEdge, PreprocessedNode,
    PreprocessedPath, PreprocessedTopology, ReducedTraversal, RetentionReason, SourceSegmentSpan,
    StationNeighborOrder, TopologyPreprocessError, UnsupportedIntersectionKind,
    VirtualContinuation, preprocess_topology,
};
pub use render::render_topology_svg;
pub use validation::{
    DuplicateStationPositionGroup, DuplicateStationPositionGroups, TopologyRenderError,
    validate_topology,
};

#[derive(Debug, Error, PartialEq)]
pub enum SchematicGenerationError {
    #[error(transparent)]
    Preprocess(#[from] TopologyPreprocessError),

    #[error("schematic generation is not implemented yet")]
    StageUnavailable,
}

/// Validate and preprocess a topology before the remaining generation stages.
///
/// The layout stages are deliberately not available yet, so a successfully
/// preprocessed topology currently returns [`SchematicGenerationError::StageUnavailable`].
pub fn generate_schematic(
    topology: &MetroTopology,
) -> Result<SchematicManifest, SchematicGenerationError> {
    let _preprocessed = preprocess::preprocess_topology(topology.clone())?;
    Err(SchematicGenerationError::StageUnavailable)
}

/// An entire metro topology manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MetroTopology {
    pub options: TopologyOptions,
    pub stations: Vec<TopologyStation>,
    pub lines: Vec<TopologyLine>,
}

/// A station and its position in the topology graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyStation {
    pub id: String,
    pub names: LocalizedNames,
    pub position: TopologyPosition,
}

/// A point in the topology's configured coordinate system.
///
/// It is serialized and deserialized as `[x, y]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TopologyPosition {
    pub x: f64,
    pub y: f64,
}

impl Serialize for TopologyPosition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [self.x, self.y].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TopologyPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [x, y] = <[f64; 2]>::deserialize(deserializer)?;
        Ok(Self { x, y })
    }
}

/// A topological metro line composed of one or more paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyLine {
    pub id: String,
    pub names: LocalizedNames,
    #[serde(alias = "colour")]
    pub color: String,
    pub paths: Vec<TopologyPath>,
}

/// An ordered topological traversal of stations belonging to a line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyPath {
    pub stations: Vec<String>,
    pub closed: bool,
}

impl MetroTopology {
    /// Deserialize a metro topology from a YAML manifest.
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Serialize a metro topology as a YAML manifest.
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self).map(canonicalize_yaml)
    }

    /// Deserialize a metro topology from JSON using the same schema as YAML.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize a metro topology as compact JSON suitable for transport to a WebUI.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOPOLOGY_YAML: &str = r#"
options:
  background:
    color: '#ffffff'
  labels:
    hidden: false
  lines:
    width: 8.0
  stations:
    common:
      fill:
        diameter: 18.0
        color:
          type: unified
          value: '#ffffff'
      stroke:
        width: 2.0
        alignment: center
        color:
          type: follow-line
    interchange:
      fill:
        width: 18.0
        color: '#ffffff'
      stroke:
        width: 2.0
        alignment: outside
        color: '#000000'
stations:
  - id: futian
    names:
      zh-CN:
        - 福田
      en:
        - Futian
    position: [10.0, 20.0]

  - id: airport
    names:
      zh-CN:
        - 机场
      en:
        - Airport
    position: [90.0, 20.0]

lines:
  - id: line-11
    names:
      zh-CN:
        - 11 号线
        - 机场线
      en:
        - Line 11
        - Airport Express
    color: '#672146'
    paths:
      - stations:
          - futian
          - airport
        closed: false
"#;

    #[test]
    fn deserializes_position_sequence() {
        let topology = MetroTopology::from_yaml(TOPOLOGY_YAML).unwrap();

        assert_eq!(topology.stations.len(), 2);
        assert_eq!(
            topology.stations[0].position,
            TopologyPosition { x: 10.0, y: 20.0 }
        );
        assert_eq!(
            topology.stations[1].position,
            TopologyPosition { x: 90.0, y: 20.0 }
        );
        assert_eq!(topology.lines[0].names["en"][0], "Line 11");
        assert_eq!(topology.lines[0].names["en"][1], "Airport Express");
        assert_eq!(topology.lines[0].color, "#672146");
        assert_eq!(
            topology.options.background,
            TopologyBackgroundOptions::Color {
                color: "#ffffff".into()
            }
        );
        assert_eq!(topology.options.lines.width.get(), 8.0);
        assert_eq!(topology.options.scale.get(), 1.0);
        assert_eq!(
            topology.options.coordinates,
            TopologyCoordinateOptions::Cartesian {
                axes: TopologyCartesianAxes::RightDown
            }
        );
        assert!(!topology.options.labels.hidden);
        assert_eq!(
            topology.options.stations.common.stroke.alignment,
            TopologyStrokeAlignment::Center
        );
        assert!(!topology.lines[0].paths[0].closed);
    }

    #[test]
    fn round_trips_through_yaml() {
        let topology = MetroTopology::from_yaml(TOPOLOGY_YAML).unwrap();
        let encoded = topology.to_yaml().unwrap();
        let decoded = MetroTopology::from_yaml(&encoded).unwrap();

        assert_eq!(decoded, topology);
        assert!(encoded.contains("position: [10.0, 20.0]"));
        assert!(encoded.contains("position: [90.0, 20.0]"));
        assert!(!encoded.contains("position:\n"));
        assert!(encoded.contains("en: [Futian]"));
        assert!(encoded.contains("zh-CN: [福田]"));
        assert!(encoded.contains("en: [Line 11, Airport Express]"));
        assert!(encoded.contains("  coordinates:\n    type: cartesian\n    axes: r-d"));
        assert!(encoded.contains("  scale: 1.0"));
    }

    #[test]
    fn converts_from_yaml_to_json_and_back() {
        let topology = MetroTopology::from_yaml(TOPOLOGY_YAML).unwrap();
        let encoded = topology.to_json().unwrap();
        let decoded = MetroTopology::from_json(&encoded).unwrap();

        assert_eq!(decoded, topology);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["stations"][0]["position"],
            serde_json::json!([10.0, 20.0])
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["lines"][0]["names"]["en"]
                [0],
            "Line 11"
        );
    }

    #[test]
    fn converts_from_json_to_primary_yaml_format() {
        let topology = MetroTopology::from_yaml(TOPOLOGY_YAML).unwrap();
        let json = topology.to_json().unwrap();

        let yaml = MetroTopology::from_json(&json).unwrap().to_yaml().unwrap();

        assert_eq!(MetroTopology::from_yaml(&yaml).unwrap(), topology);
    }

    #[test]
    fn accepts_british_colour_and_serializes_canonically() {
        let british_yaml = TOPOLOGY_YAML
            .replace("color:", "colour:")
            .replace("alignment: center", "alignment: centre");
        let topology = MetroTopology::from_yaml(&british_yaml).unwrap();
        let canonical_yaml = topology.to_yaml().unwrap();
        let british_json = topology
            .to_json()
            .unwrap()
            .replace("\"color\":", "\"colour\":");

        assert_eq!(MetroTopology::from_json(&british_json).unwrap(), topology);
        assert!(canonical_yaml.contains("  color:"));
        assert!(!canonical_yaml.contains("colour:"));
        assert!(canonical_yaml.contains("alignment: center"));
        assert!(!canonical_yaml.contains("alignment: centre"));
        assert!(
            topology
                .to_json()
                .unwrap()
                .contains("\"color\":\"#672146\"")
        );
    }

    #[test]
    fn enforces_exclusive_background_variants() {
        let color_background = "  background:\n    color: '#ffffff'";
        let transparent_yaml =
            TOPOLOGY_YAML.replace(color_background, "  background:\n    transparent: true");
        let transparent = MetroTopology::from_yaml(&transparent_yaml).unwrap();
        let canonical: serde_yaml::Value =
            serde_yaml::from_str(&transparent.to_yaml().unwrap()).unwrap();

        assert_eq!(
            transparent.options.background,
            TopologyBackgroundOptions::Transparent
        );
        assert_eq!(canonical["options"]["background"]["transparent"], true);
        assert!(canonical["options"]["background"]["color"].is_null());

        for spelling in ["color", "colour"] {
            let compatible_yaml = TOPOLOGY_YAML.replace(
                color_background,
                &format!("  background:\n    {spelling}: '#ffffff'\n    transparent: false"),
            );
            let compatible = MetroTopology::from_yaml(&compatible_yaml).unwrap();
            let canonical: serde_yaml::Value =
                serde_yaml::from_str(&compatible.to_yaml().unwrap()).unwrap();

            assert_eq!(
                compatible.options.background,
                TopologyBackgroundOptions::Color {
                    color: "#ffffff".into()
                }
            );
            assert_eq!(canonical["options"]["background"]["color"], "#ffffff");
            assert!(canonical["options"]["background"]["transparent"].is_null());
            assert!(canonical["options"]["background"]["colour"].is_null());
        }

        for invalid_background in [
            "  background:\n    color: '#ffffff'\n    transparent: true",
            "  background:\n    transparent: false",
            "  background: {}",
            "  background:\n    color: '#ffffff'\n    unexpected: true",
        ] {
            assert!(
                MetroTopology::from_yaml(
                    &TOPOLOGY_YAML.replace(color_background, invalid_background)
                )
                .is_err()
            );
        }

        let (_, manifest_body) = TOPOLOGY_YAML.split_once("stations:\n").unwrap();
        assert!(MetroTopology::from_yaml(&format!("stations:\n{manifest_body}")).is_err());
    }

    #[test]
    fn rejects_position_mapping() {
        let yaml = TOPOLOGY_YAML.replace(
            "position: [10.0, 20.0]",
            "position:\n      x: 10.0\n      y: 20.0",
        );

        assert!(MetroTopology::from_yaml(&yaml).is_err());
    }

    #[test]
    fn rejects_position_sequences_with_the_wrong_length() {
        let yaml = TOPOLOGY_YAML.replace("position: [90.0, 20.0]", "position: [90.0]");

        assert!(MetroTopology::from_yaml(&yaml).is_err());
    }

    #[test]
    fn rejects_invalid_rendering_lengths() {
        for invalid in ["0.0", "-1.0", ".inf", ".nan"] {
            let yaml = TOPOLOGY_YAML.replace("    width: 8.0", &format!("    width: {invalid}"));

            assert!(MetroTopology::from_yaml(&yaml).is_err());
        }
    }

    #[test]
    fn accepts_coordinate_scale_and_rejects_invalid_values() {
        let scaled = MetroTopology::from_yaml(
            &TOPOLOGY_YAML.replace("  labels:", "  scale: 3.0\n  labels:"),
        )
        .unwrap();
        let canonical: serde_yaml::Value =
            serde_yaml::from_str(&scaled.to_yaml().unwrap()).unwrap();

        assert_eq!(scaled.options.scale.get(), 3.0);
        assert_eq!(canonical["options"]["scale"], 3.0);

        for invalid in ["0.0", "-1.0", ".inf", ".nan"] {
            let yaml =
                TOPOLOGY_YAML.replace("  labels:", &format!("  scale: {invalid}\n  labels:"));
            assert!(MetroTopology::from_yaml(&yaml).is_err());
        }
    }

    #[test]
    fn applies_defaults_for_omitted_topology_options() {
        let yaml = TOPOLOGY_YAML
            .replace("  background:\n    color: '#ffffff'\n", "")
            .replace("  labels:\n    hidden: false\n", "");
        let topology = MetroTopology::from_yaml(&yaml).unwrap();
        let canonical: serde_yaml::Value =
            serde_yaml::from_str(&topology.to_yaml().unwrap()).unwrap();

        assert_eq!(
            topology.options.background,
            TopologyBackgroundOptions::Color {
                color: "#FFFFFF".to_owned()
            }
        );
        assert_eq!(
            topology.options.coordinates,
            TopologyCoordinateOptions::default()
        );
        assert_eq!(topology.options.labels, TopologyLabelOptions::default());
        assert_eq!(topology.options.scale, TopologyScale::default());
        assert_eq!(canonical["options"]["background"]["color"], "#FFFFFF");
        assert_eq!(canonical["options"]["coordinates"]["type"], "cartesian");
        assert_eq!(canonical["options"]["coordinates"]["axes"], "r-d");
        assert_eq!(canonical["options"]["labels"]["hidden"], false);
        assert_eq!(canonical["options"]["scale"], 1.0);

        let empty_labels = yaml.replace("  lines:", "  labels: {}\n  lines:");
        assert_eq!(
            MetroTopology::from_yaml(&empty_labels)
                .unwrap()
                .options
                .labels,
            TopologyLabelOptions::default()
        );
    }

    #[test]
    fn rejects_unknown_or_non_boolean_label_options() {
        for invalid in ["    visible: false", "    hidden: 'false'"] {
            let yaml = TOPOLOGY_YAML.replace("    hidden: false", invalid);

            assert!(MetroTopology::from_yaml(&yaml).is_err());
        }
    }

    #[test]
    fn applies_type_specific_coordinate_axis_defaults() {
        let geographic = MetroTopology::from_yaml(&TOPOLOGY_YAML.replace(
            "  labels:",
            "  coordinates:\n    type: geographic\n  labels:",
        ))
        .unwrap();
        assert_eq!(
            geographic.options.coordinates,
            TopologyCoordinateOptions::Geographic {
                axes: TopologyGeographicAxes::EastNorth
            }
        );
        let canonical: serde_yaml::Value =
            serde_yaml::from_str(&geographic.to_yaml().unwrap()).unwrap();
        assert_eq!(canonical["options"]["coordinates"]["type"], "geographic");
        assert_eq!(canonical["options"]["coordinates"]["axes"], "e-n");
    }

    #[test]
    fn accepts_every_coordinate_axis_token() {
        for (coordinate_type, axes) in [
            (
                "cartesian",
                ["r-d", "r-u", "l-d", "l-u", "d-r", "d-l", "u-r", "u-l"],
            ),
            (
                "geographic",
                ["e-n", "e-s", "w-n", "w-s", "n-e", "n-w", "s-e", "s-w"],
            ),
        ] {
            for axes in axes {
                let yaml = TOPOLOGY_YAML.replace(
                    "  labels:",
                    &format!(
                        "  coordinates:\n    type: {coordinate_type}\n    axes: {axes}\n  labels:"
                    ),
                );
                assert!(MetroTopology::from_yaml(&yaml).is_ok());
            }
        }
    }

    #[test]
    fn rejects_invalid_coordinate_options() {
        for coordinates in [
            "    axes: r-d",
            "    type: cartesian\n    axes: e-n",
            "    type: geographic\n    axes: r-d",
            "    type: geographic\n    axes: e-n\n    unexpected: true",
        ] {
            let yaml = TOPOLOGY_YAML.replace(
                "  labels:",
                &format!("  coordinates:\n{coordinates}\n  labels:"),
            );
            assert!(MetroTopology::from_yaml(&yaml).is_err());
        }
    }
}
