use std::fmt::Write;

use mtrd::{
    ContractedNode, ContractedTopology, EdgeEndpoint, IncidentEdge, RetentionReason,
    TopologyRenderError, render_topology_svg,
};

pub(super) fn render_contracted_topology_svg(
    topology: &ContractedTopology,
) -> Result<String, TopologyRenderError> {
    let mut retained = vec![false; topology.source.stations.len()];
    for node in &topology.nodes {
        if let ContractedNode::Station {
            source_station_index,
            ..
        } = node
        {
            retained[*source_station_index] = true;
        }
    }

    let mut svg = render_topology_svg(&topology.source)?;
    for (station_index, station) in topology.source.stations.iter().enumerate() {
        if retained[station_index] {
            continue;
        }
        remove_station_element(&mut svg, &station.id);
    }
    let closing = svg
        .rfind("</svg>")
        .expect("the topology renderer always emits a closing SVG element");
    svg.truncate(closing);
    svg.push_str("  <g data-contraction-debug=\"true\" visibility=\"hidden\">\n");

    for (edge_index, edge) in topology.edges.iter().enumerate() {
        let membership = topology
            .paths
            .iter()
            .filter(|path| {
                path.traversals
                    .iter()
                    .any(|traversal| traversal.edge_index == edge_index)
            })
            .map(|path| format!("{}:{}", path.line_index, path.path_index))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            svg,
            "    <g data-contracted-edge=\"{edge_index}\" data-path-membership=\"{membership}\" />"
        )
        .unwrap();

        let endpoint_stations = [edge.endpoint_a, edge.endpoint_b]
            .into_iter()
            .filter_map(|node| match topology.nodes[node] {
                ContractedNode::Station {
                    source_station_index,
                    ..
                } => Some(source_station_index),
                ContractedNode::VirtualCrossing { .. } => None,
            })
            .collect::<Vec<_>>();
        for station in edge
            .source_station_indices
            .iter()
            .filter(|station| !endpoint_stations.contains(station))
        {
            writeln!(svg, "    <g data-contracted-station=\"{station}\" />").unwrap();
        }
    }

    for node in &topology.nodes {
        match node {
            ContractedNode::Station {
                source_station_index,
                retention_reasons,
            } => {
                let reasons = retention_reasons
                    .iter()
                    .map(reason_name)
                    .collect::<Vec<_>>()
                    .join(" ");
                writeln!(
                    svg,
                    "    <g data-retained-station=\"{source_station_index}\" data-retention-reasons=\"{reasons}\" />"
                )
                .unwrap();
            }
            ContractedNode::VirtualCrossing {
                incident_edges,
                continuations,
                ..
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
                    "    <g data-virtual-crossing=\"true\" data-incident-edges=\"{incident}\" data-continuations=\"{paired}\" />"
                )
                .unwrap();
            }
        }
    }

    for order in &topology.neighbor_orders {
        writeln!(
            svg,
            "    <g data-neighbor-order=\"{}\">",
            order.source_station_index
        )
        .unwrap();
        for group in &order.neighbor_groups {
            let members = group
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            writeln!(svg, "      <g data-neighbor-group=\"{members}\" />").unwrap();
        }
        svg.push_str("    </g>\n");
    }
    svg.push_str("  </g>\n</svg>\n");
    Ok(svg)
}

fn remove_station_element(svg: &mut String, station_id: &str) {
    let opening = format!("<g data-station-id=\"{}\"", xml_escape(station_id));
    let start = svg
        .find(&opening)
        .expect("the topology renderer emits every station element");
    let end = start
        + svg[start..]
            .find("</g>")
            .expect("the topology renderer closes every station element")
        + "</g>".len();
    svg.drain(start..end);
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

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
