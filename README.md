# MTRD: MeTRo Draw

`mtrd` works with metro topology and schematic manifests. YAML is the primary
human-editable format, while JSON uses the same schemas for WebUI interchange.

## Quick Start

### 1. Install the CLI

Install the stable Rust toolchain and run this from the repository root:

```console
cargo install --path crates/mtrd
mtrd --version
```

The remaining examples assume Cargo's binary directory is on `PATH`. If it is
not, invoke the installed binary by its full path or use
`cargo run -q -p mtrd --` in place of `mtrd`.

### 2. Create a topology manifest

Save this small, complete example as `my-map.yaml`:

```yaml
options:
  lines: { width: 6.0 }
  stations:
    common:
      fill: { diameter: 10.0, color: { type: unified, value: '#FFFFFF' } }
      stroke: { width: 2.0, alignment: outside, color: { type: follow-line } }
    interchange:
      fill: { width: 12.0, color: '#FFFFFF' }
      stroke: { width: 2.0, alignment: outside, color: '#000000' }
stations:
  - id: central
    names:
      en: [Central]
    position: [0.0, 0.0]
  - id: park
    names:
      en: [Park]
    position: [120.0, 0.0]
lines:
  - id: red
    names:
      en: [Red Line]
    color: '#E53935'
    paths:
      - stations: [central, park]
        closed: false
```

Station and line IDs must be unique. A path lists station IDs in travel order,
and `names.en[0]` is the displayed English label. Positions are `[x, y]`, with
positive `x` pointing right and positive `y` pointing down by default.

### 3. Check and render it

Validate the file before rendering it:

```console
mtrd check --topology my-map.yaml
# my-map.yaml: valid

mtrd render --topology --output my-map.svg my-map.yaml
# my-map.svg
```

Open `my-map.svg` in a browser or SVG viewer. After changing station positions,
labels, line colours, or path order, repeat these two commands. Omitting
`--output` writes `my-map.yaml.svg`.

To start from a larger working example instead, ask the CLI to create one:

```console
mtrd example topology > my-map.yaml
```

Use `mtrd example schematic > my-schematic.yaml` when you need an explicitly
laid-out schematic rather than a topology graph. Bundled examples are printed
as YAML. When checking or rendering a saved file, its extension selects YAML or
JSON and the `--topology`/`--schematic` flag selects the strict manifest schema.

## Commands

```console
# Print the installed version. These flag forms are equivalent.
mtrd --version
mtrd -V

# Convert in either direction. File extensions select the formats.
mtrd convert map.yaml map.json
mtrd convert map.json map.yaml

# Print a bundled example manifest. The short aliases shown are equivalent.
mtrd example topology
mtrd example topo
mtrd example t
mtrd example schematic
mtrd example s

# Check topology syntax, schema, and renderability.
mtrd check -t topology.yaml

# Check a schematic manifest's syntax and schema.
mtrd check -s schematic.yaml

# The long manifest-kind flags and verbosity modes are also supported.
mtrd check --topology -v topology.yaml
mtrd check --schematic -vv schematic.yaml

# Render the topology graph as SVG. These flag forms are equivalent.
mtrd render -to map.svg map.yaml
mtrd render -o map.svg -t map.yaml

# Render a schematic manifest as an SVG schematic map.
mtrd render -s schematic.yaml
mtrd render --schematic -o schematic.svg schematic.json

# Without -o or -T, append .svg to the input path.
mtrd render --topology map.yaml

# Use -T to write ./mtrd-<microsecond timestamp>.svg.
mtrd render -tT map.yaml

# Initialise and contract a topology for schematic generation (currently stops before layout).
mtrd generate topology.yaml
# Short aliases are also available.
mtrd g topology.yaml schematic.yaml
mtrd gen -T topology.yaml
```

`check` requires exactly one of `-t`/`--topology` or `-s`/`--schematic` and
accepts `.yaml`, `.yml`, and `.json` inputs. For a topology manifest, a
successful check means the manifest can be processed by the topology renderer.
It validates that:

- station and line IDs are non-empty and unique;
- distinct stations do not have identical positions;
- every station referenced by a line path exists;
- both coordinates of every station are finite and within the renderer's
  supported numeric range;
- geographic longitudes are within `[-180, 180]` and latitudes are within
  `[-90, 90]` after applying the configured axes;
- an open path contains at least two stations;
- a closed path contains at least three stations; and
- a station occurs at most once in a single path.

These checks concern the manifest itself. Rendering can still fail because of
an output filesystem error, such as an unwritable destination.

For a schematic manifest, a successful check means it satisfies the strict
`SchematicManifest` YAML/JSON schema and every semantic and geometric
invariant required by the renderer. This includes valid IDs and references,
station-port compatibility, complete interchange anchors, octilinear legs,
explicit and feasible corners, exclusive path geometry, finite render bounds,
finite positions, finite positive lengths, and rejection of unknown fields.

With `check -v`, `mtrd` prints the parsed map as canonical YAML after it passes
validation. With `check -vv`, it prints the detailed Rust debug representation.

## Topology manifest library API

`MetroTopology` supports strict YAML and equivalent JSON serialisation through
`from_yaml`, `to_yaml`, `from_json`, and `to_json`. Its global `options` set
the coordinate system and scale, either an opaque background `color` or
`transparent: true`, the width of line strokes, and the fill and stroke styling
of common and interchange stations.

