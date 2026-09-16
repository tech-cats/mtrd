use std::collections::HashMap;
use std::fmt::Write;

use super::{
    MetroTopology, TopologyPath, TopologyStation, TopologyStationColor, TopologyStrokeAlignment,
    layout::Bounds,
    validation::{TopologyRenderError, station_index, validate_topology},
};

const LANE_GAP: f64 = 3.0;
const TAPER_LENGTH: f64 = 24.0;

/// Render a topology as an SVG topology graph.
///
/// Manifest positions are converted from their configured coordinate system
/// to rightward and downward axes before rendering. Each line path is drawn in
/// its configured color; closed paths are joined back to their first station.
pub fn render_topology_svg(topology: &MetroTopology) -> Result<String, TopologyRenderError> {
    let topology = topology.clone().canonicalize_coordinates()?;
    let topology = &topology;
    validate_topology(topology)?;
    let stations = station_index(topology)?;
    let station_lines = station_lines(topology);
    let segment_lanes = segment_lanes(topology);
    let scale = topology.options.scale.get();
    let line_width = topology.options.lines.width.get() * scale;
    let lane_spacing = lane_spacing(line_width);
    if !lane_spacing.is_finite() {
        return Err(TopologyRenderError::CoordinateRange);
    }
    let bounds = Bounds::from_topology(topology).ok_or(TopologyRenderError::CoordinateRange)?;
    let (width, height) = bounds
        .viewport()
        .ok_or(TopologyRenderError::CoordinateRange)?;

    let mut svg = String::new();
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="{}" height="{}" role="img">"#,
        number(width),
        number(height),
        number(width),
        number(height),
    )
    .unwrap();
    writeln!(svg, "  <title>Metro topology map</title>").unwrap();
    if let crate::TopologyBackgroundOptions::Color { color } = &topology.options.background {
        writeln!(
            svg,
            "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\" />",
            number(width),
            number(height),
            xml_escape(color),
        )
        .unwrap();
    }
    writeln!(
        svg,
        "  <g fill=\"none\" stroke-linecap=\"round\" stroke-linejoin=\"round\">"
    )
    .unwrap();

    for (line_index, line) in topology.lines.iter().enumerate() {
        for path in &line.paths {
            if let [station_id] = path.stations.as_slice() {
                let station = stations.get(station_id.as_str()).ok_or_else(|| {
                    TopologyRenderError::UnknownStation {
                        line: line.id.clone(),
                        station: station_id.clone(),
                    }
                })?;
                let (x, y) = project(bounds, station)?;
                writeln!(
                    svg,
                    "    <path data-line-id=\"{}\" d=\"M{} {}\" stroke=\"{}\" stroke-width=\"{}\" />",
                    xml_escape(&line.id),
                    number(x),
                    number(y),
                    xml_escape(&line.color),
                    number(line_width),
                )
                .unwrap();
                continue;
            }

            for (start_id, end_id) in path_segments(path) {
                let start =
                    stations
                        .get(start_id)
                        .ok_or_else(|| TopologyRenderError::UnknownStation {
                            line: line.id.clone(),
                            station: start_id.to_owned(),
                        })?;
                let end =
                    stations
                        .get(end_id)
                        .ok_or_else(|| TopologyRenderError::UnknownStation {
                            line: line.id.clone(),
                            station: end_id.to_owned(),
                        })?;
                let key = SegmentKey::new(start_id, end_id);
                let lanes = &segment_lanes[&key];
                let lane_index = lanes
                    .iter()
                    .position(|candidate| *candidate == line_index)
                    .expect("every rendered segment was indexed");
                let offset = (lane_index as f64 - (lanes.len() as f64 - 1.0) / 2.0) * lane_spacing;
                let data = segment_path(bounds, start, end, key.is_forward(start_id), offset)?;

                writeln!(
                    svg,
                    "    <path data-line-id=\"{}\" d=\"{}\" stroke=\"{}\" stroke-width=\"{}\" />",
                    xml_escape(&line.id),
                    data,
                    xml_escape(&line.color),
                    number(line_width),
                )
                .unwrap();
            }
        }
    }
    writeln!(svg, "  </g>").unwrap();
    writeln!(svg, "  <g font-family=\"sans-serif\" font-size=\"14\">").unwrap();

    for station in &topology.stations {
        let (x, y) = project(bounds, station)?;
        let line_indexes = station_lines
            .get(station.id.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        let (diameter, fill, stroke, stroke_width, stroke_alignment) = if line_indexes.len() > 1 {
            let options = &topology.options.stations.interchange;
            (
                (options.fill.width.get() * scale).max(line_width),
                options.fill.color.as_str(),
                options.stroke.color.as_str(),
                options.stroke.width.get() * scale,
                options.stroke.alignment,
            )
        } else {
            let options = &topology.options.stations.common;
            (
                (options.fill.diameter.get() * scale).max(line_width),
                station_color(topology, line_indexes, &options.fill.color),
                station_color(topology, line_indexes, &options.stroke.color),
                options.stroke.width.get() * scale,
                options.stroke.alignment,
            )
        };
        let outline_diameter = aligned_size(diameter, stroke_width, stroke_alignment);
        if !outline_diameter.is_finite() {
            return Err(TopologyRenderError::CoordinateRange);
        }
        writeln!(
            svg,
            "    <g data-station-id=\"{}\"><circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{}\" /><circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{}\" />",
            xml_escape(&station.id),
            number(x),
            number(y),
            number(diameter / 2.0),
            xml_escape(fill),
            number(x),
            number(y),
            number(outline_diameter / 2.0),
            xml_escape(stroke),
            number(stroke_width),
        )
        .unwrap();
        if !topology.options.labels.hidden {
            let label_x = x + diameter.max(outline_diameter) / 2.0 + 6.0;
            if !label_x.is_finite() {
                return Err(TopologyRenderError::CoordinateRange);
            }
            let primary = station_label(station, &topology.options.languages.primary);
            if let Some(secondary) = &topology.options.languages.secondary {
                write!(
                    svg,
                    "<text x=\"{}\" y=\"{}\" dominant-baseline=\"middle\"><tspan x=\"{}\" dy=\"-0.6em\">{}</tspan><tspan x=\"{}\" dy=\"1.2em\">{}</tspan></text>",
                    number(label_x),
                    number(y),
                    number(label_x),
                    xml_escape(primary),
                    number(label_x),
                    xml_escape(station_label(station, secondary)),
                )
                .unwrap();
            } else {
                write!(
                    svg,
                    "<text x=\"{}\" y=\"{}\" dominant-baseline=\"middle\">{}</text>",
                    number(label_x),
                    number(y),
                    xml_escape(primary),
                )
                .unwrap();
            }
        }
        writeln!(svg, "</g>").unwrap();
    }

    writeln!(svg, "  </g>").unwrap();
    writeln!(svg, "</svg>").unwrap();
    Ok(svg)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SegmentKey<'a> {
    first: &'a str,
    second: &'a str,
}

