# Topology contracting

Topology contracting converts an initialised [`MetroTopology`](../crates/mtrd/src/topology.rs)
into the deterministic physical graph used by schematic generation. Manifest
initialisation validates and canonicalises configured coordinates first.
Contracting then derives physical connectivity from ordered line paths, retains
structurally significant stations, and contracts eligible degree-2 stations
without losing the source paths needed by later stages.

The contraction result is a pipeline model rather than a second public
manifest. The private developer tools can serialise it for inspection, but
that representation has no independent compatibility contract.

## Current workflow

The public CLI entry point is:

```text
mtrd generate <topology.yaml> [schematic.yaml]
```

`g` and `gen` are aliases for `generate`. The input may be YAML, YML, or JSON.
An explicit output path and `-T`/`--timestamp` naming are accepted.

Coordinate initialisation and contracting are implemented, but
topology-to-schematic layout is not. A valid input is therefore fully validated,
canonicalised, and contracted before the command exits
unsuccessfully with:

```text
schematic generation is not implemented yet
```

The command does not create the requested schematic or any intermediate files.
Its configured default destination is `<input stem>.schematic.yaml`, while
timestamp naming selects `mtrd-<microsecond timestamp>.schematic.yaml` in the
current directory. Neither path is written while generation remains
stage-unavailable.

The equivalent library entry point is `generate_schematic(&MetroTopology)`.
It currently returns `SchematicGenerationError::StageUnavailable` after
successful contraction, or a typed contraction error if the topology
cannot be prepared.

For direct pipeline access, call `MetroTopology::canonicalize_coordinates`
after deserialisation and pass the returned topology to `contract_topology`.
The latter accepts only canonical Cartesian `r-d` coordinates, keeping
manifest initialisation distinct from contraction.

There is no public `contract` command, serialised contraction format, or
intermediate-output flag.

## Topology authoring contract

Authors continue to use the existing path-oriented topology manifest:

```rust
pub struct MetroTopology {
    pub stations: Vec<TopologyStation>,
    pub lines: Vec<TopologyLine>,
}

pub struct TopologyLine {
    pub id: String,
    // names and color omitted
    pub paths: Vec<TopologyPath>,
}

pub struct TopologyPath {
    pub stations: Vec<String>,
    pub closed: bool,
}
```

This is deliberately a human-oriented YAML/JSON representation rather than a
canonical mathematical graph. Ordered paths avoid repeated edge declarations
and preserve line traversal naturally. Contracting derives graph properties;
authors do not declare physical edges, degrees, or interchange status.

The manifest schema remains strict. In addition to the ordinary ID, reference,
and path rules, topology validation requires:

- finite station positions;
- distinct station IDs to have distinct positions;
- at least two stations in an open path;
- at least three stations in a closed path; and
- no repeated station within one path.

Positions remain two-element `[x, y]` arrays in the configured Cartesian or
geographic coordinate system. Before contraction, initialisation applies the
same axis normalisation or local equirectangular projection as topology
rendering and stores rightward/downward Cartesian positions in the owned source
topology. It resets only `options.coordinates` to canonical Cartesian `r-d`;
the source background, labels, line and station styles, and scale are retained.
Rendering-specific viewport limits are not contraction constraints, although
`mtrd check -t` and the topology renderer still enforce their own renderability
range.

## Physical graph normalisation

Every station initially identifies a possible physical vertex. Each adjacent
pair in an open path contributes one source segment occurrence. A closed path
also contributes its last-to-first segment, so a valid closed path containing
`m` stations contains exactly `m` segments.

A physical edge is keyed by the unordered pair of its distinct endpoint
stations. This gives the following guarantees:

- lines and paths using the same station pair share one physical edge;
- opposite traversals share that edge without losing their direction;
- every source occurrence retains its line, path, and segment identity; and
- physical degree counts distinct neighbours, not line count or occurrence
  count.

Source station order defines the canonical orientation of physical and reduced
edges. Reversing a source path changes traversal direction, but not the stored
orientation of the corresponding reduced edge.

An isolated non-transfer crossing splits each of its two physical edges into
sections after this normalisation. The split is internal and does not add a
station or change which lines serve either edge.

