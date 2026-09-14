use super::{
    MetroTopology, TopologyCartesianAxes, TopologyCoordinateOptions, TopologyPosition,
    TopologyRenderError, TopologyStation,
};

const EARTH_MEAN_RADIUS_METRES: f64 = 6_371_008.8;
const PADDING: f64 = 48.0;
const LABEL_SPACE: f64 = 160.0;

#[derive(Debug, Clone, Copy)]
pub(super) struct Bounds {
    projection: Projection,
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

impl Bounds {
    pub(super) fn from_topology(topology: &MetroTopology) -> Option<Self> {
        let projection = Projection::from_topology(topology)?;
        let Some(first) = topology.stations.first() else {
            return Some(Self {
                projection,
                min_x: 0.0,
                max_x: 0.0,
                min_y: 0.0,
                max_y: 0.0,
            });
        };

        let (x, y) = projection.project(first.position)?;
        let mut bounds = Self {
            projection,
            min_x: x,
            max_x: x,
            min_y: y,
            max_y: y,
        };
        for station in &topology.stations[1..] {
            let (x, y) = projection.project(station.position)?;
            bounds.min_x = bounds.min_x.min(x);
            bounds.max_x = bounds.max_x.max(x);
            bounds.min_y = bounds.min_y.min(y);
            bounds.max_y = bounds.max_y.max(y);
        }
        Some(bounds)
    }

    pub(super) fn viewport(self) -> Option<(f64, f64)> {
        let scale = self.projection.scale();
        let width = (self.max_x - self.min_x) * scale + PADDING * 2.0 + LABEL_SPACE;
        let height = (self.max_y - self.min_y) * scale + PADDING * 2.0;

        (width.is_finite() && height.is_finite()).then_some((width, height))
    }

    pub(super) fn project(self, station: &TopologyStation) -> Option<(f64, f64)> {
        let (x, y) = self.projection.project(station.position)?;
        let scale = self.projection.scale();
        let x = (x - self.min_x) * scale + PADDING;
        let y = (y - self.min_y) * scale + PADDING;

        (x.is_finite() && y.is_finite()).then_some((x, y))
    }
}

#[derive(Debug, Clone, Copy)]
enum Projection {
    Cartesian {
        coordinates: TopologyCoordinateOptions,
        scale: f64,
    },
    Geographic {
        coordinates: TopologyCoordinateOptions,
        scale: f64,
        center_longitude_radians: f64,
        center_latitude_radians: f64,
    },
}

impl Projection {
    fn from_topology(topology: &MetroTopology) -> Option<Self> {
        let coordinates = topology.options.coordinates;
        let scale = topology.options.scale.get();
        match coordinates {
            TopologyCoordinateOptions::Cartesian { .. } => {
                Some(Self::Cartesian { coordinates, scale })
            }
            TopologyCoordinateOptions::Geographic { .. } => {
                let mut longitude_bounds = None::<(f64, f64)>;
                let mut latitude_bounds = None::<(f64, f64)>;
                for station in &topology.stations {
                    let (longitude, latitude) =
                        coordinates.longitude_latitude(station.position.x, station.position.y)?;
                    extend_bounds(&mut longitude_bounds, longitude);
                    extend_bounds(&mut latitude_bounds, latitude);
                }
                let (min_longitude, max_longitude) = longitude_bounds.unwrap_or((0.0, 0.0));
                let (min_latitude, max_latitude) = latitude_bounds.unwrap_or((0.0, 0.0));
                let center_longitude_radians = ((min_longitude + max_longitude) / 2.0).to_radians();
                let center_latitude_radians = ((min_latitude + max_latitude) / 2.0).to_radians();

                (center_longitude_radians.is_finite() && center_latitude_radians.is_finite())
                    .then_some(Self::Geographic {
                        coordinates,
                        scale,
                        center_longitude_radians,
                        center_latitude_radians,
                    })
            }
        }
    }

    fn scale(self) -> f64 {
        match self {
            Self::Cartesian { scale, .. } | Self::Geographic { scale, .. } => scale,
        }
    }

