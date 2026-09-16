mod error;
mod geometry;
mod prepared;

use std::collections::{HashMap, HashSet};

use super::{
    OctilinearAxis, SchematicInterchangePort, SchematicManifest, SchematicRouteVisit,
    SchematicStation, SchematicStationColor, SchematicStationPort, SchematicStationSymbol,
};

use geometry::{
    perpendicular, rotate_clockwise, rotate_counter_clockwise, validate_non_overlapping_legs,
    validate_path,
};

pub use error::SchematicRenderError;
pub(super) use geometry::{axis_vector, corner_tangents};
pub(super) use prepared::{
    Point, PreparedPath, PreparedPathPoint, PreparedPointKind, PreparedSchematic, PreparedShape,
    PreparedStroke, PreparedSymbol,
};

/// Validate all semantic and geometric invariants required by the schematic renderer.
pub fn validate_schematic(schematic: &SchematicManifest) -> Result<(), SchematicRenderError> {
    let prepared = prepare_schematic(schematic)?;
    super::render::validate_render_bounds(&prepared)
}

pub(super) fn prepare_schematic(
    schematic: &SchematicManifest,
) -> Result<PreparedSchematic<'_>, SchematicRenderError> {
    schematic.options.languages.validate()?;
    let stations = station_index(schematic)?;
    let corners = corner_index(schematic)?;
    let mut line_ids = HashSet::with_capacity(schematic.lines.len());
    let mut corner_owners = HashMap::<&str, &str>::new();
    let mut port_owners = HashMap::<PortKey<'_>, &str>::new();
    let mut port_references = HashMap::<PortKey<'_>, usize>::new();
    let mut station_lines = HashMap::<&str, &str>::new();
    let mut strokes = Vec::with_capacity(schematic.lines.len());

    for line in &schematic.lines {
        if line.id.trim().is_empty() {
            return Err(SchematicRenderError::EmptyLineId);
        }
        if !line_ids.insert(line.id.as_str()) {
            return Err(SchematicRenderError::DuplicateLine {
                line: line.id.clone(),
            });
        }
        schematic
            .options
            .languages
            .validate_names("line", &line.id, &line.names)?;

        let mut paths = Vec::with_capacity(line.paths.len());
        for (path_index, path) in line.paths.iter().enumerate() {
            let path_number = path_index + 1;
            let station_count = path
                .visits
                .iter()
                .filter(|visit| matches!(visit, SchematicRouteVisit::Station { .. }))
                .count();
            let minimum = if path.closed { 3 } else { 2 };
            if station_count < minimum {
                return Err(SchematicRenderError::PathTooShort {
                    line: line.id.clone(),
                    path: path_number,
                    minimum,
                });
            }
            if !path.closed
                && (!matches!(
                    path.visits.first(),
                    Some(SchematicRouteVisit::Station { .. })
                ) || !matches!(
                    path.visits.last(),
                    Some(SchematicRouteVisit::Station { .. })
                ))
            {
                return Err(SchematicRenderError::OpenPathEndpoint {
                    line: line.id.clone(),
                    path: path_number,
                });
            }

            let mut path_stations = HashSet::new();
            let mut path_corners = HashSet::new();
            let mut points = Vec::with_capacity(path.visits.len());
            for visit in &path.visits {
                match visit {
                    SchematicRouteVisit::Station { station_id, port } => {
                        if !path_stations.insert(station_id.as_str()) {
                            return Err(SchematicRenderError::DuplicateStationInPath {
                                line: line.id.clone(),
                                path: path_number,
                                station: station_id.clone(),
                            });
                        }
                        let station = stations.get(station_id.as_str()).ok_or_else(|| {
                            SchematicRenderError::UnknownStation {
                                line: line.id.clone(),
                                station: station_id.clone(),
                            }
                        })?;
                        let key = PortKey::new(station_id, *port);
                        if let Some(owner) = port_owners.insert(key, &line.id)
                            && owner != line.id
                        {
                            return Err(SchematicRenderError::PortSharedByLines {
                                port: key.display(),
                                first_line: owner.to_owned(),
                                second_line: line.id.clone(),
                            });
                        }
                        *port_references.entry(key).or_default() += 1;
                        station_lines.entry(station_id).or_insert(&line.id);
                        points.push(resolve_port(station, *port)?);
                    }
                    SchematicRouteVisit::Corner { corner_id } => {
                        if !path_corners.insert(corner_id.as_str()) {
                            return Err(SchematicRenderError::DuplicateCornerInPath {
                                line: line.id.clone(),
                                path: path_number,
                                corner: corner_id.clone(),
                            });
                        }
                        let corner = corners.get(corner_id.as_str()).ok_or_else(|| {
                            SchematicRenderError::UnknownCorner {
                                line: line.id.clone(),
                                corner: corner_id.clone(),
                            }
                        })?;
                        if let Some(owner) = corner_owners.insert(corner_id, &line.id)
                            && owner != line.id
                        {
                            return Err(SchematicRenderError::CornerSharedByLines {
                                corner: corner_id.clone(),
                                first_line: owner.to_owned(),
                                second_line: line.id.clone(),
                            });
                        }
                        points.push(PreparedPathPoint {
                            position: corner.position.into(),
                            kind: PreparedPointKind::Corner {
                                id: &corner.id,
                                radius: corner.radius.get(),
                            },
                        });
                    }
                }
            }
            validate_path(&line.id, path_number, &points, path.closed)?;
            paths.push(PreparedPath {
                points,
                closed: path.closed,
            });
        }
        strokes.push(PreparedStroke {
            id: &line.id,
            color: &line.color,
            paths,
        });
    }

    for corner in &schematic.corners {
        if !corner_owners.contains_key(corner.id.as_str()) {
            return Err(SchematicRenderError::UnreferencedCorner {
                corner: corner.id.clone(),
            });
        }
    }
    validate_perpendicular_references(schematic, &port_references)?;

    let line_width = schematic.options.lines.width.get();
    let mut symbols = Vec::with_capacity(schematic.stations.len());
    for station in &schematic.stations {
        let center = station.position.into();
        match station.symbol {
            SchematicStationSymbol::Circle {} => {
                let options = &schematic.options.stations.common;
                let fill = match &options.fill.color {
                    SchematicStationColor::Unified { value } => value.as_str(),
                    SchematicStationColor::FollowLine {} => station_lines
                        .get(station.id.as_str())
                        .and_then(|line_id| {
                            schematic.lines.iter().find(|line| line.id == **line_id)
                        })
                        .map_or("currentColor", |line| line.color.as_str()),
                };
                let stroke = match &options.stroke.color {
                    SchematicStationColor::Unified { value } => value.as_str(),
                    SchematicStationColor::FollowLine {} => station_lines
                        .get(station.id.as_str())
                        .and_then(|line_id| {
                            schematic.lines.iter().find(|line| line.id == **line_id)
                        })
                        .map_or("currentColor", |line| line.color.as_str()),
                };
                symbols.push(PreparedSymbol {
                    station,
                    center,
                    shape: PreparedShape::Circle {
                        diameter: options.fill.diameter.get().max(line_width),
                    },
                    fill,
                    stroke,
                    stroke_width: options.stroke.width.get(),
                    stroke_alignment: options.stroke.alignment,
                });
            }
            SchematicStationSymbol::Capsule {
                axis,
                anchor_count,
                anchor_interval,
            } => {
                if anchor_count > 1 && anchor_interval.get() < line_width {
                    return Err(SchematicRenderError::AnchorIntervalTooSmall {
                        station: station.id.clone(),
                    });
                }
                let options = &schematic.options.stations.interchange;
                let diameter = options.fill.width.get().max(line_width);
                let length = capsule_length(diameter, anchor_count, anchor_interval.get());
                if !length.is_finite() {
                    return Err(SchematicRenderError::CoordinateRange);
                }
                symbols.push(PreparedSymbol {
                    station,
                    center,
                    shape: PreparedShape::Capsule {
                        axis,
                        diameter,
                        length,
                    },
                    fill: &options.fill.color,
                    stroke: &options.stroke.color,
                    stroke_width: options.stroke.width.get(),
                    stroke_alignment: options.stroke.alignment,
                });
            }
        }
    }

    validate_non_overlapping_legs(&strokes)?;

    Ok(PreparedSchematic {
        line_width,
        symbols,
        strokes,
    })
}