All source lines and paths remain separately identifiable. Shared track never
merges line membership or erases path direction.

## Retained and contracted stations

A served station is retained if any of these conditions applies:

- its physical degree is non-zero and not exactly two;
- it is the first or last station of an open path;
- it is served by more than one distinct line; or
- a closed-cycle rule selects it.

Every other served degree-2 station is contractible. Degree-0 stations remain
in the owned source topology but are omitted from the reduced nodes and edges.
Disconnected served components are preserved.

Every automatically derived virtual crossing is also retained so contraction
cannot erase the point at which its two non-transfer passages cross. It does
not have station retention reasons and is never classified as an interchange.

The first and last station of every open path are called **path endpoints**.
This is intentionally different from “terminus”: a branched line may use its
branching station as the endpoint of several paths even when passenger service
continues through it.

An **interchange** is derived from service by more than one distinct
`TopologyLine::id`. Repeated occurrences or multiple paths belonging to one
line do not create an interchange. No `interchange` field is added to the
topology manifest.

Retention reasons accumulate in a stable semantic order. For example, a
degree-2 station may be retained as both a path endpoint and an interchange.
The internal representation also supports named policy retention, which is
used by internal tests; no public custom-policy interface is currently
available.

## Closed cycles

Closed paths receive additional retained stations when their natural retained
set would allow contraction to erase or excessively collapse the ring. Each
closed path is analysed independently using the natural retention decisions
made before any cycle rule is applied.

Let `m` be the number of source segments in the closed path.

- If `m < 6`, every station on the path is retained as part of a short cycle.
- If `m >= 6` and the path has no naturally retained station, its first source
  station is retained as a deterministic cycle seed.
- Between consecutive retained stations, including the wraparound interval,
  the forward source-segment distance `d` must satisfy `3 * d <= m`.

For an interval longer than `floor(m / 3)`, contracting inserts the minimum
number of stations needed to split it into balanced integer lengths. The
lengths differ by at most one, with source path order providing the final
tie-break. An odd interval with one required addition uses the first midpoint
encountered in forward path order. Intervals requiring three additions use a
midpoint followed by the midpoint of each resulting half.

For example, an otherwise unretained 12-station ring retains its first station
as a seed and adds two stations. The resulting intervals have lengths
`4, 4, 4`. Ten- and eleven-station rings add three stations and produce four
intervals, each no longer than three segments.

The threshold always uses the original cycle length `m`, not the length of a
newly split interval. This prevents repeated midpoint selection from retaining
more stations than necessary.

## Contraction and provenance

Contracting walks the split physical graph into maximal chains. Each reduced
edge has retained station or virtual-crossing endpoints and zero or more
contractible degree-2 stations in its interior. Its encountered source stations
are stored in canonical orientation.

Separate reduced-path traversals preserve the relationship between the source
manifest and these chains. The result maintains these invariants:

- every source segment occurrence belongs to one or more ordered traversal
  spans whose fractions cover that segment exactly;
- line index, path index, source segment order and fractions, traversal
  direction, and the original `closed` value remain available;
- each reduced-chain endpoint is retained;
- each reduced-chain interior station is contractible;
- shared and opposite-direction traversals remain distinguishable;
- continuation pairs at a virtual crossing identify the two sides of each
  non-transfer passage;
- source station names and positions, line names and colours, and isolated
  stations remain available through the owned source topology; and
- no served station, path, line, or disconnected served component is lost.

The model stores path traversal provenance directly rather than duplicating
derived edge-membership lists. When a crossing divides a source segment, each
span records its start and end fractions in source traversal order. Concatenated
spans reconstruct the exact source path while avoiding two independently
mutable representations of the same relationship.

## Topology-equivalence information

Connectivity alone does not describe a source embedding. At every physical
station with degree greater than two, contracting records the cyclic order of
its distinct neighbours from their source positions.

Neighbours with the same bearing form one order-equivalence group. The group
occupies one position in the cyclic order, while its members may be reordered
freely. Source station order gives members a deterministic stored order; it is
not an additional geometric constraint.

Parallel line-strand order on a shared physical edge is not constrained at this
stage. Individual occurrences remain available through provenance, but source
line order is not treated as a required geometric ordering.

