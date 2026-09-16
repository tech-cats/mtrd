use thiserror::Error;

use crate::LanguageError;

/// An invariant violation that prevents a schematic manifest from rendering.
#[derive(Debug, Error, PartialEq)]
pub enum SchematicRenderError {
    #[error("station id must not be empty")]
    EmptyStationId,
    #[error("station id '{station}' is defined more than once")]
    DuplicateStation { station: String },
    #[error("corner id must not be empty")]
    EmptyCornerId,
    #[error("corner id '{corner}' is defined more than once")]
    DuplicateCorner { corner: String },
    #[error("line id must not be empty")]
    EmptyLineId,
    #[error("line id '{line}' is defined more than once")]
    DuplicateLine { line: String },
    #[error(transparent)]
    Languages(#[from] LanguageError),
    #[error("line '{line}' refers to unknown station '{station}'")]
    UnknownStation { line: String, station: String },
    #[error("line '{line}' refers to unknown corner '{corner}'")]
    UnknownCorner { line: String, corner: String },
    #[error("corner '{corner}' is not referenced by a line")]
    UnreferencedCorner { corner: String },
    #[error("path {path} of line '{line}' must contain at least {minimum} station visits")]
    PathTooShort {
        line: String,
        path: usize,
        minimum: usize,
    },
    #[error("open path {path} of line '{line}' must begin and end at stations")]
    OpenPathEndpoint { line: String, path: usize },
    #[error("path {path} of line '{line}' visits station '{station}' more than once")]
    DuplicateStationInPath {
        line: String,
        path: usize,
        station: String,
    },
    #[error("path {path} of line '{line}' refers to corner '{corner}' more than once")]
    DuplicateCornerInPath {
        line: String,
        path: usize,
        corner: String,
    },
    #[error("station '{station}' uses a port incompatible with its symbol")]
    IncompatiblePort { station: String },
    #[error(
        "station '{station}' uses a perpendicular port incompatible with anchor-count {anchor_count}"
    )]
    IncompatiblePerpendicularPort { station: String, anchor_count: u8 },
    #[error(
        "station '{station}' uses perpendicular anchor {index}, outside anchor-count {anchor_count}"
    )]
    PerpendicularAnchorOutOfRange {
        station: String,
        index: u8,
        anchor_count: u8,
    },
    #[error("station '{station}' cannot use an oblique port with anchor-count {anchor_count}")]
    ObliquePortWithMultipleAnchors { station: String, anchor_count: u8 },
    #[error("station port '{port}' is referenced by lines '{first_line}' and '{second_line}'")]
    PortSharedByLines {
        port: String,
        first_line: String,
        second_line: String,
    },
    #[error("corner '{corner}' is referenced by lines '{first_line}' and '{second_line}'")]
    CornerSharedByLines {
        corner: String,
        first_line: String,
        second_line: String,
    },
    #[error(
        "station '{station}' must reference each of its {anchor_count} perpendicular anchors exactly once"
    )]
    IncompletePerpendicularPorts { station: String, anchor_count: u8 },
    #[error("station '{station}' anchor-interval must be at least the line width")]
    AnchorIntervalTooSmall { station: String },
    #[error("path {path} of line '{line}' has coincident consecutive points")]
    CoincidentPoints { line: String, path: usize },
    #[error("path {path} of line '{line}' contains a non-octilinear leg")]
    NonOctilinearLeg { line: String, path: usize },
    #[error("station '{station}' is used on an axis forbidden by its port")]
    InvalidPortAxis { station: String },
    #[error("path {path} of line '{line}' bends at station '{station}'")]
    BendAtStation {
        line: String,
        path: usize,
        station: String,
    },
    #[error("corner '{corner}' does not change direction")]
    CollinearCorner { corner: String },
    #[error("corner '{corner}' reverses direction")]
    ReversingCorner { corner: String },
    #[error("corner '{corner}' radius does not fit its adjacent legs")]
    CornerRadiusTooLarge { corner: String },
    #[error("corner radii overlap on path {path} of line '{line}'")]
    CornerRadiiOverlap { line: String, path: usize },
    #[error("schematic paths contain overlapping legs")]
    OverlappingLegs,
    #[error("schematic geometry is outside the renderer's numeric range")]
    CoordinateRange,
}
