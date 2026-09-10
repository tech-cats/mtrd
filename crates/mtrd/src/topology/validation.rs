use std::collections::{HashMap, HashSet};
use std::fmt;

use thiserror::Error;

use super::{MetroTopology, TopologyPosition, TopologyStation, layout::Bounds};

/// A group of stations occupying one position in a topology manifest.
#[derive(Debug, PartialEq)]
pub struct DuplicateStationPositionGroup {
    pub position: TopologyPosition,
    pub station_ids: Vec<String>,
}

/// Every group of stations occupying identical positions in a topology manifest.
#[derive(Debug, PartialEq)]
pub struct DuplicateStationPositionGroups {
    pub groups: Vec<DuplicateStationPositionGroup>,
}

impl fmt::Display for DuplicateStationPositionGroups {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("stations share identical coordinates:")?;
        for group in &self.groups {
            write!(
                formatter,
                "\n  [{}, {}]: ",
                group.position.x, group.position.y
            )?;
            for (index, station_id) in group.station_ids.iter().enumerate() {
                if index > 0 {
                    formatter.write_str(", ")?;
                }
                write!(formatter, "'{station_id}'")?;
            }
        }
        Ok(())
    }
}

/// An error encountered while rendering a metro topology.
#[derive(Debug, Error, PartialEq)]
pub enum TopologyRenderError {
    #[error("station id must not be empty")]
    EmptyStationId,

    #[error("station '{station}' has a non-finite position")]
    NonFinitePosition { station: String },

    #[error(
        "station '{station}' has geographic position outside longitude [-180, 180] and latitude [-90, 90]: [{longitude}, {latitude}]"
    )]
    GeographicPositionOutOfRange {
        station: String,
        longitude: f64,
        latitude: f64,
    },

    #[error("station id '{station}' is defined more than once")]
    DuplicateStation { station: String },

    #[error("{0}")]
    DuplicateStationPositions(DuplicateStationPositionGroups),

    #[error("line id must not be empty")]
    EmptyLineId,

    #[error("line id '{line}' is defined more than once")]
    DuplicateLine { line: String },

    #[error("line '{line}' refers to unknown station '{station}'")]
    UnknownStation { line: String, station: String },

    #[error("path {path} of line '{line}' must contain at least {minimum} stations")]
    PathTooShort {
        line: String,
        path: usize,
        minimum: usize,
    },

    #[error("path {path} of line '{line}' contains station '{station}' more than once")]
    DuplicateStationInPath {
        line: String,
        path: usize,
        station: String,
    },

    #[error("station coordinates are too large to render")]
    CoordinateRange,
}

/// Validate all invariants required to render a topology graph.
pub fn validate_topology(topology: &MetroTopology) -> Result<(), TopologyRenderError> {
    validate_topology_structure(topology)?;

    let bounds = Bounds::from_topology(topology).ok_or(TopologyRenderError::CoordinateRange)?;
    if bounds.viewport().is_none()
        || topology
            .stations
            .iter()
            .any(|station| bounds.project(station).is_none())
    {
        return Err(TopologyRenderError::CoordinateRange);
    }

    Ok(())
}

pub(super) fn validate_topology_structure(
    topology: &MetroTopology,
) -> Result<(), TopologyRenderError> {
    let stations = station_index(topology)?;
    let mut line_ids = HashSet::with_capacity(topology.lines.len());

    for line in &topology.lines {
        if line.id.trim().is_empty() {
            return Err(TopologyRenderError::EmptyLineId);
        }
        if !line_ids.insert(line.id.as_str()) {
            return Err(TopologyRenderError::DuplicateLine {
                line: line.id.clone(),
            });
        }

        for (path_index, path) in line.paths.iter().enumerate() {
            let minimum = if path.closed { 3 } else { 2 };
            if path.stations.len() < minimum {
                return Err(TopologyRenderError::PathTooShort {
                    line: line.id.clone(),
                    path: path_index + 1,
                    minimum,
                });
            }

            let mut path_stations = HashSet::with_capacity(path.stations.len());
            for station in &path.stations {
                if !stations.contains_key(station.as_str()) {
                    return Err(TopologyRenderError::UnknownStation {
                        line: line.id.clone(),
                        station: station.clone(),
                    });
                }
                if !path_stations.insert(station.as_str()) {
                    return Err(TopologyRenderError::DuplicateStationInPath {
                        line: line.id.clone(),
                        path: path_index + 1,
                        station: station.clone(),
                    });
                }
            }
        }
    }

    Ok(())
}