This neighbour-order information is stored in the contraction result. No
coarse optimiser, capsule placement, or fine optimiser currently consumes it.
Virtual crossings separately retain the cyclic order of their four incident
reduced edges and the two continuation pairs that must remain disconnected.

## Geometric intersections

Contracting examines the straight source segments implied by station
positions. Segments meeting at a shared station ID are the ordinary valid case,
and repeated occurrences of the same physical edge are valid shared track.

An intersection is automatically accepted as a **virtual crossing** when it is
strictly interior to exactly two distinct, non-collinear physical edges. All
source occurrences on those edges are preserved, so a shared-track edge may
cross another edge without losing any of its line paths. One physical edge may
contain several distinct isolated crossings; its sections and provenance spans
follow canonical edge order.

For example, the Shipaiqiao–Gangding segment from `(4, 0)` to `(6, 0)`
crosses the Liede–Tianhe Park segment from `(6, 3)` to `(4, -3)` at `(5, 0)`.
Because the point lies strictly inside exactly those two physical edges and has
no station ID, contracting accepts it as a non-transfer virtual crossing.

The virtual crossing splits both physical edges and has four incident reduced
edge sides. Two explicit continuation pairs preserve the original passages:
travelling through one pair never permits transfer to the other. A virtual
crossing therefore constrains the source embedding without becoming a station,
an interchange, or a `SchematicManifest` station.

Boundary cases stop generation with a typed `TopologyContractError` and a
diagnostic naming every involved line, path, and segment:

- `VirtualCrossingDecisionRequired` is returned when a station endpoint lies
  strictly inside another physical segment. Automatic handling is unsafe
  because the geometry alone cannot determine whether the station represents a
  branch, transfer, or non-transfer crossing.
- Collinear physical edges with a non-zero overlap are rejected because there
  is no single crossing point or unambiguous continuation boundary. When an
  endpoint lies inside the other edge, the station-on-segment diagnosis below
  takes precedence; otherwise the typed reason is `CollinearOverlap`.
- `UnsupportedTopologyIntersection` with `MultiplePhysicalEdges` rejects three
  or more physical edges meeting at one non-station point because two
  continuation pairs are insufficient to describe the junction.

Station-on-segment cases are classified before generic collinear intersection
handling. Thus equal-bearing branches with a nearer adjacent station produce
the decision-required error rather than being treated as an ordinary neighbour
ordering case.

The implementation never guesses in these rejected cases. There is currently
no UI, persisted authoring decision, or manifest field for resolving them.

## Determinism

Equal inputs produce structurally equal contraction results and byte-for-byte
equal internal debug SVG. Stable ordering follows source data rather than hash
iteration:

- served nodes follow source station order;
- occurrences follow source line, path, and segment order;
- edge orientation follows source station order;
- virtual crossings follow physical-edge discovery and canonical position
  order;
- multiple splits of one physical edge follow its canonical direction;
- reduced-edge discovery follows the first source occurrence of its first
  unvisited physical segment;
- reduced paths follow source line and path order;
- cycle placement tie-breaks follow source path order; and
- retention reasons follow their semantic enum order.

The private debug renderer renders the source paths and retained stations with
the canonicalised source's original rendering options. Contracted and isolated
station symbols are omitted. The SVG also embeds hidden diagnostic metadata for
retained and contracted stations, virtual crossings and their continuation
pairs, reduced edges, path membership, retention reasons, and neighbour-order
groups. It is intended for unit tests and development inspection only; its
structure is not public API.

## Unsupported functionality

Topology contracting does not currently provide:

- topology-to-schematic layout or station reinsertion;
- octilinear grid construction or routing;
- optimisation objectives or penalty functions;
- capsule, port, or anchor assignment;
- automatic resolution of ambiguous crossing boundary cases or interactive
  virtual-crossing decisions;
- real-world edge shapes or a geographical coordinate reference system;
- a public contraction command or debug renderer; or
- public intermediate manifests or SVG output flags.

Schematic `Corner` values also do not belong in the topology manifest. Corners
are routing output; source geography is preserved only through station
positions and the ordered indices of contracted stations.