    fn project(self, position: TopologyPosition) -> Option<(f64, f64)> {
        let projected = match self {
            Self::Cartesian { coordinates, .. } => {
                coordinates.canonical_cartesian(position.x, position.y)?
            }
            Self::Geographic {
                coordinates,
                center_longitude_radians,
                center_latitude_radians,
                ..
            } => {
                let (longitude, latitude) =
                    coordinates.longitude_latitude(position.x, position.y)?;
                let longitude = longitude.to_radians();
                let latitude = latitude.to_radians();
                let x = EARTH_MEAN_RADIUS_METRES
                    * center_latitude_radians.cos()
                    * (longitude - center_longitude_radians);
                let y = EARTH_MEAN_RADIUS_METRES * (center_latitude_radians - latitude);
                (x, y)
            }
        };

        (projected.0.is_finite() && projected.1.is_finite()).then_some(projected)
    }
}

pub(super) fn canonicalize_coordinates(
    mut topology: MetroTopology,
) -> Result<MetroTopology, TopologyRenderError> {
    let projection =
        Projection::from_topology(&topology).ok_or(TopologyRenderError::CoordinateRange)?;
    for station in &mut topology.stations {
        let (x, y) = projection
            .project(station.position)
            .ok_or(TopologyRenderError::CoordinateRange)?;
        station.position = TopologyPosition { x, y };
    }
    topology.options.coordinates = TopologyCoordinateOptions::Cartesian {
        axes: TopologyCartesianAxes::RightDown,
    };
    Ok(topology)
}

fn extend_bounds(bounds: &mut Option<(f64, f64)>, value: f64) {
    match bounds {
        Some((minimum, maximum)) => {
            *minimum = minimum.min(value);
            *maximum = maximum.max(value);
        }
        None => *bounds = Some((value, value)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TopologyBackgroundOptions, TopologyCartesianAxes, TopologyCommonStationFill,
        TopologyCommonStationOptions, TopologyCommonStationStroke, TopologyGeographicAxes,
        TopologyInterchangeStationFill, TopologyInterchangeStationOptions,
        TopologyInterchangeStationStroke, TopologyLabelOptions, TopologyLength,
        TopologyLineOptions, TopologyOptions, TopologyScale, TopologyStationColor,
        TopologyStationOptions, TopologyStrokeAlignment,
    };

    fn topology(coordinates: TopologyCoordinateOptions, positions: &[(f64, f64)]) -> MetroTopology {
        MetroTopology {
            options: TopologyOptions {
                background: TopologyBackgroundOptions::Transparent,
                coordinates,
                labels: TopologyLabelOptions { hidden: true },
                lines: TopologyLineOptions {
                    width: TopologyLength::new(8.0).unwrap(),
                },
                scale: TopologyScale::default(),
                stations: TopologyStationOptions {
                    common: TopologyCommonStationOptions {
                        fill: TopologyCommonStationFill {
                            diameter: TopologyLength::new(18.0).unwrap(),
                            color: TopologyStationColor::FollowLine {},
                        },
                        stroke: TopologyCommonStationStroke {
                            width: TopologyLength::new(2.0).unwrap(),
                            alignment: TopologyStrokeAlignment::Center,
                            color: TopologyStationColor::FollowLine {},
                        },
                    },
                    interchange: TopologyInterchangeStationOptions {
                        fill: TopologyInterchangeStationFill {
                            width: TopologyLength::new(18.0).unwrap(),
                            color: "#fff".into(),
                        },
                        stroke: TopologyInterchangeStationStroke {
                            width: TopologyLength::new(2.0).unwrap(),
                            alignment: TopologyStrokeAlignment::Center,
                            color: "#000".into(),
                        },
                    },
                },
            },
            stations: positions
                .iter()
                .enumerate()
                .map(|(index, &(x, y))| TopologyStation {
                    id: index.to_string(),
                    names: Default::default(),
                    position: TopologyPosition { x, y },
                })
                .collect(),
            lines: vec![],
        }
    }

    #[test]
    fn normalizes_every_cartesian_orientation() {
        let cases = [
            (TopologyCartesianAxes::RightDown, (2.0, 3.0)),
            (TopologyCartesianAxes::RightUp, (2.0, -3.0)),
            (TopologyCartesianAxes::LeftDown, (-2.0, 3.0)),
            (TopologyCartesianAxes::LeftUp, (-2.0, -3.0)),
            (TopologyCartesianAxes::DownRight, (3.0, 2.0)),
            (TopologyCartesianAxes::DownLeft, (3.0, -2.0)),
            (TopologyCartesianAxes::UpRight, (-3.0, 2.0)),
            (TopologyCartesianAxes::UpLeft, (-3.0, -2.0)),
        ];

        for (axes, input) in cases {
            let topology = topology(TopologyCoordinateOptions::Cartesian { axes }, &[input]);
            let projection = Projection::from_topology(&topology).unwrap();
            assert_eq!(
                projection.project(TopologyPosition {
                    x: input.0,
                    y: input.1
                }),
                Some((2.0, 3.0))
            );
        }
    }

    #[test]
    fn uses_unit_scale_for_cartesian_coordinates_by_default() {
        let topology = topology(
            TopologyCoordinateOptions::Cartesian {
                axes: TopologyCartesianAxes::RightDown,
            },
            &[(0.0, 0.0), (2.0, 3.0)],
        );
        let bounds = Bounds::from_topology(&topology).unwrap();
        let first = bounds.project(&topology.stations[0]).unwrap();
        let second = bounds.project(&topology.stations[1]).unwrap();

        assert_eq!((second.0 - first.0, second.1 - first.1), (2.0, 3.0));
    }

    #[test]
    fn normalizes_every_geographic_orientation() {
        let cases = [
            (TopologyGeographicAxes::EastNorth, (-118.0, 34.0)),
            (TopologyGeographicAxes::EastSouth, (-118.0, -34.0)),
            (TopologyGeographicAxes::WestNorth, (118.0, 34.0)),
            (TopologyGeographicAxes::WestSouth, (118.0, -34.0)),
            (TopologyGeographicAxes::NorthEast, (34.0, -118.0)),
            (TopologyGeographicAxes::NorthWest, (34.0, 118.0)),
            (TopologyGeographicAxes::SouthEast, (-34.0, -118.0)),
            (TopologyGeographicAxes::SouthWest, (-34.0, 118.0)),
        ];

        for (axes, input) in cases {
            let coordinates = TopologyCoordinateOptions::Geographic { axes };
            assert_eq!(
                coordinates.longitude_latitude(input.0, input.1),
                Some((-118.0, 34.0))
            );
        }
    }

    #[test]
    fn projects_geographic_positions_in_metres_with_downward_y() {
        let topology = topology(
            TopologyCoordinateOptions::Geographic {
                axes: TopologyGeographicAxes::EastNorth,
            },
            &[(10.0, 59.0), (12.0, 61.0)],
        );
        let bounds = Bounds::from_topology(&topology).unwrap();
        let west_south = bounds.project(&topology.stations[0]).unwrap();
        let east_north = bounds.project(&topology.stations[1]).unwrap();
        let expected_width =
            EARTH_MEAN_RADIUS_METRES * 60_f64.to_radians().cos() * 2_f64.to_radians();
        let expected_height = EARTH_MEAN_RADIUS_METRES * 2_f64.to_radians();

        assert!((east_north.0 - west_south.0 - expected_width).abs() < 1e-6);
        assert!((west_south.1 - east_north.1 - expected_height).abs() < 1e-6);
        assert_eq!(east_north.1, PADDING);
    }

    #[test]
    fn applies_configured_scale_to_cartesian_and_geographic_coordinates() {
        let mut cartesian = topology(
            TopologyCoordinateOptions::Cartesian {
                axes: TopologyCartesianAxes::RightDown,
            },
            &[(0.0, 0.0), (2.0, 3.0)],
        );
        cartesian.options.scale = TopologyScale::new(3.0).unwrap();
        let bounds = Bounds::from_topology(&cartesian).unwrap();
        let first = bounds.project(&cartesian.stations[0]).unwrap();
        let second = bounds.project(&cartesian.stations[1]).unwrap();

        assert_eq!((second.0 - first.0, second.1 - first.1), (6.0, 9.0));

        let mut geographic = topology(
            TopologyCoordinateOptions::Geographic {
                axes: TopologyGeographicAxes::EastNorth,
            },
            &[(10.0, 59.0), (12.0, 61.0)],
        );
        geographic.options.scale = TopologyScale::new(3.0).unwrap();
        let bounds = Bounds::from_topology(&geographic).unwrap();
        let west_south = bounds.project(&geographic.stations[0]).unwrap();
        let east_north = bounds.project(&geographic.stations[1]).unwrap();
        let expected_width =
            3.0 * EARTH_MEAN_RADIUS_METRES * 60_f64.to_radians().cos() * 2_f64.to_radians();
        let expected_height = 3.0 * EARTH_MEAN_RADIUS_METRES * 2_f64.to_radians();

        assert!((east_north.0 - west_south.0 - expected_width).abs() < 1e-6);
        assert!((west_south.1 - east_north.1 - expected_height).abs() < 1e-6);
    }

    #[test]
    fn uses_direct_longitude_bounds_across_the_antimeridian() {
        let topology = topology(
            TopologyCoordinateOptions::Geographic {
                axes: TopologyGeographicAxes::EastNorth,
            },
            &[(-179.0, 0.0), (179.0, 0.0)],
        );
        let (width, _) = Bounds::from_topology(&topology)
            .unwrap()
            .viewport()
            .unwrap();
        let expected =
            EARTH_MEAN_RADIUS_METRES * 358_f64.to_radians() + PADDING * 2.0 + LABEL_SPACE;

        assert!((width - expected).abs() < 1e-6);
    }
}
