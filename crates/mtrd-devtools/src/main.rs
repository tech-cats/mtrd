mod preprocessed;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mtrd::{MetroTopology, TopologyPreprocessError};
use thiserror::Error;

use self::preprocessed::{PreprocessedManifestError, PreprocessedTopology};

#[derive(Debug, Parser)]
#[command(name = "mtrd-devtools", about = "Inspect the mtrd generation pipeline")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate a preprocessed topology manifest.
    Preprocess {
        /// Source topology manifest in YAML or JSON.
        input: PathBuf,

        /// Destination preprocessed manifest in YAML or JSON.
        output: PathBuf,
    },

    /// Render a preprocessed topology manifest as SVG.
    Render {
        /// Preprocessed topology manifest in YAML or JSON.
        input: PathBuf,

        /// Destination SVG file.
        output: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Yaml,
    Json,
}

impl Format {
    fn from_path(path: &Path) -> Result<Self, CliError> {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("yaml" | "yml") => Ok(Self::Yaml),
            Some("json") => Ok(Self::Json),
            _ => Err(CliError::UnsupportedFormat(path.to_path_buf())),
        }
    }
}

#[derive(Debug, Error)]
enum CliError {
    #[error("cannot determine format for '{0}'; use a .yaml, .yml, or .json extension")]
    UnsupportedFormat(PathBuf),

    #[error("failed to read '{path}': {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write '{path}': {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid topology YAML in '{path}': {source}")]
    TopologyYaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("invalid topology JSON in '{path}': {source}")]
    TopologyJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error(transparent)]
    Preprocess(#[from] TopologyPreprocessError),

    #[error(transparent)]
    PreprocessedManifest(#[from] PreprocessedManifestError),
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(output) => {
            println!("{}", output.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<PathBuf, CliError> {
    match cli.command {
        Command::Preprocess { input, output } => preprocess(&input, &output),
        Command::Render { input, output } => render(&input, &output),
    }
}

fn preprocess(input: &Path, output: &Path) -> Result<PathBuf, CliError> {
    let input_format = Format::from_path(input)?;
    let output_format = Format::from_path(output)?;
    let source = read(input)?;
    let topology = match input_format {
        Format::Yaml => {
            MetroTopology::from_yaml(&source).map_err(|source| CliError::TopologyYaml {
                path: input.to_path_buf(),
                source,
            })?
        }
        Format::Json => {
            MetroTopology::from_json(&source).map_err(|source| CliError::TopologyJson {
                path: input.to_path_buf(),
                source,
            })?
        }
    };
    let topology = PreprocessedTopology::generate(topology)?;
    let manifest = match output_format {
        Format::Yaml => topology.to_yaml()?,
        Format::Json => topology.to_json()?,
    };
    write(output, manifest)?;
    Ok(output.to_path_buf())
}

fn render(input: &Path, output: &Path) -> Result<PathBuf, CliError> {
    let input_format = Format::from_path(input)?;
    let manifest = read(input)?;
    let topology = match input_format {
        Format::Yaml => PreprocessedTopology::from_yaml(&manifest)?,
        Format::Json => PreprocessedTopology::from_json(&manifest)?,
    };
    write(output, topology.render_svg())?;
    Ok(output.to_path_buf())
}

fn read(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|source| CliError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn write(path: &Path, contents: String) -> Result<(), CliError> {
    fs::write(path, contents).map_err(|source| CliError::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOPOLOGY_YAML: &str = r##"
options:
  background: { transparent: true }
  labels: { hidden: false }
  lines: { width: 8.0 }
  stations:
    common:
      fill: { diameter: 18.0, color: { type: unified, value: "#ffffff" } }
      stroke: { width: 2.0, alignment: center, color: { type: follow-line } }
    interchange:
      fill: { width: 18.0, color: "#ffffff" }
      stroke: { width: 2.0, alignment: outside, color: "#000000" }
stations:
  - id: A
    names: {}
    position: [0, 0]
  - id: B
    names: {}
    position: [1, 1]
  - id: C
    names: {}
    position: [2, 0]
lines:
  - id: red
    names: {}
    color: "#f00"
    paths:
      - stations: [A, B, C]
        closed: false
"##;

    fn temporary_path(extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mtrd-devtools-{}-{}.{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test"),
            extension
        ))
    }

    #[test]
    fn preprocesses_then_renders_saved_manifest() {
        let input = temporary_path("topology.yaml");
        let manifest = temporary_path("preprocessed.yaml");
        let svg = temporary_path("svg");
        fs::write(&input, TOPOLOGY_YAML).unwrap();

        preprocess(&input, &manifest).unwrap();
        render(&manifest, &svg).unwrap();

        let manifest_contents = fs::read_to_string(&manifest).unwrap();
        let svg_contents = fs::read_to_string(&svg).unwrap();
        assert!(manifest_contents.contains("source_station_indices:"));
        assert!(svg_contents.contains("data-contracted-station=\"1\""));
        assert!(svg_contents.contains("viewBox=\""));

        fs::remove_file(input).unwrap();
        fs::remove_file(manifest).unwrap();
        fs::remove_file(svg).unwrap();
    }

    #[test]
    fn parses_commands() {
        assert!(matches!(
            Cli::try_parse_from([
                "mtrd-devtools",
                "preprocess",
                "topology.yaml",
                "preprocessed.yaml"
            ])
            .unwrap()
            .command,
            Command::Preprocess { .. }
        ));
        assert!(matches!(
            Cli::try_parse_from([
                "mtrd-devtools",
                "render",
                "preprocessed.yaml",
                "preprocessed.svg"
            ])
            .unwrap()
            .command,
            Command::Render { .. }
        ));
    }
}
