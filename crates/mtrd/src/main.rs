use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{ArgAction, Parser, Subcommand, ValueEnum};
use thiserror::Error;

use mtrd::{
    MetroTopology, SchematicGenerationError, SchematicManifest, SchematicRenderError,
    TopologyRenderError, generate_schematic, render_schematic_svg, render_topology_svg,
    validate_schematic, validate_topology,
};

#[derive(Debug, Parser)]
#[command(name = "mtrd", version, about = "Work with metro manifests")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print an example manifest as YAML.
    Example {
        /// Manifest type: topology (t/topo) or schematic (s).
        #[arg(value_name = "TYPE")]
        kind: ExampleKind,
    },

    /// Convert a metro topology between YAML and JSON.
    #[command(alias = "conv")]
    Convert {
        /// Source .yaml, .yml, or .json file.
        input: PathBuf,

        /// Destination file; its extension selects the output format.
        output: PathBuf,
    },

    /// Check whether a metro manifest has valid syntax and schema.
    Check {
        /// Print canonical YAML (-v) or detailed debug output (-vv).
        #[arg(short = 'v', action = ArgAction::Count)]
        verbose: u8,

        /// Check a topology manifest.
        #[arg(
            short = 't',
            long,
            required_unless_present = "schematic",
            conflicts_with = "schematic"
        )]
        topology: bool,

        /// Check a schematic manifest.
        #[arg(
            short = 's',
            long,
            required_unless_present = "topology",
            conflicts_with = "topology"
        )]
        schematic: bool,

        /// Metro manifest file to check.
        input: PathBuf,
    },

    /// Render a metro manifest as SVG.
    #[command(alias = "r")]
    Render {
        /// Generate a topology graph.
        #[arg(
            short = 't',
            long,
            required_unless_present = "schematic",
            conflicts_with = "schematic"
        )]
        topology: bool,

        /// Generate a schematic map.
        #[arg(
            short = 's',
            long,
            required_unless_present = "topology",
            conflicts_with = "topology"
        )]
        schematic: bool,

        /// SVG destination (defaults to <input path>.svg).
        #[arg(short = 'o', long, value_name = "FILE", conflicts_with = "timestamp")]
        output: Option<PathBuf>,

        /// Name the output mtrd-<microsecond timestamp>.svg.
        #[arg(short = 'T', long, conflicts_with = "output")]
        timestamp: bool,

        /// Metro manifest file to render.
        input: PathBuf,
    },

    /// Generate a schematic manifest from a topology manifest.
    #[command(alias = "g", alias = "gen")]
    Generate {
        /// Topology .yaml, .yml, or .json input file.
        input: PathBuf,

        /// Schematic destination (defaults to <input stem>.schematic.yaml).
        #[arg(conflicts_with = "timestamp")]
        output: Option<PathBuf>,

        /// Name the output mtrd-<microsecond timestamp>.schematic.yaml.
        #[arg(short = 'T', long, conflicts_with = "output")]
        timestamp: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ExampleKind {
    #[value(alias = "t", alias = "topo")]
    Topology,
    #[value(alias = "s")]
    Schematic,
}

const TOPOLOGY_EXAMPLE: &str = include_str!("../examples/topology.yaml");
const SCHEMATIC_EXAMPLE: &str = include_str!("../examples/schematic.yaml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Yaml,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestKind {
    Topology,
    Schematic,
}

impl Format {
    fn from_path(path: &Path) -> Result<Self, CliError> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);

        match extension.as_deref() {
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

    #[error("input and output formats are both {0}")]
    SameFormat(&'static str),

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

    #[error("invalid YAML in '{path}': {source}")]
    ParseYaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("invalid JSON in '{path}': {source}")]
    ParseJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to serialize YAML: {0}")]
    SerializeYaml(#[source] serde_yaml::Error),

    #[error("failed to serialize JSON: {0}")]
    SerializeJson(#[source] serde_json::Error),

    #[error("failed to render topology: {0}")]
    RenderTopology(#[from] TopologyRenderError),

    #[error("invalid metro topology: {0}")]
    InvalidTopology(TopologyRenderError),

    #[error("failed to render schematic: {0}")]
    RenderSchematic(#[from] SchematicRenderError),

    #[error("invalid schematic manifest: {0}")]
    InvalidSchematic(SchematicRenderError),

    #[error(transparent)]
    Generate(#[from] SchematicGenerationError),

    #[error("failed to determine the current directory: {0}")]
    CurrentDirectory(#[source] std::io::Error),

    #[error("verbosity may be specified at most twice")]
    ExcessiveVerbosity,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(output) => {
            print_output(&output);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<String, CliError> {
    match cli.command {
        Command::Example { kind } => Ok(example(kind).to_owned()),
        Command::Convert { input, output } => {
            convert(&input, &output)?;
            Ok(format!("{} -> {}", input.display(), output.display()))
        }
        Command::Check {
            input,
            verbose,
            topology,
            schematic,
        } => {
            let kind = if topology {
                debug_assert!(!schematic);
                ManifestKind::Topology
            } else {
                debug_assert!(schematic);
                ManifestKind::Schematic
            };
            check(&input, verbose, kind)
        }
        Command::Render {
            topology,
            schematic,
            output,
            timestamp,
            input,
        } => {
            let kind = if topology {
                debug_assert!(!schematic);
                ManifestKind::Topology
            } else {
                debug_assert!(schematic);
                ManifestKind::Schematic
            };
            render(&input, output.as_deref(), timestamp, kind)
        }
        Command::Generate {
            input,
            output,
            timestamp,
        } => generate(&input, output.as_deref(), timestamp),
    }
}

fn example(kind: ExampleKind) -> &'static str {
    match kind {
        ExampleKind::Topology => TOPOLOGY_EXAMPLE,
        ExampleKind::Schematic => SCHEMATIC_EXAMPLE,
    }
}

fn generate(input: &Path, output: Option<&Path>, timestamp: bool) -> Result<String, CliError> {
    let format = Format::from_path(input)?;
    let topology = read_topology(input, format)?;
    let schematic = generate_schematic(&topology)?;
    let output = generation_output_path(input, output, timestamp)?;
    let yaml = schematic.to_yaml().map_err(CliError::SerializeYaml)?;
    fs::write(&output, yaml).map_err(|source| CliError::Write {
        path: output.clone(),
        source,
    })?;
    Ok(output.display().to_string())
}

fn generation_output_path(
    input: &Path,
    output: Option<&Path>,
    timestamp: bool,
) -> Result<PathBuf, CliError> {
    if let Some(output) = output {
        return Ok(output.to_path_buf());
    }
    if timestamp {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();
        return Ok(std::env::current_dir()
            .map_err(CliError::CurrentDirectory)?
            .join(format!("mtrd-{timestamp}.schematic.yaml")));
    }
    let mut output = input.to_path_buf();
    let stem = input.file_stem().unwrap_or_default();
    output.set_file_name(format!("{}.schematic.yaml", stem.to_string_lossy()));
    Ok(output)
}

fn render(
    input: &Path,
    output: Option<&Path>,
    timestamp: bool,
    kind: ManifestKind,
) -> Result<String, CliError> {
    match kind {
        ManifestKind::Topology => render_topology(input, output, timestamp),
        ManifestKind::Schematic => render_schematic(input, output, timestamp),
    }
}

fn render_topology(
    input: &Path,
    output: Option<&Path>,
    timestamp: bool,
) -> Result<String, CliError> {
    let format = Format::from_path(input)?;
    let topology = read_topology(input, format)?;
    let svg = render_topology_svg(&topology)?;
    let output = render_output_path(input, output, timestamp)?;

    fs::write(&output, svg).map_err(|source| CliError::Write {
        path: output.clone(),
        source,
    })?;
    Ok(output.display().to_string())
}

fn render_schematic(
    input: &Path,
    output: Option<&Path>,
    timestamp: bool,
) -> Result<String, CliError> {
    let format = Format::from_path(input)?;
    let schematic = read_schematic(input, format)?;
    let svg = render_schematic_svg(&schematic)?;
    let output = render_output_path(input, output, timestamp)?;

    fs::write(&output, svg).map_err(|source| CliError::Write {
        path: output.clone(),
        source,
    })?;
    Ok(output.display().to_string())
}

fn render_output_path(
    input: &Path,
    output: Option<&Path>,
    timestamp: bool,
) -> Result<PathBuf, CliError> {
    if let Some(output) = output {
        return Ok(output.to_path_buf());
    }
    if !timestamp {
        let mut output = input.as_os_str().to_owned();
        output.push(".svg");
        return Ok(output.into());
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    let directory = std::env::current_dir().map_err(CliError::CurrentDirectory)?;
    Ok(directory.join(format!("mtrd-{timestamp}.svg")))
}

fn convert(input: &Path, output: &Path) -> Result<(), CliError> {
    let input_format = Format::from_path(input)?;
    let output_format = Format::from_path(output)?;
    if input_format == output_format {
        return Err(CliError::SameFormat(match input_format {
            Format::Yaml => "YAML",
            Format::Json => "JSON",
        }));
    }

    let topology = read_topology(input, input_format)?;
    let mut encoded = serialize_topology(&topology, output_format)?;
    if !encoded.ends_with('\n') {
        encoded.push('\n');
    }

    fs::write(output, encoded).map_err(|source| CliError::Write {
        path: output.to_path_buf(),
        source,
    })
}

fn check(input: &Path, verbose: u8, kind: ManifestKind) -> Result<String, CliError> {
    let format = Format::from_path(input)?;

    match kind {
        ManifestKind::Topology => {
            let topology = read_topology(input, format)?;
            validate_topology(&topology).map_err(CliError::InvalidTopology)?;

            match verbose {
                0 => Ok(format!("{}: valid", input.display())),
                1 => topology.to_yaml().map_err(CliError::SerializeYaml),
                2 => Ok(format!("{topology:#?}")),
                _ => Err(CliError::ExcessiveVerbosity),
            }
        }
        ManifestKind::Schematic => {
            let schematic = read_schematic(input, format)?;
            validate_schematic(&schematic).map_err(CliError::InvalidSchematic)?;

            match verbose {
                0 => Ok(format!("{}: valid", input.display())),
                1 => schematic.to_yaml().map_err(CliError::SerializeYaml),
                2 => Ok(format!("{schematic:#?}")),
                _ => Err(CliError::ExcessiveVerbosity),
            }
        }
    }
}

fn read_topology(path: &Path, format: Format) -> Result<MetroTopology, CliError> {
    let contents = fs::read_to_string(path).map_err(|source| CliError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    match format {
        Format::Yaml => MetroTopology::from_yaml(&contents).map_err(|source| CliError::ParseYaml {
            path: path.to_path_buf(),
            source,
        }),
        Format::Json => MetroTopology::from_json(&contents).map_err(|source| CliError::ParseJson {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read_schematic(path: &Path, format: Format) -> Result<SchematicManifest, CliError> {
    let contents = fs::read_to_string(path).map_err(|source| CliError::Read {
        path: path.to_path_buf(),
        source,
    })?;

    match format {
        Format::Yaml => {
            SchematicManifest::from_yaml(&contents).map_err(|source| CliError::ParseYaml {
                path: path.to_path_buf(),
                source,
            })
        }
        Format::Json => {
            SchematicManifest::from_json(&contents).map_err(|source| CliError::ParseJson {
                path: path.to_path_buf(),
                source,
            })
        }
    }
}

fn serialize_topology(topology: &MetroTopology, format: Format) -> Result<String, CliError> {
    match format {
        Format::Yaml => topology.to_yaml().map_err(CliError::SerializeYaml),
        Format::Json => topology.to_json().map_err(CliError::SerializeJson),
    }
}

fn print_output(output: &str) {
    print!("{output}");
    if !output.ends_with('\n') {
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = r#"
options:
  background:
    color: '#ffffff'
  labels:
    hidden: false
  lines:
    width: 8.0
  stations:
    common:
      fill:
        diameter: 18.0
        color: { type: unified, value: '#ffffff' }
      stroke:
        width: 2.0
        alignment: center
        color: { type: follow-line }
    interchange:
      fill: { width: 18.0, color: '#ffffff' }
      stroke: { width: 2.0, alignment: outside, color: '#000000' }
stations:
  - id: central
    names:
      en:
        - Central
    position: [1.0, 2.0]
  - id: harbour
    names:
      en:
        - Harbour
    position: [3.0, 4.0]
lines:
  - id: red
    names:
      en:
        - Red Line
    color: '#ff0000'
    paths:
      - stations: [central, harbour]
        closed: false
"#;

    const SCHEMATIC_YAML: &str = r##"
options:
  background:
    color: "#ffffff"
  lines:
    width: 8.0
  stations:
    common:
      fill:
        diameter: 18.0
        color:
          type: unified
          value: "#ffffff"
      stroke:
        width: 2.0
        alignment: center
        color:
          type: follow-line
    interchange:
      fill:
        width: 18.0
        color: "#ffffff"
      stroke:
        width: 2.0
        alignment: outside
        color: "#000000"
stations:
  - id: central
    position: [1.0, 2.0]
    names:
      en:
        - Central
    symbol:
      type: circle
corners: []
lines:
  - id: red
    names:
      en:
        - Red Line
    color: "#ff0000"
    paths: []
"##;

    fn temporary_path(extension: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("mtrd-{}-{nonce}.{extension}", std::process::id()))
    }

    #[test]
    fn parses_cli_contract() {
        let cli = Cli::try_parse_from(["mtrd", "check", "-tvv", "topology.yaml"]).unwrap();

        assert!(matches!(
            cli.command,
            Command::Check {
                verbose: 2,
                topology: true,
                schematic: false,
                input
            } if input == Path::new("topology.yaml")
        ));

        let cli = Cli::try_parse_from(["mtrd", "check", "--schematic", "schematic.yaml"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Check {
                verbose: 0,
                topology: false,
                schematic: true,
                input
            } if input == Path::new("schematic.yaml")
        ));
    }

    #[test]
    fn accepts_every_example_type_alias() {
        for (value, expected) in [
            ("t", ExampleKind::Topology),
            ("topo", ExampleKind::Topology),
            ("topology", ExampleKind::Topology),
            ("s", ExampleKind::Schematic),
            ("schematic", ExampleKind::Schematic),
        ] {
            let cli = Cli::try_parse_from(["mtrd", "example", value]).unwrap();
            assert!(matches!(cli.command, Command::Example { kind } if kind == expected));
        }

        assert!(Cli::try_parse_from(["mtrd", "example", "unknown"]).is_err());
    }

    #[test]
    fn bundled_examples_are_canonical_manifests() {
        let topology = MetroTopology::from_yaml(example(ExampleKind::Topology)).unwrap();
        let schematic = SchematicManifest::from_yaml(example(ExampleKind::Schematic)).unwrap();

        assert_eq!(topology.to_yaml().unwrap(), example(ExampleKind::Topology));
        assert_eq!(
            schematic.to_yaml().unwrap(),
            example(ExampleKind::Schematic)
        );
    }

    #[test]
    fn parses_generate_command_and_aliases() {
        for command in ["generate", "g", "gen"] {
            let cli = Cli::try_parse_from(["mtrd", command, "topology.yaml"]).unwrap();
            assert!(matches!(
                cli.command,
                Command::Generate { input, output: None, timestamp: false }
                    if input == Path::new("topology.yaml")
            ));
        }
        assert!(
            Cli::try_parse_from(["mtrd", "generate", "topology.yaml", "schematic.yaml", "-T"])
                .is_err()
        );
    }

    #[test]
    fn generate_preprocesses_then_fails_without_creating_output() {
        let input = temporary_path("yaml");
        let output = temporary_path("schematic.yaml");
        fs::write(&input, YAML).unwrap();

        assert!(matches!(
            generate(&input, Some(&output), false),
            Err(CliError::Generate(
                SchematicGenerationError::StageUnavailable
            ))
        ));
        assert!(!output.exists());

        fs::remove_file(input).unwrap();
    }

    #[test]
    fn selects_generation_output_path() {
        assert_eq!(
            generation_output_path(Path::new("examples/simple.yaml"), None, false).unwrap(),
            Path::new("examples/simple.schematic.yaml")
        );
        assert_eq!(
            generation_output_path(
                Path::new("examples/simple.yaml"),
                Some(Path::new("chosen.yaml")),
                false,
            )
            .unwrap(),
            Path::new("chosen.yaml")
        );
    }

    #[test]
    fn prints_version_with_long_and_short_flags() {
        let expected = format!("mtrd {}\n", env!("CARGO_PKG_VERSION"));

        for flag in ["--version", "-V"] {
            let error = Cli::try_parse_from(["mtrd", flag]).unwrap_err();
            assert_eq!(error.kind(), clap::error::ErrorKind::DisplayVersion);
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn check_requires_exactly_one_manifest_kind() {
        assert!(Cli::try_parse_from(["mtrd", "check", "map.yaml"]).is_err());
        assert!(Cli::try_parse_from(["mtrd", "check", "-ts", "map.yaml"]).is_err());
    }

    #[test]
    fn parses_render_flags_separately_or_combined() {
        let combined =
            Cli::try_parse_from(["mtrd", "render", "-to", "topology.svg", "topology.yaml"])
                .unwrap();
        assert!(matches!(
            combined.command,
            Command::Render {
                topology: true,
                schematic: false,
                output: Some(output),
                timestamp: false,
                input,
            } if output == Path::new("topology.svg") && input == Path::new("topology.yaml")
        ));

        let separate = Cli::try_parse_from([
            "mtrd",
            "render",
            "-o",
            "topology.svg",
            "-t",
            "topology.yaml",
        ])
        .unwrap();
        assert!(matches!(
            separate.command,
            Command::Render {
                topology: true,
                schematic: false,
                output: Some(output),
                timestamp: false,
                input,
            } if output == Path::new("topology.svg") && input == Path::new("topology.yaml")
        ));

        let timestamped = Cli::try_parse_from(["mtrd", "render", "-tT", "topology.yaml"]).unwrap();
        assert!(matches!(
            timestamped.command,
            Command::Render {
                topology: true,
                schematic: false,
                output: None,
                timestamp: true,
                input,
            } if input == Path::new("topology.yaml")
        ));
    }

    #[test]
    fn render_requires_exactly_one_manifest_kind() {
        assert!(Cli::try_parse_from(["mtrd", "render", "topology.yaml"]).is_err());
        assert!(Cli::try_parse_from(["mtrd", "render", "-ts", "map.yaml"]).is_err());

        let schematic = Cli::try_parse_from(["mtrd", "render", "-s", "schematic.yaml"]).unwrap();
        assert!(matches!(
            schematic.command,
            Command::Render {
                topology: false,
                schematic: true,
                input,
                ..
            } if input == Path::new("schematic.yaml")
        ));
    }

    #[test]
    fn timestamp_and_explicit_output_conflict() {
        assert!(
            Cli::try_parse_from([
                "mtrd",
                "render",
                "-tT",
                "-o",
                "topology.svg",
                "topology.yaml"
            ])
            .is_err()
        );
    }

    #[test]
    fn selects_render_output_path() {
        let input = Path::new("examples/simple.yaml");

        assert_eq!(
            render_output_path(input, None, false).unwrap(),
            Path::new("examples/simple.yaml.svg")
        );
        assert_eq!(
            render_output_path(input, Some(Path::new("topology.svg")), false).unwrap(),
            Path::new("topology.svg")
        );

        let timestamped = render_output_path(input, None, true).unwrap();
        assert_eq!(
            timestamped.parent().unwrap(),
            std::env::current_dir().unwrap()
        );
        let filename = timestamped.file_name().unwrap().to_str().unwrap();
        let timestamp = filename
            .strip_prefix("mtrd-")
            .and_then(|filename| filename.strip_suffix(".svg"))
            .unwrap();
        assert!(timestamp.parse::<u128>().is_ok());
    }

    #[test]
    fn converts_yaml_to_json_and_json_to_yaml() {
        let yaml_path = temporary_path("yaml");
        let json_path = temporary_path("json");
        let round_trip_path = temporary_path("yml");
        fs::write(&yaml_path, YAML).unwrap();

        convert(&yaml_path, &json_path).unwrap();
        let json = fs::read_to_string(&json_path).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(json_value["stations"][0]["id"], "central");
        assert_eq!(json_value["options"]["coordinates"]["type"], "cartesian");
        assert_eq!(json_value["options"]["coordinates"]["axes"], "r-d");

        convert(&json_path, &round_trip_path).unwrap();
        let yaml = fs::read_to_string(&round_trip_path).unwrap();
        assert!(yaml.contains("position: [1.0, 2.0]"));
        assert!(yaml.contains("  coordinates:\n    type: cartesian\n    axes: r-d"));

        fs::remove_file(yaml_path).unwrap();
        fs::remove_file(json_path).unwrap();
        fs::remove_file(round_trip_path).unwrap();
    }

    #[test]
    fn check_verbosity_selects_yaml_then_debug() {
        let path = temporary_path("yaml");
        fs::write(&path, YAML).unwrap();

        assert!(
            check(&path, 0, ManifestKind::Topology)
                .unwrap()
                .ends_with(": valid")
        );
        assert!(
            check(&path, 1, ManifestKind::Topology)
                .unwrap()
                .starts_with("options:")
        );
        assert!(
            check(&path, 2, ManifestKind::Topology)
                .unwrap()
                .starts_with("MetroTopology {")
        );
        assert!(matches!(
            check(&path, 3, ManifestKind::Topology),
            Err(CliError::ExcessiveVerbosity)
        ));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn checks_schematic_yaml_and_json_with_verbosity() {
        let yaml_path = temporary_path("yaml");
        let json_path = temporary_path("json");
        fs::write(&yaml_path, SCHEMATIC_YAML).unwrap();
        let schematic = SchematicManifest::from_yaml(SCHEMATIC_YAML).unwrap();
        fs::write(&json_path, schematic.to_json().unwrap()).unwrap();

        assert!(
            check(&yaml_path, 0, ManifestKind::Schematic)
                .unwrap()
                .ends_with(": valid")
        );
        assert!(
            check(&yaml_path, 1, ManifestKind::Schematic)
                .unwrap()
                .starts_with("options:")
        );
        assert!(
            check(&json_path, 2, ManifestKind::Schematic)
                .unwrap()
                .starts_with("SchematicManifest {")
        );

        fs::remove_file(yaml_path).unwrap();
        fs::remove_file(json_path).unwrap();
    }

    #[test]
    fn schematic_check_rejects_unknown_fields() {
        let path = temporary_path("yaml");
        let yaml = SCHEMATIC_YAML.replace("    width: 8.0", "    width: 8.0\n    extra: true");
        fs::write(&path, yaml).unwrap();

        assert!(matches!(
            check(&path, 0, ManifestKind::Schematic),
            Err(CliError::ParseYaml { .. })
        ));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn check_rejects_topologies_that_cannot_render() {
        let path = temporary_path("yaml");
        let yaml = YAML.replace("[central, harbour]", "[central, missing]");
        fs::write(&path, yaml).unwrap();

        assert!(matches!(
            check(&path, 0, ManifestKind::Topology),
            Err(CliError::InvalidTopology(
                TopologyRenderError::UnknownStation { line, station }
            ))
                if line == "red" && station == "missing"
        ));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_same_or_unknown_formats() {
        assert!(matches!(
            convert(Path::new("topology.yaml"), Path::new("copy.yml")),
            Err(CliError::SameFormat("YAML"))
        ));
        assert!(matches!(
            Format::from_path(Path::new("topology.txt")),
            Err(CliError::UnsupportedFormat(_))
        ));
    }

    #[test]
    fn renders_svg_to_the_requested_file() {
        let input = temporary_path("yaml");
        let output = temporary_path("svg");
        fs::write(&input, YAML).unwrap();

        assert_eq!(
            render_topology(&input, Some(&output), false).unwrap(),
            output.display().to_string()
        );
        let svg = fs::read_to_string(&output).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("data-line-id=\"red\""));
        assert!(svg.contains("data-station-id=\"central\""));

        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
    }

    #[test]
    fn checks_and_renders_geographic_topologies() {
        let input = temporary_path("yaml");
        let output = temporary_path("svg");
        let yaml = YAML
            .replace(
                "  labels:",
                "  coordinates:\n    type: geographic\n    axes: n-w\n  labels:",
            )
            .replace("position: [1.0, 2.0]", "position: [34.0, 118.0]")
            .replace("position: [3.0, 4.0]", "position: [34.1, 117.9]");
        fs::write(&input, yaml).unwrap();

        assert!(
            check(&input, 0, ManifestKind::Topology)
                .unwrap()
                .ends_with(": valid")
        );
        render_topology(&input, Some(&output), false).unwrap();
        let svg = fs::read_to_string(&output).unwrap();
        assert!(svg.contains("data-line-id=\"red\""));
        assert!(svg.contains("stroke-width=\"8\""));

        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
    }

    #[test]
    fn renders_schematic_svg_to_the_requested_file() {
        let input = temporary_path("yaml");
        let output = temporary_path("svg");
        let yaml = SCHEMATIC_YAML.replace(
            "    paths: []",
            "    paths:\n      - visits:\n          - type: station\n            station-id: central\n            port:\n              type: single-line\n          - type: station\n            station-id: east\n            port:\n              type: single-line\n        closed: false",
        ).replace(
            "corners: []",
            "  - id: east\n    position: [41.0, 2.0]\n    names: { en: [East] }\n    symbol: { type: circle }\ncorners: []",
        );
        fs::write(&input, yaml).unwrap();

        assert_eq!(
            render_schematic(&input, Some(&output), false).unwrap(),
            output.display().to_string()
        );
        let svg = fs::read_to_string(&output).unwrap();
        assert!(svg.contains("<title>Metro schematic map</title>"));
        assert!(svg.contains("data-line-id=\"red\""));
        assert!(svg.contains("data-station-id=\"central\""));

        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
    }

    #[test]
    fn renders_next_to_the_input_by_default() {
        let input = temporary_path("yaml");
        let mut expected = input.as_os_str().to_owned();
        expected.push(".svg");
        let expected = PathBuf::from(expected);
        fs::write(&input, YAML).unwrap();

        assert_eq!(
            render_topology(&input, None, false).unwrap(),
            expected.display().to_string()
        );
        assert!(fs::read_to_string(&expected).unwrap().contains("<svg"));

        fs::remove_file(input).unwrap();
        fs::remove_file(expected).unwrap();
    }
}
