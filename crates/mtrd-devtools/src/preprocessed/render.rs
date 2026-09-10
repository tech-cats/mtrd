use std::fmt::Write;

use mtrd::{
    EdgeEndpoint, IncidentEdge, MetroTopology, PreprocessedNode, PreprocessedTopology,
    RetentionReason, TopologyPosition,
};

pub(super) fn render_preprocessed_topology_svg(topology: &PreprocessedTopology) -> String {
    let view_box = view_box(topology);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" viewBox=\"{view_box}\">\n  <title>Preprocessed metro topology</title>\n"
    );
    for (edge_index, edge) in topology.edges.iter().enumerate() {
        let membership = topology
            .paths
            .iter()
            .filter(|path| {
                path.traversals
                    .iter()
                    .any(|item| item.edge_index == edge_index)
            })
            .map(|path| format!("{}:{}", path.line_index, path.path_index))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            svg,
            "  <g data-reduced-edge=\"{edge_index}\" data-path-membership=\"{membership}\">"
        )
        .unwrap();
        let mut positions = vec![node_position(
            edge.endpoint_a,
            &topology.source,
            &topology.nodes,
        )];
        for &station in &edge.source_station_indices {
            let position = topology.source.stations[station].position;
            if positions.last() != Some(&position) {
                positions.push(position);
            }
        }
        let end = node_position(edge.endpoint_b, &topology.source, &topology.nodes);
        if positions.last() != Some(&end) {
            positions.push(end);
        }
        for pair in positions.windows(2) {
            writeln!(
                svg,
                "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#555\" />",
                pair[0].x, -pair[0].y, pair[1].x, -pair[1].y,
            )
            .unwrap();
        }
        let endpoint_stations = [edge.endpoint_a, edge.endpoint_b]
            .into_iter()
            .filter_map(|node| match topology.nodes[node] {
                PreprocessedNode::Station {
                    source_station_index,
                    ..
                } => Some(source_station_index),
                PreprocessedNode::VirtualCrossing { .. } => None,
            })
            .collect::<Vec<_>>();
        for station in edge
            .source_station_indices
            .iter()
            .filter(|station| !endpoint_stations.contains(station))
        {
            let position = topology.source.stations[*station].position;
            writeln!(
                svg,
                "    <circle data-contracted-station=\"{station}\" cx=\"{}\" cy=\"{}\" r=\"2\" fill=\"#999\" />",
                position.x, -position.y
            )
            .unwrap();
        }
        svg.push_str("  </g>\n");
    }
    for node in &topology.nodes {
        match node {
            PreprocessedNode::Station {
                source_station_index,
                retention_reasons,
            } => {
                let position = topology.source.stations[*source_station_index].position;
                let reasons = retention_reasons
                    .iter()
                    .map(reason_name)
                    .collect::<Vec<_>>()
                    .join(" ");
                writeln!(
                    svg,
                    "  <circle data-retained-station=\"{source_station_index}\" data-retention-reasons=\"{reasons}\" cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"#fff\" stroke=\"#111\" />",
                    position.x,
                    -position.y,
                )
                .unwrap();
            }
            PreprocessedNode::VirtualCrossing {
                position,
                incident_edges,
                continuations,
            } => {
                let incident = incident_edges
                    .iter()
                    .map(incident_name)
                    .collect::<Vec<_>>()
                    .join(" ");
                let paired = continuations
                    .iter()
                    .map(|continuation| {
                        format!(
                            "{}:{}+{}",
                            continuation.physical_edge_index,
                            incident_name(&continuation.incident_edges[0]),
                            incident_name(&continuation.incident_edges[1]),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                writeln!(
                    svg,
                    "  <circle data-virtual-crossing=\"true\" data-incident-edges=\"{incident}\" data-continuations=\"{paired}\" cx=\"{}\" cy=\"{}\" r=\"3\" fill=\"none\" stroke=\"#b000ff\" />",
                    position.x,
                    -position.y,
                )
                .unwrap();
            }
        }
    }
    for order in &topology.neighbor_orders {
        writeln!(
            svg,
            "  <g data-neighbor-order=\"{}\">",
            order.source_station_index
        )
        .unwrap();
        for group in &order.neighbor_groups {
            let members = group
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(svg, "    <g data-neighbor-group=\"{members}\" />").unwrap();
        }
        svg.push_str("  </g>\n");
    }
    svg.push_str("</svg>\n");
    svg
}

fn node_position(
    node: usize,
    topology: &MetroTopology,
    nodes: &[PreprocessedNode],
) -> TopologyPosition {
    match nodes[node] {
        PreprocessedNode::Station {
            source_station_index,
            ..
        } => topology.stations[source_station_index].position,
        PreprocessedNode::VirtualCrossing { position, .. } => position,
    }
}

fn view_box(topology: &PreprocessedTopology) -> String {
    let Some(first) = topology.source.stations.first() else {
        return "0 0 1 1".into();
    };
    let mut min_x = first.position.x;
    let mut max_x = first.position.x;
    let mut min_y = -first.position.y;
    let mut max_y = -first.position.y;
    for station in topology.source.stations.iter().skip(1) {
        min_x = min_x.min(station.position.x);
        max_x = max_x.max(station.position.x);
        min_y = min_y.min(-station.position.y);
        max_y = max_y.max(-station.position.y);
    }
    let span = (max_x - min_x).max(max_y - min_y);
    let padding = (span * 0.05).max(6.0);
    let width = max_x - min_x + 2.0 * padding;
    let height = max_y - min_y + 2.0 * padding;
    if !padding.is_finite() || !width.is_finite() || !height.is_finite() {
        return "0 0 1 1".into();
    }
    format!("{} {} {width} {height}", min_x - padding, min_y - padding)
}

fn incident_name(incident: &IncidentEdge) -> String {
    format!(
        "{}{}",
        incident.edge_index,
        match incident.endpoint {
            EdgeEndpoint::A => "a",
            EdgeEndpoint::B => "b",
        }
    )
}

fn reason_name(reason: &RetentionReason) -> &'static str {
    match reason {
        RetentionReason::NonDegreeTwo => "non-degree-two",
        RetentionReason::PathEndpoint { .. } => "path-endpoint",
        RetentionReason::Interchange => "interchange",
        RetentionReason::ShortCycle { .. } => "short-cycle",
        RetentionReason::CycleSeed { .. } => "cycle-seed",
        RetentionReason::CycleSpacing { .. } => "cycle-spacing",
        RetentionReason::Policy { .. } => "policy",
    }
}
