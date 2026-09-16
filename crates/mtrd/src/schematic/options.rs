use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap};

use crate::Languages;

use super::SchematicLength;

/// Global visual options used by a schematic map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicOptions {
    pub background: SchematicBackgroundOptions,
    pub languages: Languages,
    pub lines: SchematicLineOptions,
    pub stations: SchematicStationOptions,
}

/// An opaque colour or a transparent map background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchematicBackgroundOptions {
    Color { color: String },
    Transparent,
}

impl Serialize for SchematicBackgroundOptions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            Self::Color { color } => map.serialize_entry("color", color)?,
            Self::Transparent => map.serialize_entry("transparent", &true)?,
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for SchematicBackgroundOptions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(alias = "colour")]
            color: Option<String>,
            transparent: Option<bool>,
        }

        match Fields::deserialize(deserializer)? {
            Fields {
                color: Some(color),
                transparent: None | Some(false),
            } => Ok(Self::Color { color }),
            Fields {
                color: None,
                transparent: Some(true),
            } => Ok(Self::Transparent),
            Fields {
                color: None,
                transparent: Some(false),
            } => Err(serde::de::Error::custom("transparent must be true")),
            Fields {
                color: Some(_),
                transparent: Some(true),
            } => Err(serde::de::Error::custom(
                "background color conflicts with transparent: true",
            )),
            Fields {
                color: None,
                transparent: None,
            } => Err(serde::de::Error::custom(
                "background must contain color or transparent: true",
            )),
        }
    }
}

/// Global styling shared by all metro lines.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicLineOptions {
    pub width: SchematicLength,
}

/// Global styling for common and interchange stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicStationOptions {
    pub common: SchematicCommonStationOptions,
    pub interchange: SchematicInterchangeStationOptions,
}

/// Styling shared by circle-shaped common stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicCommonStationOptions {
    pub fill: SchematicCommonStationFill,
    pub stroke: SchematicCommonStationStroke,
}

/// Fill styling for circle-shaped common stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicCommonStationFill {
    pub diameter: SchematicLength,
    #[serde(alias = "colour")]
    pub color: SchematicStationColor,
}

/// Stroke styling for circle-shaped common stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicCommonStationStroke {
    pub width: SchematicLength,
    pub alignment: SchematicStrokeAlignment,
    #[serde(alias = "colour")]
    pub color: SchematicStationColor,
}

/// Styling shared by capsule-shaped interchange stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicInterchangeStationOptions {
    pub fill: SchematicInterchangeStationFill,
    pub stroke: SchematicInterchangeStationStroke,
}

/// Fill styling for capsule-shaped interchange stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicInterchangeStationFill {
    pub width: SchematicLength,
    #[serde(alias = "colour")]
    pub color: String,
}

/// Stroke styling for capsule-shaped interchange stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SchematicInterchangeStationStroke {
    pub width: SchematicLength,
    pub alignment: SchematicStrokeAlignment,
    #[serde(alias = "colour")]
    pub color: String,
}

/// How a common-station colour is selected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "kebab-case",
    deny_unknown_fields
)]
pub enum SchematicStationColor {
    Unified { value: String },
    // The empty struct makes `deny_unknown_fields` apply to this fieldless case.
    FollowLine {},
}

/// Placement of a station stroke relative to its fill boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchematicStrokeAlignment {
    Inside,
    #[serde(alias = "centre")]
    Center,
    Outside,
}
