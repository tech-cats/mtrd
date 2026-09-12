# mtrd developer tools

This private workspace package inspects intermediate stages of the `mtrd`
generation pipeline. It is not published and is excluded from the workspace's
default package set.

Generate a preprocessed YAML or JSON manifest from a topology manifest:

```sh
cargo run -p mtrd-devtools -- preprocess topology.yaml
```

This writes `topology.preprocessed.yaml`. An explicit output path may be given
as the second positional argument.

Render the saved preprocessed manifest as SVG:

```sh
cargo run -p mtrd-devtools -- render topology.preprocessed.yaml
```

This writes `topology.preprocessed.yaml.svg`. To preprocess and render in one
command, use `-r` or `--render`:

```sh
cargo run -p mtrd-devtools -- preprocess --render topology.yaml
```

Preprocessed manifests are unstable debugging artifacts rather than part of
the public manifest contract.