fn capsule_length(diameter: f64, anchor_count: u8, anchor_interval: f64) -> f64 {
    if anchor_count <= 1 {
        diameter * 2.0
    } else {
        diameter + f64::from(anchor_count - 1) * anchor_interval
    }
}

fn station_index(
    schematic: &SchematicManifest,
) -> Result<HashMap<&str, &SchematicStation>, SchematicRenderError> {
    let mut stations = HashMap::with_capacity(schematic.stations.len());
    for station in &schematic.stations {
        if station.id.trim().is_empty() {
            return Err(SchematicRenderError::EmptyStationId);
        }
        if stations.insert(station.id.as_str(), station).is_some() {
            return Err(SchematicRenderError::DuplicateStation {
                station: station.id.clone(),
            });
        }
        schematic
            .options
            .languages
            .validate_names("station", &station.id, &station.names)?;
    }
    Ok(stations)
}

fn corner_index(
    schematic: &SchematicManifest,
) -> Result<HashMap<&str, &crate::SchematicCorner>, SchematicRenderError> {
    let mut corners = HashMap::with_capacity(schematic.corners.len());
    for corner in &schematic.corners {
        if corner.id.trim().is_empty() {
            return Err(SchematicRenderError::EmptyCornerId);
        }
        if corners.insert(corner.id.as_str(), corner).is_some() {
            return Err(SchematicRenderError::DuplicateCorner {
                corner: corner.id.clone(),
            });
        }
    }
    Ok(corners)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PortKey<'a> {
    station: &'a str,
    port: SchematicStationPort,
}

