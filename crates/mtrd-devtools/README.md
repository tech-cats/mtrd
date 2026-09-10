# mtrd developer tools

This private workspace package inspects intermediate stages of the `mtrd`
generation pipeline. It is not published and is excluded from the workspace's
default package set.

Generate a preprocessed YAML or JSON manifest from a topology manifest:

```sh
cargo run -p mtrd-devtools -- \
  preprocess topology.yaml tmp/topology.preprocessed.yaml
```

Render the saved preprocessed manifest as SVG:

```sh
cargo run -p mtrd-devtools -- \
  render tmp/topology.preprocessed.yaml tmp/topology.preprocessed.svg
```

Preprocessed manifests are unstable debugging artifacts rather than part of
the public manifest contract.