impl<'a> SegmentKey<'a> {
    fn new(first: &'a str, second: &'a str) -> Self {
        if first <= second {
            Self { first, second }
        } else {
            Self {
                first: second,
                second: first,
            }
        }
    }

    fn is_forward(self, start: &str) -> bool {
        self.first == start
    }
}

fn segment_lanes(topology: &MetroTopology) -> HashMap<SegmentKey<'_>, Vec<usize>> {
    let mut segments = HashMap::<_, Vec<_>>::new();
    for (line_index, line) in topology.lines.iter().enumerate() {
        for path in &line.paths {
            for (start, end) in path_segments(path) {
                let lanes = segments.entry(SegmentKey::new(start, end)).or_default();
                if !lanes.contains(&line_index) {
                    lanes.push(line_index);
                }
            }
        }
    }
    segments
}

fn station_lines(topology: &MetroTopology) -> HashMap<&str, Vec<usize>> {
    let mut stations = HashMap::<_, Vec<_>>::new();
    for (line_index, line) in topology.lines.iter().enumerate() {
        for path in &line.paths {
            for station in &path.stations {
                let lines = stations.entry(station.as_str()).or_default();
                if !lines.contains(&line_index) {
                    lines.push(line_index);
                }
            }
        }
    }
    stations
}

