# mtrd developer tools

This private workspace package inspects intermediate stages of the `mtrd`
generation pipeline. It is not published and is excluded from the workspace's
default package set.

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
cargo run -p mtrd-devtools -- density --render topology.yaml
```

The first command writes `topology.density.yaml` (or equivalent JSON). The
`-r`/`--render` flag additionally writes `topology.density.yaml.svg`, showing
the density heatmap, source lines, and stations. An explicit output path may be
given after the input. The estimator and exact/factor overrides come from the
topology manifest's `options.density-reshape` block. These analysis artifacts
are debugging output, not stable manifest schemas.