pub(super) fn station_index(
    topology: &MetroTopology,
) -> Result<HashMap<&str, &TopologyStation>, TopologyRenderError> {
    let mut stations = HashMap::with_capacity(topology.stations.len());
    let mut positions = HashMap::with_capacity(topology.stations.len());
    for (station_index, station) in topology.stations.iter().enumerate() {
        if station.id.trim().is_empty() {
            return Err(TopologyRenderError::EmptyStationId);
        }
        if !station.position.x.is_finite() || !station.position.y.is_finite() {
            return Err(TopologyRenderError::NonFinitePosition {
                station: station.id.clone(),
            });
        }
        if let Some((longitude, latitude)) = topology
            .options
            .coordinates
            .longitude_latitude(station.position.x, station.position.y)
            && (!(-180.0..=180.0).contains(&longitude) || !(-90.0..=90.0).contains(&latitude))
        {
            return Err(TopologyRenderError::GeographicPositionOutOfRange {
                station: station.id.clone(),
                longitude,
                latitude,
            });
        }
        if stations.insert(station.id.as_str(), station).is_some() {
            return Err(TopologyRenderError::DuplicateStation {
                station: station.id.clone(),
            });
        }
        let group = positions
            .entry(position_key(station.position))
            .or_insert_with(|| IndexedPositionGroup {
                position: station.position,
                station_ids: Vec::new(),
                first_station_index: station_index,
            });
        group.station_ids.push(station.id.as_str());
    }

    let mut duplicate_groups = positions
        .into_values()
        .filter(|group| group.station_ids.len() > 1)
        .collect::<Vec<_>>();
    duplicate_groups.sort_unstable_by_key(|group| group.first_station_index);

    if !duplicate_groups.is_empty() {
        return Err(TopologyRenderError::DuplicateStationPositions(
            DuplicateStationPositionGroups {
                groups: duplicate_groups
                    .into_iter()
                    .map(|group| DuplicateStationPositionGroup {
                        position: group.position,
                        station_ids: group.station_ids.into_iter().map(str::to_owned).collect(),
                    })
                    .collect(),
            },
        ));
    }

    Ok(stations)
}

struct IndexedPositionGroup<'a> {
    position: TopologyPosition,
    station_ids: Vec<&'a str>,
    first_station_index: usize,
}