fn path_segments(path: &TopologyPath) -> impl Iterator<Item = (&str, &str)> {
    let adjacent = path
        .stations
        .windows(2)
        .map(|stations| (stations[0].as_str(), stations[1].as_str()));
    let closing = (path.closed && path.stations.len() > 1).then(|| {
        (
            path.stations.last().unwrap().as_str(),
            path.stations.first().unwrap().as_str(),
        )
    });
    adjacent.chain(closing)
}

fn lane_spacing(line_width: f64) -> f64 {
    line_width + LANE_GAP
}

fn station_color<'a>(
    topology: &'a MetroTopology,
    line_indexes: &[usize],
    color: &'a TopologyStationColor,
) -> &'a str {
    match color {
        TopologyStationColor::Unified { value } => value,
        TopologyStationColor::FollowLine {} => {
            line_indexes.first().map_or("currentColor", |index| {
                topology.lines[*index].color.as_str()
            })
        }
    }
}

fn aligned_size(size: f64, stroke_width: f64, alignment: TopologyStrokeAlignment) -> f64 {
    match alignment {
        TopologyStrokeAlignment::Inside => (size - stroke_width).max(0.0),
        TopologyStrokeAlignment::Center => size,
        TopologyStrokeAlignment::Outside => size + stroke_width,
    }
}

fn segment_path(
    bounds: Bounds,
    start: &TopologyStation,
    end: &TopologyStation,
    canonical_direction: bool,
    offset: f64,
) -> Result<String, TopologyRenderError> {
    let (start_x, start_y) = project(bounds, start)?;
    let (end_x, end_y) = project(bounds, end)?;
    let dx = end_x - start_x;
    let dy = end_y - start_y;
    let length = dx.hypot(dy);

    if offset == 0.0 || length == 0.0 {
        return Ok(format!(
            "M{} {} L{} {}",
            number(start_x),
            number(start_y),
            number(end_x),
            number(end_y)
        ));
    }

    let direction_x = dx / length;
    let direction_y = dy / length;
    let canonical_sign = if canonical_direction { 1.0 } else { -1.0 };
    let normal_x = -direction_y * canonical_sign;
    let normal_y = direction_x * canonical_sign;
    let taper = TAPER_LENGTH.min(length / 4.0);
    let first_x = start_x + direction_x * taper + normal_x * offset;
    let first_y = start_y + direction_y * taper + normal_y * offset;
    let second_x = end_x - direction_x * taper + normal_x * offset;
    let second_y = end_y - direction_y * taper + normal_y * offset;

    Ok(format!(
        "M{} {} L{} {} L{} {} L{} {}",
        number(start_x),
        number(start_y),
        number(first_x),
        number(first_y),
        number(second_x),
        number(second_y),
        number(end_x),
        number(end_y)
    ))
}

fn project(bounds: Bounds, station: &TopologyStation) -> Result<(f64, f64), TopologyRenderError> {
    bounds
        .project(station)
        .ok_or(TopologyRenderError::CoordinateRange)
}

fn station_label<'a>(station: &'a TopologyStation, primary: &str) -> &'a str {
    station
        .names
        .get(primary)
        .and_then(|names| names.first())
        .map(String::as_str)
        .unwrap_or(&station.id)
}