Topology `background`, `coordinates`, `labels`, and `scale` may all be omitted
on input. Their defaults are an opaque `#FFFFFF` background, Cartesian
coordinates with rightward `x` and downward `y` axes, visible labels
(`hidden: false`), and a scale of `1.0`. Canonical YAML and JSON include these
resolved values.

The whole `coordinates` mapping may be omitted, and `axes` may be omitted when
`type` is present. If the mapping is present, `type` is required:

```yaml
options:
  coordinates:
    type: cartesian
    axes: r-d
```

The global coordinate scale defaults to `1.0` and may be overridden alongside
the other global options. It multiplies projected station coordinates for both
coordinate systems:

```yaml
options:
  scale: 3.0
```

The scale must be finite and strictly positive. It multiplies coordinate
spacing, line widths, and common/interchange station fill and stroke lengths.
Canonical YAML and JSON always include the resolved scale, including when it
was omitted on input.

Cartesian axes may be `r-d`, `r-u`, `l-d`, `l-u`, `d-r`, `d-l`, `u-r`, or
`u-l`. Each letter gives the positive direction of the corresponding value in
the station's `[x, y]` position. At the default scale, one Cartesian coordinate
unit is one SVG user unit. The corrected default renders positive `y`
downwards; use explicit `axes: r-u` to preserve the orientation produced for
omitted options by older versions.

Geographic positions are longitude and latitude values whose order and signs
are selected in the same way:

```yaml
options:
  coordinates:
    type: geographic
    axes: e-n
```

Geographic axes may be `e-n`, `e-s`, `w-n`, `w-s`, `n-e`, `n-w`, `s-e`, or
`s-w`; `n-e` is the default. For example, Los Angeles is approximately
`[34.0, -118.0]` with `n-e` and `[34.0, 118.0]` with `n-w`.

The renderer finds the direct numeric longitude and latitude bounds, uses their
average as the centre, and applies a local equirectangular projection with the
IUGG mean Earth radius of `6,371,008.8 m`. Longitude is scaled by the cosine of
the centre latitude. The north-west projected boundary becomes `(0, 0)` in the
renderer's right/down coordinate system. Antimeridian wrapping is not applied.
At the default scale, one projected metre is one SVG user unit. Geographic
topology line widths and common/interchange station fill and stroke lengths are
also specified in metres and converted to SVG user units by the global scale.

Canonical YAML and JSON always include `coordinates`, `type`, and the resolved
`axes`, including when defaults were omitted on input.

The bundled topology example contains an ordinary line, a branched line, and a
loop line. Print it with `mtrd example topology`, or inspect
[`crates/mtrd/examples/topology.yaml`](crates/mtrd/examples/topology.yaml).

An explicit `transparent: false` may accompany a background colour and
`colour` is accepted on input; canonical output uses `color` and omits the
redundant transparency field. A coloured background is rendered across the
entire SVG viewport, while a transparent background emits no background
rectangle. Stations used by more than one distinct line receive interchange
styling; other stations receive common styling. Common-station fill and stroke
colours support the `unified` and `follow-line` policies. Station stroke
alignment is `inside`, `center`, or `outside`, with `centre` accepted on input.
Station-name labels are displayed when `options.labels.hidden` is `false` and
omitted when it is `true`.

Configuration keys and enum values use kebab-case in canonical YAML and JSON.
For example, schematic routes use `station-id` and `single-line`, while Rust
fields and variants retain their conventional `snake_case` and `PascalCase`.
Canonical YAML keeps positions and each locale's names in compact flow style:

```yaml
position: [754.0, 323.0]
names:
  de: [München]
  en: [Munich]
```

`generate`, with aliases `g` and `gen`, validates and canonicalises the
coordinates of its topology input before contracting its physical graph. The
layout stages are not implemented yet, so it currently exits with
`schematic generation is not implemented yet` and creates no output. Once the
remaining stages are implemented, its default destination will be
`<input stem>.schematic.yaml`; an explicit positional destination and `-T`/
`--timestamp` naming are already accepted.
w
## Schematic manifest library API

The library also defines the semantic schematic-map schema documented in
[`docs/schematic-map/v4.md`](docs/schematic-map/v4.md). `SchematicManifest`
supports strict YAML and equivalent JSON serialisation through `from_yaml`,
`to_yaml`, `from_json`, and `to_json`. Schematic positions are `[x, y]` arrays,
lengths are finite positive scalars, and unknown fields are rejected.

Global `options` set either an opaque background `color` or
`transparent: true`, group line styling under `lines`, and station styling
under `stations.common` and `stations.interchange`. An explicit
`transparent: false` may accompany the background colour.
Common-station colours support
the tagged `unified` and `follow-line` policies. Station stroke alignment is
`inside`, `center`, or `outside`; `centre` is accepted on input and serialised
canonically as `center`. Likewise, every `color` field in topology and
schematic manifests also accepts `colour` on input and is serialised
canonically as `color`.

`render_schematic_svg` validates, resolves, and renders this schema in the
library. The `mtrd render -s` CLI is a thin file adapter around that API and
uses the same output naming options as topology rendering. It draws line
strokes, rounded explicit corners, circles, and oriented interchange capsules;
labels, legends, titles, and line marks remain deferred.
[`crates/mtrd/examples/schematic.yaml`](crates/mtrd/examples/schematic.yaml) is
a representative semantic manifest.
