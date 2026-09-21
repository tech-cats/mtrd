use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap};

use crate::Languages;

/// Global visual options used by a topology map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyOptions {
    #[serde(default)]
    pub background: TopologyBackgroundOptions,
    #[serde(default)]
    pub coordinates: TopologyCoordinateOptions,
    #[serde(default)]
    pub labels: TopologyLabelOptions,
    pub languages: Languages,
    pub lines: TopologyLineOptions,
    #[serde(default)]
    pub scale: TopologyScale,
    pub stations: TopologyStationOptions,
}

/// The coordinate system and axis orientation used by station positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "kebab-case",
    deny_unknown_fields
)]
pub enum TopologyCoordinateOptions {
    Cartesian {
        #[serde(default)]
        axes: TopologyCartesianAxes,
    },
    Geographic {
        #[serde(default)]
        axes: TopologyGeographicAxes,
    },
}

impl Default for TopologyCoordinateOptions {
    fn default() -> Self {
        Self::Cartesian {
            axes: TopologyCartesianAxes::default(),
        }
    }
}

impl TopologyCoordinateOptions {
    pub(super) fn canonical_cartesian(self, x: f64, y: f64) -> Option<(f64, f64)> {
        match self {
            Self::Cartesian { axes } => Some(axes.canonical(x, y)),
            Self::Geographic { .. } => None,
        }
    }

    pub(super) fn longitude_latitude(self, x: f64, y: f64) -> Option<(f64, f64)> {
        match self {
            Self::Cartesian { .. } => None,
            Self::Geographic { axes } => Some(axes.longitude_latitude(x, y)),
        }
    }
}

/// Positive directions of the first and second Cartesian coordinates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TopologyCartesianAxes {
    #[default]
    #[serde(rename = "r-d")]
    RightDown,
    #[serde(rename = "r-u")]
    RightUp,
    #[serde(rename = "l-d")]
    LeftDown,
    #[serde(rename = "l-u")]
    LeftUp,
    #[serde(rename = "d-r")]
    DownRight,
    #[serde(rename = "d-l")]
    DownLeft,
    #[serde(rename = "u-r")]
    UpRight,
    #[serde(rename = "u-l")]
    UpLeft,
}

impl TopologyCartesianAxes {
    fn canonical(self, x: f64, y: f64) -> (f64, f64) {
        match self {
            Self::RightDown => (x, y),
            Self::RightUp => (x, -y),
            Self::LeftDown => (-x, y),
            Self::LeftUp => (-x, -y),
            Self::DownRight => (y, x),
            Self::DownLeft => (-y, x),
            Self::UpRight => (y, -x),
            Self::UpLeft => (-y, -x),
        }
    }
}

/// Positive directions of the first and second geographic coordinates.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TopologyGeographicAxes {
    #[default]
    #[serde(rename = "n-e")]
    NorthEast,
    #[serde(rename = "e-n")]
    EastNorth,
    #[serde(rename = "s-e")]
    SouthEast,
    #[serde(rename = "e-s")]
    EastSouth,
    #[serde(rename = "n-w")]
    NorthWest,
    #[serde(rename = "w-n")]
    WestNorth,
    #[serde(rename = "s-w")]
    SouthWest,
    #[serde(rename = "w-s")]
    WestSouth,
}

impl TopologyGeographicAxes {
    fn longitude_latitude(self, x: f64, y: f64) -> (f64, f64) {
        match self {
            Self::EastNorth => (x, y),
            Self::EastSouth => (x, -y),
            Self::WestNorth => (-x, y),
            Self::WestSouth => (-x, -y),
            Self::NorthEast => (y, x),
            Self::NorthWest => (-y, x),
            Self::SouthEast => (y, -x),
            Self::SouthWest => (-y, -x),
        }
    }
}

/// Global display options for station-name labels.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyLabelOptions {
    #[serde(default)]
    pub hidden: bool,
}

/// Global styling shared by all metro lines.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyLineOptions {
    pub width: TopologyLength,
}

/// Global styling for common and interchange stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyStationOptions {
    pub common: TopologyCommonStationOptions,
    pub interchange: TopologyInterchangeStationOptions,
}

/// Styling shared by common stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyCommonStationOptions {
    pub fill: TopologyCommonStationFill,
    pub stroke: TopologyCommonStationStroke,
}

/// Fill styling for common stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyCommonStationFill {
    pub diameter: TopologyLength,
    #[serde(alias = "colour")]
    pub color: TopologyStationColor,
}

/// Stroke styling for common stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyCommonStationStroke {
    pub width: TopologyLength,
    pub alignment: TopologyStrokeAlignment,
    #[serde(alias = "colour")]
    pub color: TopologyStationColor,
}

/// Styling shared by interchange stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyInterchangeStationOptions {
    pub fill: TopologyInterchangeStationFill,
    pub stroke: TopologyInterchangeStationStroke,
}

/// Fill styling for interchange stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyInterchangeStationFill {
    pub width: TopologyLength,
    #[serde(alias = "colour")]
    pub color: String,
}

/// Stroke styling for interchange stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TopologyInterchangeStationStroke {
    pub width: TopologyLength,
    pub alignment: TopologyStrokeAlignment,
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
pub enum TopologyStationColor {
    Unified { value: String },
    // The empty struct makes `deny_unknown_fields` apply to this fieldless case.
    FollowLine {},
}

/// Placement of a station stroke relative to its fill boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TopologyStrokeAlignment {
    Inside,
    #[serde(alias = "centre")]
    Center,
    Outside,
}

/// A finite, strictly positive topology rendering length.
///
/// Geographic topology lengths are metres. Cartesian topology lengths use the
/// same unit as Cartesian station coordinates. The global coordinate scale
/// converts either unit to SVG user units.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct TopologyLength(f64);

impl TopologyLength {
    /// Construct a finite, strictly positive length.
    pub fn new(value: f64) -> Result<Self, TopologyValueError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(TopologyValueError::InvalidLength(value));
        }

        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl Serialize for TopologyLength {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TopologyLength {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// A finite, strictly positive multiplier applied to projected coordinates.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct TopologyScale(f64);

impl TopologyScale {
    /// Construct a finite, strictly positive coordinate scale.
    pub fn new(value: f64) -> Result<Self, TopologyValueError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(TopologyValueError::InvalidScale(value));
        }

        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl Default for TopologyScale {
    fn default() -> Self {
        Self(1.0)
    }
}

impl Serialize for TopologyScale {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TopologyScale {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// A scalar invariant violation while constructing topology rendering values.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum TopologyValueError {
    #[error("topology length must be finite and strictly positive, got {0}")]
    InvalidLength(f64),

    #[error("topology scale must be finite and strictly positive, got {0}")]
    InvalidScale(f64),
}

/// An opaque colour or a transparent map background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopologyBackgroundOptions {
    Color { color: String },
    Transparent,
}

impl Default for TopologyBackgroundOptions {
    fn default() -> Self {
        Self::Color {
            color: "#FFFFFF".to_owned(),
        }
    }
}

impl Serialize for TopologyBackgroundOptions {
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

impl<'de> Deserialize<'de> for TopologyBackgroundOptions {
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
