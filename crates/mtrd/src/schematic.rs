mod geometry;
mod options;
mod render;
mod route;
mod station;
mod validation;

use serde::{Deserialize, Serialize};

use crate::manifest_format::canonicalize_yaml;

pub use geometry::{SchematicLength, SchematicPoint, SchematicValueError};
pub use options::{
    SchematicBackgroundOptions, SchematicCommonStationFill, SchematicCommonStationOptions,
    SchematicCommonStationStroke, SchematicInterchangeStationFill,
    SchematicInterchangeStationOptions, SchematicInterchangeStationStroke, SchematicLineOptions,
    SchematicOptions, SchematicStationColor, SchematicStationOptions, SchematicStrokeAlignment,
};
pub use render::render_schematic_svg;
pub use route::{SchematicCorner, SchematicLine, SchematicPath, SchematicRouteVisit};
pub use station::{
    OctilinearAxis, SchematicInterchangePort, SchematicStation, SchematicStationPort,
    SchematicStationSymbol,
};
pub use validation::{SchematicRenderError, validate_schematic};

/// A complete, human-editable schematic-map manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicManifest {
    pub options: SchematicOptions,
    pub stations: Vec<SchematicStation>,
    pub corners: Vec<SchematicCorner>,
    pub lines: Vec<SchematicLine>,
}
impl SchematicManifest {
    /// Deserialize a semantic schematic manifest from YAML.
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Serialize a semantic schematic manifest to YAML.
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self).map(canonicalize_yaml)
    }

    /// Deserialize a semantic schematic manifest from equivalent JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize a semantic schematic manifest as compact JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMATIC_YAML: &str = r##"
options:
  languages: { set: [en], primary: en }
  background:
    color: "#ffffff"
  lines:
    width: 8.0
  stations:
    common:
      fill:
        diameter: 18.0
        color:
          type: unified
          value: "#ffffff"
      stroke:
        width: 2.0
        alignment: centre
        color:
          type: follow-line
    interchange:
      fill:
        width: 18.0
        color: "#ffffff"
      stroke:
        width: 2.0
        alignment: outside
        color: "#000000"

stations:
  - id: west
    position: [0.0, 40.0]
    names:
      en:
        - West
    symbol:
      type: circle

  - id: central
    position: [80.0, 80.0]
    names:
      en:
        - Central
    symbol:
      type: capsule
      axis: rising-diagonal
      anchor-count: 1
      anchor-interval: 24.0

corners:
  - id: west-corner
    position: [40.0, 40.0]
    radius: 8.0

lines:
  - id: line-a
    names:
      en:
        - Line A
    color: "#e2231a"
    paths:
      - visits:
          - type: station
            station-id: west
            port:
              type: single-line
          - type: corner
            corner-id: west-corner
          - type: station
            station-id: central
            port:
              type: interchange
              interchange:
                type: single-perpendicular
        closed: false
