# Repository Guidelines

## Project Structure & Module Organization

This Rust 2024 workspace contains `crates/mtrd`. `src/lib.rs` is the public facade; topology and schematic models live in `src/topology.rs` and `src/schematic.rs`, with responsibility-specific children under `src/topology/` and `src/schematic/`. `src/main.rs` is the thin `mtrd` CLI adapter. Tests are colocated under `#[cfg(test)]`.

`README.md` documents the CLI and manifest contract. `docs/` contains design notes, including proposed schematic-map APIs that are not yet implemented, while `images/` holds documentation graphics. `target/`, `tmp/`, and `examples/` are ignored build, scratch, and output paths.

Files named `v1.md`, `v2.md`, and so on inside a design-note directory are
revisions of that document. Their numbers are unrelated to the Cargo
workspace/package version and must not be used to infer a release version.

## Build, Test, and Development Commands

- `cargo build --workspace` builds the library and `mtrd` binary.
- `cargo run -p metro-draw -- --help` runs the CLI from source.
- `cargo run -p metro-draw -- check path/to/map.yaml` validates schema and renderability.
- `cargo test --workspace` runs all colocated unit tests.
- `cargo fmt --all -- --check` verifies formatting without changing files.
- `cargo clippy --workspace --all-targets -- -D warnings` treats every lint warning as an error.
- `git diff --check` catches trailing whitespace and malformed patches.

Run `cargo fmt --all` before the check-only commands when code has changed.

## Coding Style & Naming Conventions

Use standard rustfmt output and four-space indentation. Follow Rust naming conventions: `snake_case` for modules, functions, and tests; `PascalCase` for types and error variants; `SCREAMING_SNAKE_CASE` for constants. Order Rust declarations and imports as: `mod` declarations, `use std::…`, dependency imports, `use crate::…`, `use super::…`, then `use self::…` (omitting `self::` when idiomatic). Keep each import group separated by a blank line. Prefer typed `thiserror` errors and preserve their sources. Keep schema, validation, and rendering behavior in the library; CLI code should handle arguments, files, and user-facing output only.

Use North American English for code identifiers and for the canonical spelling of public interface tokens such as subcommands, flags, configuration keys, and serialised field or variant names. Prefer British English in documentation and other prose. Where practical, accept British spellings as aliases for public interface tokens, while emitting and documenting the North American spelling as canonical.

YAML is the primary human-edited format and JSON is equivalent interchange. Keep Serde schemas strict. Positions must remain two-element `[x, y]` arrays, and `names[locale][0]` is the canonical label.

## Testing Guidelines

Name tests after behavior, such as `rejects_same_or_unknown_formats`. Add regression tests beside the affected module for successful round trips, exact CLI contracts, validation errors, and SVG structure. No numeric coverage threshold is configured, but every behavior change should exercise both success and failure paths where relevant.

## Compatibility Policy

Breaking changes are always acceptable until the project reaches version `1.0.0`.

## Commit & Pull Request Guidelines

Recent history uses Conventional Commit-style subjects such as `feat(render): ...`, `fix(render): ...`, `refactor(core): ...`, `docs: ...`, and `chore: ...`. Keep each commit focused and its subject imperative.

Pull requests should explain the behavior and manifest impact, list validation commands run, and link relevant issues. Update `README.md` when CLI or schema contracts change. For rendering changes, include a representative before/after SVG or screenshot.
