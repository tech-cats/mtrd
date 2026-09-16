mod contracted;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mtrd::MetroTopology;
use thiserror::Error;

use self::contracted::{ContractedManifestError, ContractedTopology};

#[derive(Debug, Parser)]
#[command(name = "mtrd-devtools", about = "Inspect the mtrd generation pipeline")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate a contracted topology manifest.
    Contract {
        /// Source topology manifest in YAML or JSON.
        input: PathBuf,

        /// Destination contracted manifest (defaults to <input stem>.contracted.<extension>).
        output: Option<PathBuf>,

        /// Also render the generated contracted manifest as SVG.
        #[arg(short, long)]
        render: bool,
    },

    /// Render a contracted topology manifest as SVG.
    Render {
        /// Contracted topology manifest in YAML or JSON.
        input: PathBuf,

        /// Destination SVG file (defaults to <input path>.svg).
        output: Option<PathBuf>,
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
    ContractedManifest(#[from] ContractedManifestError),
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(outputs) => {
            for output in outputs {
                println!("{}", output.display());
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<Vec<PathBuf>, CliError> {
    match cli.command {
        Command::Contract {
            input,
            output,
            render,
        } => contract(&input, output.as_deref(), render),
        Command::Render { input, output } => {
            render(&input, output.as_deref()).map(|output| vec![output])
        }
    }
}

fn contract(input: &Path, output: Option<&Path>, render: bool) -> Result<Vec<PathBuf>, CliError> {
    let input_format = Format::from_path(input)?;
    let output = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| contracted_output_path(input));
    let output_format = Format::from_path(&output)?;
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
    let topology = ContractedTopology::generate(topology)?;
    let manifest = match output_format {
        Format::Yaml => topology.to_yaml()?,
        Format::Json => topology.to_json()?,
    };
    write(&output, manifest)?;

    let mut outputs = vec![output.clone()];
    if render {
        let svg = render_output_path(&output);
        write(&svg, topology.render_svg()?)?;
        outputs.push(svg);
    }
    Ok(outputs)
}

fn render(input: &Path, output: Option<&Path>) -> Result<PathBuf, CliError> {
    let input_format = Format::from_path(input)?;
    let output = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| render_output_path(input));
    let manifest = read(input)?;
    let topology = match input_format {
        Format::Yaml => ContractedTopology::from_yaml(&manifest)?,
        Format::Json => ContractedTopology::from_json(&manifest)?,
    };
    write(&output, topology.render_svg()?)?;
    Ok(output)
}

fn contracted_output_path(input: &Path) -> PathBuf {
    let extension = input
        .extension()
        .expect("input format has already been determined")
        .to_os_string();
    let mut output = input.to_path_buf();
    output.set_extension("contracted");
    output.as_mut_os_string().push(".");
    output.as_mut_os_string().push(extension);
    output
}

fn render_output_path(input: &Path) -> PathBuf {
    let mut output = input.as_os_str().to_os_string();
    output.push(".svg");
    PathBuf::from(output)
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
    fn contracts_then_renders_saved_manifest() {
        let input = temporary_path("topology.yaml");
        let manifest = temporary_path("contracted.yaml");
        let svg = temporary_path("svg");
        fs::write(&input, TOPOLOGY_YAML).unwrap();

        contract(&input, Some(&manifest), false).unwrap();
        render(&manifest, Some(&svg)).unwrap();

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
            Cli::try_parse_from(["mtrd-devtools", "contract", "-r", "topology.yaml"])
                .unwrap()
                .command,
            Command::Contract {
                output: None,
                render: true,
                ..
            }
        ));
        assert!(matches!(
            Cli::try_parse_from(["mtrd-devtools", "render", "contracted.yaml"])
                .unwrap()
                .command,
            Command::Render { output: None, .. }
        ));
        assert!(Cli::try_parse_from(["mtrd-devtools", "preprocess", "topology.yaml"]).is_err());
    }

    #[test]
    fn derives_default_output_paths() {
        assert_eq!(
            contracted_output_path(Path::new("examples/topology.yaml")),
            Path::new("examples/topology.contracted.yaml")
        );
        assert_eq!(
            contracted_output_path(Path::new("examples/topology.json")),
            Path::new("examples/topology.contracted.json")
        );
        assert_eq!(
            render_output_path(Path::new("examples/topology.contracted.yaml")),
            Path::new("examples/topology.contracted.yaml.svg")
        );
    }

    #[test]
    fn contract_can_render_with_default_paths() {
        let input = temporary_path("yaml");
        fs::write(&input, TOPOLOGY_YAML).unwrap();

        let outputs = contract(&input, None, true).unwrap();
        let manifest = contracted_output_path(&input);
        let svg = render_output_path(&manifest);

        assert_eq!(outputs, [manifest.clone(), svg.clone()]);
        assert!(
            fs::read_to_string(&manifest)
                .unwrap()
                .contains("source_station_indices:")
        );
        assert!(fs::read_to_string(&svg).unwrap().contains("viewBox=\""));

        fs::remove_file(input).unwrap();
        fs::remove_file(manifest).unwrap();
        fs::remove_file(svg).unwrap();
    }
}
