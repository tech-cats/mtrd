# mtrd developer tools

This private workspace package inspects intermediate stages of the `mtrd`
generation pipeline. It is not published and is excluded from the workspace's
default package set.

Export the automatically derived generation settings for a topology as numeric
scalars:

```sh
cargo run -p mtrd-devtools -- derive topology.yaml
cargo run -p mtrd-devtools -- derive topology.yaml generation.yaml
```

The first command writes YAML to stdout. An output file with a `.yaml`, `.yml`,
or `.json` extension receives the corresponding format. The values are resolved
from the source topology, so the command requires an input path.

Generate a contracted YAML or JSON manifest from a topology manifest:

```sh
cargo run -p mtrd-devtools -- contract topology.yaml
```

This writes `topology.contracted.yaml`. An explicit output path may be given
as the second positional argument.

Render the saved contracted manifest as SVG:

```sh
cargo run -p mtrd-devtools -- render topology.contracted.yaml
```

This writes `topology.contracted.yaml.svg`. To contract and render in one
command, use `-r` or `--render`:

```sh
cargo run -p mtrd-devtools -- contract --render topology.yaml
```

Contracted manifests are unstable debugging artifacts rather than part of
the public manifest contract.

Inspect the pre-layout density mesh and triangle masses:

```sh
cargo run -p mtrd-devtools -- density topology.yaml
cargo run -p mtrd-devtools -- density --render -m generation.yaml topology.yaml
```

The first command writes `topology.density.yaml` (or equivalent JSON). The
`-r`/`--render` flag additionally writes `topology.density.yaml.svg`, showing
the density heatmap, source lines, and stations. An explicit output path may be
given after the input. The optional `-m`/`--manifest` flag reads a separate YAML
generation manifest for that invocation; without it, built-in defaults apply.
The `density-reshape` settings are defined by `mtrd::GenerationManifest`.
These analysis artifacts are debugging output, not stable manifest schemas.
