mod analysis;
mod warp;

use super::{ContractedNode, ContractedTopology, TopologyPosition};

pub(crate) use analysis::analyze_canonical_density;
pub use analysis::{
    DensityAnalysis, DensityError, DensityTriangle, ResolvedDensityOptions, analyze_density,
};
pub(crate) use warp::DensityWarp;
pub use warp::{
    DensityWarpAnalysis, DensityWarpDiagnostics, WarpedSegment, WarpedStation, analyze_density_warp,
};

/// Warped positions for contracted nodes, indexed like
/// [`ContractedTopology::nodes`].
///
/// Original positions remain available through the contracted topology. This
/// separate collection is consumed only as geographic targets by later layout
/// stages.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WarpedLayoutTargets {
    node_positions: Vec<TopologyPosition>,
}

impl WarpedLayoutTargets {
    pub(crate) fn from_contracted(
        contracted: &ContractedTopology,
        warp: &DensityWarp,
    ) -> Result<Self, DensityError> {
        let node_positions = contracted
            .nodes
            .iter()
            .map(|node| {
                let source_position = match node {
                    ContractedNode::Station {
                        source_station_index,
                        ..
                    } => contracted.source.stations[*source_station_index].position,
                    ContractedNode::VirtualCrossing { position, .. } => *position,
                };
                warp.transform(source_position)
            })
            .collect::<Result<_, _>>()?;

        Ok(Self { node_positions })
    }

    pub(crate) fn len(&self) -> usize {
        self.node_positions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ContractedEdge, ContractedPath, MetroTopology, RetentionReason, StationNeighborOrder,
        TopologyOptions, TopologyStation,
    };

    fn options() -> TopologyOptions {
        serde_yaml::from_str(
            r##"
languages: { set: [en], primary: en }
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

    fn contracted_topology() -> ContractedTopology {
        ContractedTopology {
            source: MetroTopology {
                options: options(),
                stations: vec![TopologyStation {
                    id: "station".into(),
                    names: Default::default(),
                    position: TopologyPosition { x: 2.0, y: 3.0 },
                }],
                lines: Vec::new(),
            },
            nodes: vec![
                ContractedNode::Station {
                    source_station_index: 0,
                    retention_reasons: vec![RetentionReason::NonDegreeTwo],
                },
                ContractedNode::VirtualCrossing {
                    position: TopologyPosition { x: 5.0, y: 7.0 },
                    incident_edges: Vec::new(),
                    continuations: Vec::new(),
                },
            ],
            edges: Vec::<ContractedEdge>::new(),
            paths: Vec::<ContractedPath>::new(),
            neighbor_orders: Vec::<StationNeighborOrder>::new(),
        }
    }

    fn identity_warp() -> DensityWarp {
        DensityWarp::test_identity([0.0, 0.0, 10.0, 10.0])
    }

    #[test]
    fn identity_preserves_arbitrary_positions() {
        let warp = identity_warp();
        let position = TopologyPosition { x: 4.5, y: 8.25 };

        assert_eq!(warp.transform(position).unwrap(), position);
    }

    #[test]
    fn identity_targets_cover_stations_and_virtual_crossings() {
        let contracted = contracted_topology();
        let original = contracted.clone();
        let warp = identity_warp();
        let targets = WarpedLayoutTargets::from_contracted(&contracted, &warp).unwrap();

        assert_eq!(
            targets.node_positions,
            [
                TopologyPosition { x: 2.0, y: 3.0 },
                TopologyPosition { x: 5.0, y: 7.0 },
            ]
        );
        assert_eq!(targets.len(), contracted.nodes.len());
        assert_eq!(contracted, original);
    }
}