"##;

    #[test]
    fn deserializes_schematic_station_ports() {
        let schematic = SchematicManifest::from_yaml(SCHEMATIC_YAML).unwrap();

        assert_eq!(schematic.options.lines.width.get(), 8.0);
        assert_eq!(
            schematic.options.background,
            SchematicBackgroundOptions::Color {
                color: "#ffffff".to_owned()
            }
        );
        assert_eq!(
            schematic.options.stations.common.stroke.alignment,
            SchematicStrokeAlignment::Center
        );
        assert_eq!(schematic.stations[0].position.x(), 0.0);
        assert_eq!(schematic.stations[0].position.y(), 40.0);
        assert_eq!(
            schematic.lines[0].paths[0].visits[2],
            SchematicRouteVisit::Station {
                station_id: "central".to_owned(),
                port: SchematicStationPort::Interchange(
                    SchematicInterchangePort::SinglePerpendicular {},
                ),
            }
        );
    }

    #[test]
    fn round_trips_schematic_manifest_through_yaml() {
        let schematic = SchematicManifest::from_yaml(SCHEMATIC_YAML).unwrap();
        let encoded = schematic.to_yaml().unwrap();
        let decoded = SchematicManifest::from_yaml(&encoded).unwrap();
        let value = serde_yaml::from_str::<serde_yaml::Value>(&encoded).unwrap();

        assert_eq!(decoded, schematic);
        assert!(encoded.contains("position: [0.0, 40.0]"));
        assert!(encoded.contains("en: [West]"));
        assert!(encoded.contains("en: [Line A]"));
        assert!(encoded.contains("alignment: center"));
        assert!(!encoded.contains("alignment: centre"));
        assert_eq!(value["options"]["background"]["color"], "#ffffff");
        assert!(value["options"]["background"]["colour"].is_null());
        assert!(value["options"]["background"]["transparent"].is_null());
        assert_eq!(value["stations"][1]["symbol"]["axis"], "rising-diagonal");
        assert_eq!(value["stations"][1]["symbol"]["anchor-count"], 1);
        assert!(value["stations"][1]["symbol"]["anchor_count"].is_null());
        assert_eq!(
            value["lines"][0]["paths"][0]["visits"][2]["station-id"],
            "central"
        );
        assert!(value["lines"][0]["paths"][0]["visits"][2]["station_id"].is_null());
        assert_eq!(
            value["lines"][0]["paths"][0]["visits"][2]["port"]["interchange"]["type"],
            "single-perpendicular"
        );
        assert!(value["lines"][0]["paths"][0]["visits"][2]["port"]["port"].is_null());
    }

    #[test]
    fn rejects_snake_case_configuration_tokens() {
        let snake_case_key = SCHEMATIC_YAML.replace("anchor-count: 1", "anchor_count: 1");
        let snake_case_value = SCHEMATIC_YAML.replace("type: single-line", "type: single_line");

        assert!(SchematicManifest::from_yaml(&snake_case_key).is_err());
        assert!(SchematicManifest::from_yaml(&snake_case_value).is_err());
    }

    #[test]
    fn round_trips_schematic_manifest_through_json() {
        let schematic = SchematicManifest::from_yaml(SCHEMATIC_YAML).unwrap();
        let encoded = schematic.to_json().unwrap();
        let decoded = SchematicManifest::from_json(&encoded).unwrap();

        assert_eq!(decoded, schematic);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["lines"][0]["paths"][0]["visits"]
                [2]["port"]["interchange"]["type"],
            "single-perpendicular"
        );
    }

    #[test]
    fn rejects_schematic_name_languages_outside_global_set() {
        let mut schematic = SchematicManifest::from_yaml(SCHEMATIC_YAML).unwrap();
        schematic.lines[0]
            .names
            .insert("fr-ch".into(), vec!["Ligne A".into()]);
        assert!(matches!(
            validate_schematic(&schematic),
            Err(SchematicRenderError::Languages(
                crate::LanguageError::NameLanguages { kind: "line", .. }
            ))
        ));

        schematic.lines[0].names.remove("fr-ch");
        schematic.stations[0].names.remove("en");
        assert!(matches!(
            validate_schematic(&schematic),
            Err(SchematicRenderError::Languages(
                crate::LanguageError::NameLanguages {
                    kind: "station",
                    ..
                }
            ))
        ));
    }

    #[test]
    fn accepts_british_spellings_and_serializes_canonically() {
        let british_yaml = SCHEMATIC_YAML.replace("color:", "colour:");
        let schematic = SchematicManifest::from_yaml(&british_yaml).unwrap();
        let canonical_yaml = schematic.to_yaml().unwrap();
        let british_json = schematic
            .to_json()
            .unwrap()
            .replace("\"color\":", "\"colour\":")
            .replace("\"center\"", "\"centre\"");

        assert_eq!(
            SchematicManifest::from_json(&british_json).unwrap(),
            schematic
        );
        assert!(canonical_yaml.contains("alignment: center"));
        assert!(canonical_yaml.contains("color:"));
        assert!(!canonical_yaml.contains("alignment: centre"));
        assert!(!canonical_yaml.contains("colour:"));
        assert!(
            schematic
                .to_json()
                .unwrap()
                .contains("\"alignment\":\"center\"")
        );
        assert!(schematic.to_json().unwrap().contains("\"color\":"));
    }

    #[test]
    fn rejects_repeated_port_key_for_interchange_payload() {
        let yaml = SCHEMATIC_YAML.replace(
            "              interchange:\n                type: single-perpendicular",
            "              port:\n                type: single-perpendicular",
        );

        assert!(SchematicManifest::from_yaml(&yaml).is_err());
    }

    #[test]
    fn rejects_invalid_schematic_scalars() {
        let non_finite_point =
            SCHEMATIC_YAML.replace("position: [0.0, 40.0]", "position: [.nan, 40.0]");
        let zero_length = SCHEMATIC_YAML.replace("    width: 8.0", "    width: 0.0");

        assert!(SchematicManifest::from_yaml(&non_finite_point).is_err());
        assert!(SchematicManifest::from_yaml(&zero_length).is_err());
        assert_eq!(
            SchematicLength::new(-1.0),
            Err(SchematicValueError::InvalidLength(-1.0))
        );
    }

    #[test]
    fn enforces_exclusive_background_variants() {
        let color_background = "  background:\n    color: \"#ffffff\"";
        let transparent =
            SCHEMATIC_YAML.replace(color_background, "  background:\n    transparent: true");
        let decoded = SchematicManifest::from_yaml(&transparent).unwrap();
        let encoded = decoded.to_yaml().unwrap();
        let value = serde_yaml::from_str::<serde_yaml::Value>(&encoded).unwrap();

        assert_eq!(
            decoded.options.background,
            SchematicBackgroundOptions::Transparent
        );
        assert_eq!(value["options"]["background"]["transparent"], true);
        assert!(value["options"]["background"]["color"].is_null());
        for spelling in ["color", "colour"] {
            let opaque = SCHEMATIC_YAML.replace(
                color_background,
                &format!("  background:\n    {spelling}: \"#ffffff\"\n    transparent: false"),
            );
            let decoded = SchematicManifest::from_yaml(&opaque).unwrap();
            let canonical =
                serde_yaml::from_str::<serde_yaml::Value>(&decoded.to_yaml().unwrap()).unwrap();

            assert_eq!(
                decoded.options.background,
                SchematicBackgroundOptions::Color {
                    color: "#ffffff".to_owned()
                }
            );
            assert_eq!(canonical["options"]["background"]["color"], "#ffffff");
            assert!(canonical["options"]["background"]["transparent"].is_null());
            assert!(canonical["options"]["background"]["colour"].is_null());
        }
        assert!(
            SchematicManifest::from_yaml(&SCHEMATIC_YAML.replace(
                color_background,
                "  background:\n    color: \"#ffffff\"\n    transparent: true"
            ))
            .is_err()
        );
        assert!(
            SchematicManifest::from_yaml(
                &SCHEMATIC_YAML.replace(color_background, "  background:\n    transparent: false")
            )
            .is_err()
        );
        assert!(
            SchematicManifest::from_yaml(
                &SCHEMATIC_YAML.replace(color_background, "  background: {}")
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_unknown_schematic_fields() {
        let invalid_port = SCHEMATIC_YAML.replace(
            "                type: single-perpendicular",
            "                type: single-perpendicular\n                index: 0",
        );
        let invalid_circle = SCHEMATIC_YAML.replace(
            "    symbol:\n      type: circle",
            "    symbol:\n      type: circle\n      diameter: 18.0",
        );
        let invalid_single_line = SCHEMATIC_YAML.replace(
            "              type: single-line",
            "              type: single-line\n              index: 0",
        );
        let invalid_color = SCHEMATIC_YAML.replace(
            "          type: follow-line",
            "          type: follow-line\n          value: \"#ffffff\"",
        );
        let obsolete_flat_option = SCHEMATIC_YAML.replace(
            "  lines:\n    width: 8.0",
            "  line-width: 8.0\n  lines:\n    width: 8.0",
        );

        assert!(SchematicManifest::from_yaml(&invalid_port).is_err());
        assert!(SchematicManifest::from_yaml(&invalid_circle).is_err());
        assert!(SchematicManifest::from_yaml(&invalid_single_line).is_err());
        assert!(SchematicManifest::from_yaml(&invalid_color).is_err());
        assert!(SchematicManifest::from_yaml(&obsolete_flat_option).is_err());
    }
}