impl<'a> PortKey<'a> {
    fn new(station: &'a str, port: SchematicStationPort) -> Self {
        Self { station, port }
    }

    fn display(self) -> String {
        format!("{}:{:?}", self.station, self.port)
    }
}

fn resolve_port<'a>(
    station: &'a SchematicStation,
    port: SchematicStationPort,
) -> Result<PreparedPathPoint<'a>, SchematicRenderError> {
    let (position, permitted_axis) = match (station.symbol, port) {
        (SchematicStationSymbol::Circle {}, SchematicStationPort::SingleLine) => {
            (station.position.into(), None)
        }
        (
            SchematicStationSymbol::Capsule {
                axis,
                anchor_count,
                anchor_interval,
            },
            SchematicStationPort::Interchange(interchange),
        ) => {
            let (offset, axis) = match interchange {
                SchematicInterchangePort::MajorAxis {} => (0.0, axis),
                SchematicInterchangePort::RisingOblique {} => {
                    if anchor_count > 1 {
                        return Err(SchematicRenderError::ObliquePortWithMultipleAnchors {
                            station: station.id.clone(),
                            anchor_count,
                        });
                    }
                    (0.0, rotate_counter_clockwise(axis))
                }
                SchematicInterchangePort::FallingOblique {} => {
                    if anchor_count > 1 {
                        return Err(SchematicRenderError::ObliquePortWithMultipleAnchors {
                            station: station.id.clone(),
                            anchor_count,
                        });
                    }
                    (0.0, rotate_clockwise(axis))
                }
                SchematicInterchangePort::SinglePerpendicular {} => {
                    if anchor_count != 1 {
                        return Err(SchematicRenderError::IncompatiblePerpendicularPort {
                            station: station.id.clone(),
                            anchor_count,
                        });
                    }
                    (0.0, perpendicular(axis))
                }
                SchematicInterchangePort::PerpendicularAnchor { index } => {
                    if anchor_count <= 1 {
                        return Err(SchematicRenderError::IncompatiblePerpendicularPort {
                            station: station.id.clone(),
                            anchor_count,
                        });
                    }
                    if index >= anchor_count {
                        return Err(SchematicRenderError::PerpendicularAnchorOutOfRange {
                            station: station.id.clone(),
                            index,
                            anchor_count,
                        });
                    }
                    (
                        (f64::from(index) - (f64::from(anchor_count) - 1.0) / 2.0)
                            * anchor_interval.get(),
                        perpendicular(axis),
                    )
                }
            };
            let (unit_x, unit_y) = axis_vector(axis_of_capsule(station));
            let center = Point::from(station.position);
            (
                Point {
                    x: center.x + unit_x * offset,
                    y: center.y + unit_y * offset,
                },
                Some(axis),
            )
        }
        _ => {
            return Err(SchematicRenderError::IncompatiblePort {
                station: station.id.clone(),
            });
        }
    };
    if !position.x.is_finite() || !position.y.is_finite() {
        return Err(SchematicRenderError::CoordinateRange);
    }
    Ok(PreparedPathPoint {
        position,
        kind: PreparedPointKind::Anchor {
            station: &station.id,
            permitted_axis,
        },
    })
}

fn axis_of_capsule(station: &SchematicStation) -> OctilinearAxis {
    match station.symbol {
        SchematicStationSymbol::Capsule { axis, .. } => axis,
        SchematicStationSymbol::Circle {} => unreachable!("called only for a capsule"),
    }
}

fn validate_perpendicular_references(
    schematic: &SchematicManifest,
    references: &HashMap<PortKey<'_>, usize>,
) -> Result<(), SchematicRenderError> {
    for station in &schematic.stations {
        let SchematicStationSymbol::Capsule { anchor_count, .. } = station.symbol else {
            continue;
        };
        let valid = match anchor_count {
            0 => true,
            1 => {
                references.get(&PortKey::new(
                    &station.id,
                    SchematicStationPort::Interchange(
                        SchematicInterchangePort::SinglePerpendicular {},
                    ),
                )) == Some(&1)
            }
            count => (0..count).all(|index| {
                references.get(&PortKey::new(
                    &station.id,
                    SchematicStationPort::Interchange(
                        SchematicInterchangePort::PerpendicularAnchor { index },
                    ),
                )) == Some(&1)
            }),
        };
        if !valid {
            return Err(SchematicRenderError::IncompletePerpendicularPorts {
                station: station.id.clone(),
                anchor_count,
            });
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::capsule_length;

    #[test]
    fn zero_and_one_perpendicular_anchors_use_two_width_capsules() {
        assert_eq!(capsule_length(18.0, 0, 24.0), 36.0);
        assert_eq!(capsule_length(18.0, 1, 24.0), 36.0);
    }

    #[test]
    fn multiple_perpendicular_anchors_span_their_intervals() {
        assert_eq!(capsule_length(18.0, 3, 24.0), 66.0);
    }
}
