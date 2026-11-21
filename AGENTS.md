# Repository Guidelines

## Project Structure & Module Organization

This Rust 2024 workspace has two crates.

- `crates/mtrd` is the default workspace member. Its `src/lib.rs` is the public facade, and `src/main.rs` is the thin `mtrd` CLI adapter. The manifest models live in `src/topology.rs`, `src/schematic.rs`, and `src/generation.rs`. Topology and schematic implementation details live in their respective submodules.
- `crates/mtrd-devtools` is the unpublished crate for developing and testing features outside the released CLI. Use it to experiment with pipeline stages, generate intermediate files, and inspect them with debug SVGs. It depends on `mtrd` and is excluded from the default workspace members. Its current commands are `derive`, `density`, `warp`, `contract`, and `render`. Intermediate artefacts may change freely and carry no public compatibility guarantee.

`README.md` documents the public CLI and manifest contracts. `crates/mtrd-devtools/README.md` covers development workflows. `docs/` describes the implemented schematic manifest and renderer, topology contracting, and remaining generation work. The library implements density analysis, warping, and contraction. Automatic topology-to-schematic layout is unfinished, and the public CLI has no `generate` command.

The root `target`, `tmp/`, and `examples/` paths are ignored. The files in `crates/mtrd/examples/` are tracked sample manifests. Tests are colocated under `#[cfg(test)]` in both crates.

Files named `v1.md`, `v2.md`, and so on inside a design-note directory are
revisions of that document. Their numbers are unrelated to the Cargo
workspace/package version and must not be used to infer a release version.

## Build, Test, and Development Commands

- `cargo build --workspace` builds both crates.
- `cargo run -p mtrd -- --help` shows the public CLI commands.
- `cargo run -p mtrd -- check --topology crates/mtrd/examples/topology.yaml` checks a topology manifest's schema and renderability. Use `--schematic` with a schematic manifest.
- `cargo run -p mtrd-devtools -- --help` shows the development commands.
- `cargo run -p mtrd-devtools -- derive crates/mtrd/examples/topology.yaml` prints the resolved generation defaults as YAML.
- `cargo test --workspace` runs tests for both crates. Use `cargo test -p mtrd-devtools` for the development crate alone.
- `cargo fmt --all -- --check` verifies formatting without changing files.
- `cargo clippy --workspace --all-targets -- -D warnings` treats every lint warning as an error.
- `git diff --check` catches trailing whitespace and malformed patches.

Run `cargo fmt --all` before the check-only commands when code has changed.

## Coding Style & Naming Conventions

Use standard rustfmt output and four-space indentation. Use `snake_case` for modules, functions, and tests, `PascalCase` for types and error variants, and `SCREAMING_SNAKE_CASE` for constants. Order Rust declarations and imports with `mod` declarations first, followed by `use std::…`, dependency imports, `use crate::…`, `use super::…`, and `use self::…` (omitting `self::` when idiomatic). Keep each import group separated by a blank line. Prefer typed `thiserror` errors and preserve their sources.

Keep public schemas, validation, map rendering, and shared pipeline logic in the `mtrd` library. The public CLI handles arguments, files, and user-facing output. Keep experiments specific to development, intermediate artefact handling, and debug rendering in `mtrd-devtools`.

Use North American English for code identifiers and for the canonical spelling of public interface tokens such as subcommands, flags, configuration keys, and serialised field or variant names. Prefer British English in documentation and other prose. Where practical, accept British spellings as aliases for public interface tokens, while emitting and documenting the North American spelling as canonical.

YAML is the primary human-edited format and JSON is equivalent interchange. Keep Serde schemas strict. Positions must remain two-element `[x, y]` arrays, and `names[locale][0]` is the canonical label.

## Testing Guidelines

Name tests after behaviour, such as `rejects_same_or_unknown_formats`. Add regression tests beside the affected module for successful round trips, exact CLI contracts, validation errors, and SVG structure. No numeric coverage threshold is configured, but every behaviour change should exercise both success and failure paths where relevant.

## Compatibility Policy

Breaking changes are always acceptable until the project reaches version `1.0.0`.

## Commit & Pull Request Guidelines

Recent history uses Conventional Commit-style subjects such as `feat(render): ...`, `fix(render): ...`, `refactor(core): ...`, `docs: ...`, and `chore: ...`. Keep each commit focused and its subject imperative.

Pull requests should explain the behaviour and manifest impact, list validation commands run, and link relevant issues. Update the root `README.md` when public CLI or schema contracts change, and the devtools README when development workflows change. For rendering changes, include a representative before/after SVG or screenshot.