fn position_key(position: TopologyPosition) -> (u64, u64) {
    fn coordinate_key(coordinate: f64) -> u64 {
        if coordinate == 0.0 {
            0.0_f64.to_bits()
        } else {
            coordinate.to_bits()
        }
    }

    (coordinate_key(position.x), coordinate_key(position.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TopologyCoordinateOptions, TopologyGeographicAxes, TopologyLine, TopologyOptions,
        TopologyPath, TopologyPosition,
    };

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

    fn topology() -> MetroTopology {
        MetroTopology {
            options: options(),
            stations: vec![
                TopologyStation {
                    id: "south&west".into(),
                    names: [("en".into(), vec!["South <West>".into()])].into(),
                    position: TopologyPosition { x: -1.0, y: 1.0 },
                },
                TopologyStation {
                    id: "north".into(),
                    names: [("en".into(), vec!["North".into()])].into(),
                    position: TopologyPosition { x: 1.0, y: 3.0 },
                },
            ],
            lines: vec![TopologyLine {
                id: "red\"line".into(),
                names: Default::default(),
                color: "#f00".into(),
                paths: vec![TopologyPath {
                    stations: vec!["south&west".into(), "north".into()],
                    closed: false,
                }],
            }],
        }
    }

    #[test]
    fn validates_ids_station_references_and_coordinates() {
        let mut invalid = topology();
        invalid.stations[0].id.clear();
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::EmptyStationId)
        );

        let mut invalid = topology();
        invalid.stations[1].id = invalid.stations[0].id.clone();
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::DuplicateStation {
                station: "south&west".into()
            })
        );

        let mut invalid = topology();
        invalid.stations[0].position.x = f64::NAN;
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::NonFinitePosition {
                station: "south&west".into()
            })
        );

        let mut invalid = topology();
        invalid.stations[0].position.x = -f64::MAX;
        invalid.stations[1].position.x = f64::MAX;
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::CoordinateRange)
        );

        let mut invalid = topology();
        invalid.lines[0].id.clear();
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::EmptyLineId)
        );

        let mut invalid = topology();
        invalid.lines.push(invalid.lines[0].clone());
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::DuplicateLine {
                line: "red\"line".into()
            })
        );

        let mut invalid = topology();
        invalid.lines[0].paths[0].stations[1] = "missing".into();
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::UnknownStation {
                line: "red\"line".into(),
                station: "missing".into()
            })
        );
    }

    #[test]
    fn validates_path_lengths_and_repeated_stations() {
        let mut invalid = topology();
        invalid.lines[0].paths[0].stations.pop();
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::PathTooShort {
                line: "red\"line".into(),
                path: 1,
                minimum: 2,
            })
        );

        let mut invalid = topology();
        invalid.lines[0].paths[0].closed = true;
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::PathTooShort {
                line: "red\"line".into(),
                path: 1,
                minimum: 3,
            })
        );

        let mut invalid = topology();
        invalid.lines[0].paths[0].stations.push("south&west".into());
        assert_eq!(
            validate_topology(&invalid),
            Err(TopologyRenderError::DuplicateStationInPath {
                line: "red\"line".into(),
                path: 1,
                station: "south&west".into(),
            })
        );
    }

    #[test]
    fn validates_decoded_geographic_ranges() {
        let mut valid = topology();
        valid.options.coordinates = TopologyCoordinateOptions::Geographic {
            axes: TopologyGeographicAxes::NorthWest,
        };
        valid.stations[0].position = TopologyPosition { x: 34.0, y: 118.0 };
        valid.stations[1].position = TopologyPosition { x: 34.1, y: 117.9 };
        assert_eq!(validate_topology(&valid), Ok(()));

        valid.stations[1].position.y = 181.0;
        assert_eq!(
            validate_topology(&valid),
            Err(TopologyRenderError::GeographicPositionOutOfRange {
                station: "north".into(),
                longitude: -181.0,
                latitude: 34.1,
            })
        );
    }

    #[test]
    fn reports_all_groups_of_station_ids_at_the_same_position() {
        let mut invalid = topology();
        invalid.stations[0].position = TopologyPosition { x: 0.0, y: -0.0 };
        invalid.stations[1].position = TopologyPosition { x: -0.0, y: 0.0 };

        let mut east = invalid.stations[0].clone();
        east.id = "east".into();
        east.position = TopologyPosition { x: 2.0, y: 3.0 };
        let mut west = east.clone();
        west.id = "west".into();
        let mut central = east.clone();
        central.id = "central".into();
        invalid.stations.extend([east, west, central]);

        let error = validate_topology(&invalid).unwrap_err();

        assert_eq!(
            error,
            TopologyRenderError::DuplicateStationPositions(DuplicateStationPositionGroups {
                groups: vec![
                    DuplicateStationPositionGroup {
                        position: TopologyPosition { x: 0.0, y: -0.0 },
                        station_ids: vec!["south&west".into(), "north".into()],
                    },
                    DuplicateStationPositionGroup {
                        position: TopologyPosition { x: 2.0, y: 3.0 },
                        station_ids: vec!["east".into(), "west".into(), "central".into()],
                    },
                ],
            })
        );
        assert_eq!(
            error.to_string(),
            "stations share identical coordinates:\n  [0, -0]: 'south&west', 'north'\n  [2, 3]: 'east', 'west', 'central'"
        );
    }
}