fn number(value: f64) -> String {
    let value = if value == 0.0 { 0.0 } else { value };
    let formatted = format!("{value:.3}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TopologyBackgroundOptions, TopologyCartesianAxes, TopologyCommonStationFill,
        TopologyCommonStationOptions, TopologyCommonStationStroke, TopologyCoordinateOptions,
        TopologyGeographicAxes, TopologyInterchangeStationFill, TopologyInterchangeStationOptions,
        TopologyInterchangeStationStroke, TopologyLabelOptions, TopologyLength, TopologyLine,
        TopologyLineOptions, TopologyOptions, TopologyPath, TopologyPosition, TopologyScale,
        TopologyStationOptions,
    };

    fn options() -> TopologyOptions {
        TopologyOptions {
            background: TopologyBackgroundOptions::Color {
                color: "#abcdef".into(),
            },
            coordinates: Default::default(),
            labels: TopologyLabelOptions { hidden: false },
            languages: crate::Languages {
                set: ["en".to_owned()].into(),
                primary: "en".to_owned(),
                secondary: None,
            },
            lines: TopologyLineOptions {
                width: TopologyLength::new(8.0).unwrap(),
            },
            scale: Default::default(),
            stations: TopologyStationOptions {
                common: TopologyCommonStationOptions {
                    fill: TopologyCommonStationFill {
                        diameter: TopologyLength::new(18.0).unwrap(),
                        color: TopologyStationColor::Unified {
                            value: "#fedcba".into(),
                        },
                    },
                    stroke: TopologyCommonStationStroke {
                        width: TopologyLength::new(2.0).unwrap(),
                        alignment: TopologyStrokeAlignment::Center,
                        color: TopologyStationColor::FollowLine {},
                    },
                },
                interchange: TopologyInterchangeStationOptions {
                    fill: TopologyInterchangeStationFill {
                        width: TopologyLength::new(20.0).unwrap(),
                        color: "#eeeeee".into(),
                    },
                    stroke: TopologyInterchangeStationStroke {
                        width: TopologyLength::new(4.0).unwrap(),
                        alignment: TopologyStrokeAlignment::Outside,
                        color: "#111111".into(),
                    },
                },
            },
        }
    }

    fn topology() -> MetroTopology {
        MetroTopology {
            options: options(),
            stations: vec![
                TopologyStation {
                    id: "south&west".into(),
                    names: [("en".into(), vec!["South <West>".into()])].into(),
                    position: TopologyPosition { x: -80.0, y: 80.0 },
                },
                TopologyStation {
                    id: "north".into(),
                    names: [("en".into(), vec!["North".into()])].into(),
                    position: TopologyPosition { x: 80.0, y: 240.0 },
                },
            ],
            lines: vec![TopologyLine {
                id: "red\"line".into(),
                names: [("en".into(), vec!["Test".into()])].into(),
                color: "#f00".into(),
                paths: vec![TopologyPath {
                    stations: vec!["south&west".into(), "north".into()],
                    closed: false,
                }],
            }],
        }
    }

    fn horizontal_shared_topology(line_count: usize) -> MetroTopology {
        let stations = vec![
            TopologyStation {
                id: "a".into(),
                names: [("en".into(), vec!["Test".into()])].into(),
                position: TopologyPosition { x: 0.0, y: 0.0 },
            },
            TopologyStation {
                id: "b".into(),
                names: [("en".into(), vec!["Test".into()])].into(),
                position: TopologyPosition { x: 160.0, y: 0.0 },
            },
        ];
        let lines = (0..line_count)
            .map(|index| TopologyLine {
                id: format!("line-{index}"),
                names: [("en".into(), vec!["Test".into()])].into(),
                color: format!("#{index}{index}{index}"),
                paths: vec![TopologyPath {
                    stations: if index == 1 {
                        vec!["b".into(), "a".into()]
                    } else {
                        vec!["a".into(), "b".into()]
                    },
                    closed: false,
                }],
            })
            .collect();
        MetroTopology {
            options: options(),
            stations,
            lines,
        }
    }

    #[test]
    fn renders_paths_stations_labels_and_downward_cartesian_y_axis() {
        let svg = render_topology_svg(&topology()).unwrap();

        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("data-line-id=\"red&quot;line\""));
        assert!(svg.contains("d=\"M48 48 L208 208\""));
        assert!(svg.contains("data-station-id=\"south&amp;west\""));
        assert!(svg.contains(">South &lt;West&gt;</text>"));
        assert!(svg.contains("r=\"9\" fill=\"#fedcba\""));
        assert!(svg.contains("r=\"9\" fill=\"none\" stroke=\"#f00\" stroke-width=\"2\""));
        assert!(
            svg.contains("<rect x=\"0\" y=\"0\" width=\"416\" height=\"256\" fill=\"#abcdef\" />")
        );
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn explicit_right_up_axes_preserve_the_previous_orientation() {
        let mut topology = topology();
        topology.options.coordinates = TopologyCoordinateOptions::Cartesian {
            axes: TopologyCartesianAxes::RightUp,
        };

        let svg = render_topology_svg(&topology).unwrap();

        assert!(svg.contains("d=\"M48 208 L208 48\""));
    }

    #[test]
    fn equivalent_geographic_axes_render_identically() {
        let mut east_north = topology();
        east_north.options.coordinates = TopologyCoordinateOptions::Geographic {
            axes: TopologyGeographicAxes::EastNorth,
        };
        east_north.stations[0].position = TopologyPosition { x: -118.0, y: 34.0 };
        east_north.stations[1].position = TopologyPosition { x: -117.9, y: 34.1 };

        let mut north_west = east_north.clone();
        north_west.options.coordinates = TopologyCoordinateOptions::Geographic {
            axes: TopologyGeographicAxes::NorthWest,
        };
        north_west.stations[0].position = TopologyPosition { x: 34.0, y: 118.0 };
        north_west.stations[1].position = TopologyPosition { x: 34.1, y: 117.9 };

        assert_eq!(
            render_topology_svg(&east_north).unwrap(),
            render_topology_svg(&north_west).unwrap()
        );
    }

    #[test]
    fn omits_the_background_rectangle_when_transparent() {
        let mut topology = topology();
        topology.options.background = TopologyBackgroundOptions::Transparent;

        let svg = render_topology_svg(&topology).unwrap();

        assert!(!svg.contains("<rect"));
    }

    #[test]
    fn renders_the_configured_line_width() {
        let mut topology = topology();
        topology.options.lines.width = TopologyLength::new(12.0).unwrap();

        let svg = render_topology_svg(&topology).unwrap();

        assert!(svg.contains("stroke=\"#f00\" stroke-width=\"12\""));
    }

    #[test]
    fn scales_line_and_station_lengths() {
        let mut common = topology();
        common.options.scale = TopologyScale::new(0.1).unwrap();
        common.options.lines.width = TopologyLength::new(80.0).unwrap();
        common.options.stations.common.fill.diameter = TopologyLength::new(100.0).unwrap();
        common.options.stations.common.stroke.width = TopologyLength::new(25.0).unwrap();
        common.options.stations.common.stroke.alignment = TopologyStrokeAlignment::Outside;

        let svg = render_topology_svg(&common).unwrap();

        assert!(svg.contains("stroke=\"#f00\" stroke-width=\"8\""));
        assert!(svg.contains("r=\"5\" fill=\"#fedcba\""));
        assert!(svg.contains("r=\"6.25\" fill=\"none\" stroke=\"#f00\" stroke-width=\"2.5\""));

        let mut interchange = horizontal_shared_topology(2);
        interchange.options.scale = TopologyScale::new(0.1).unwrap();
        interchange.options.lines.width = TopologyLength::new(80.0).unwrap();
        interchange.options.stations.interchange.fill.width = TopologyLength::new(125.0).unwrap();
        interchange.options.stations.interchange.stroke.width = TopologyLength::new(25.0).unwrap();

        let svg = render_topology_svg(&interchange).unwrap();

        assert!(svg.contains("r=\"6.25\" fill=\"#eeeeee\""));
        assert!(svg.contains("r=\"7.5\" fill=\"none\" stroke=\"#111111\" stroke-width=\"2.5\""));
    }

    #[test]
    fn omits_station_name_labels_when_hidden() {
        let mut topology = topology();
        topology.options.labels.hidden = true;

        let svg = render_topology_svg(&topology).unwrap();

        assert!(!svg.contains("<text"));
        assert!(!svg.contains("South &lt;West&gt;"));
        assert!(svg.contains("data-station-id=\"south&amp;west\""));
    }

    #[test]
    fn renders_primary_and_optional_secondary_station_labels() {
        let mut topology = topology();
        topology.options.languages.set.insert("de-ch".into());
        topology.options.languages.primary = "de-ch".into();
        topology.options.languages.secondary = Some("en".into());
        topology.stations[0]
            .names
            .insert("de-ch".into(), vec!["Süd & West".into()]);
        topology.stations[1]
            .names
            .insert("de-ch".into(), vec!["Nord".into()]);
        topology.lines[0]
            .names
            .insert("de-ch".into(), vec!["Rote Linie".into()]);

        let svg = render_topology_svg(&topology).unwrap();
        assert!(svg.contains("<tspan x=\""));
        assert!(svg.contains(">Süd &amp; West</tspan>"));
        assert!(svg.contains(">South &lt;West&gt;</tspan>"));

        topology.options.languages.secondary = None;
        let svg = render_topology_svg(&topology).unwrap();
        assert!(svg.contains(">Süd &amp; West</text>"));
        assert!(!svg.contains("<tspan"));
    }

    #[test]
    fn renders_two_shared_lines_in_separate_lanes() {
        let svg = render_topology_svg(&horizontal_shared_topology(2)).unwrap();

        assert!(svg.contains("data-line-id=\"line-0\" d=\"M48 48 L72 42.5 L184 42.5 L208 48\""));
        assert!(svg.contains("data-line-id=\"line-1\" d=\"M208 48 L184 53.5 L72 53.5 L48 48\""));
        assert!(svg.contains("r=\"10\" fill=\"#eeeeee\""));
        assert!(svg.contains("r=\"12\" fill=\"none\" stroke=\"#111111\" stroke-width=\"4\""));
    }

    #[test]
    fn renders_three_shared_lines_symmetrically() {
        let svg = render_topology_svg(&horizontal_shared_topology(3)).unwrap();

        assert!(svg.contains("d=\"M48 48 L72 37 L184 37 L208 48\""));
        assert!(svg.contains("d=\"M208 48 L48 48\""));
        assert!(svg.contains("d=\"M48 48 L72 59 L184 59 L208 48\""));
    }

    #[test]
    fn indexes_closing_segments_and_deduplicates_a_line_lane() {
        let mut topology = horizontal_shared_topology(1);
        topology.stations.push(TopologyStation {
            id: "c".into(),
            names: [("en".into(), vec!["Test".into()])].into(),
            position: TopologyPosition { x: 1.0, y: 1.0 },
        });
        topology.lines[0].paths = vec![
            TopologyPath {
                stations: vec!["a".into(), "b".into(), "c".into()],
                closed: true,
            },
            TopologyPath {
                stations: vec!["a".into(), "b".into()],
                closed: false,
            },
        ];

        let lanes = segment_lanes(&topology);
        assert_eq!(lanes[&SegmentKey::new("a", "c")], vec![0]);
        assert_eq!(lanes[&SegmentKey::new("a", "b")], vec![0]);
    }

    #[test]
    fn reports_unknown_stations() {
        let mut topology = topology();
        topology.lines[0].paths[0].stations.push("missing".into());

        assert_eq!(
            render_topology_svg(&topology),
            Err(TopologyRenderError::UnknownStation {
                line: "red\"line".into(),
                station: "missing".into(),
            })
        );
    }

    #[test]
    fn renders_an_empty_topology() {
        let svg = render_topology_svg(&MetroTopology {
            options: options(),
            stations: vec![],
            lines: vec![],
        })
        .unwrap();

        assert!(svg.contains("viewBox=\"0 0 256 96\""));
    }
}
