//! Core data models for diagrammatic city metro topology and schematic maps.
//!
//! Topology coordinates may be Cartesian or geographic coordinates.
//! Schematic coordinates follow SVG's rightward x-axis and downward y-axis.
//!
//! YAML is the primary, human-editable manifest format. JSON uses the same
//! schemas and is available for interchange with web applications.

mod manifest_format;
mod schematic;
mod topology;

use std::collections::BTreeMap;

pub use schematic::{
    OctilinearAxis, SchematicBackgroundOptions, SchematicCommonStationFill,
    SchematicCommonStationOptions, SchematicCommonStationStroke, SchematicCorner,
    SchematicInterchangePort, SchematicInterchangeStationFill, SchematicInterchangeStationOptions,
    SchematicInterchangeStationStroke, SchematicLength, SchematicLine, SchematicLineOptions,
    SchematicManifest, SchematicOptions, SchematicPath, SchematicPoint, SchematicRenderError,
    SchematicRouteVisit, SchematicStation, SchematicStationColor, SchematicStationOptions,
    SchematicStationPort, SchematicStationSymbol, SchematicStrokeAlignment, SchematicValueError,
    render_schematic_svg, validate_schematic,
};
pub use topology::{
    DuplicateStationPositionGroup, DuplicateStationPositionGroups, EdgeEndpoint, IncidentEdge,
    LocatedOccurrence, MetroTopology, PreprocessedEdge, PreprocessedNode, PreprocessedPath,
    PreprocessedTopology, ReducedTraversal, RetentionReason, SchematicGenerationError,
    SourceSegmentSpan, StationNeighborOrder, TopologyBackgroundOptions, TopologyCartesianAxes,
    TopologyCommonStationFill, TopologyCommonStationOptions, TopologyCommonStationStroke,
    TopologyCoordinateOptions, TopologyGeographicAxes, TopologyInterchangeStationFill,
    TopologyInterchangeStationOptions, TopologyInterchangeStationStroke, TopologyLabelOptions,
    TopologyLength, TopologyLine, TopologyLineOptions, TopologyOptions, TopologyPath,
    TopologyPosition, TopologyPreprocessError, TopologyRenderError, TopologyScale, TopologyStation,
    TopologyStationColor, TopologyStationOptions, TopologyStrokeAlignment, TopologyValueError,
    UnsupportedIntersectionKind, VirtualContinuation, generate_schematic, preprocess_topology,
    render_topology_svg, validate_topology,
};

/// Names indexed by a locale such as `en` or `zh-CN`.
///
/// The first entry for a locale is its canonical name. Every following entry
/// is an alias in that locale.
pub type LocalizedNames = BTreeMap<String, Vec<String>>;
