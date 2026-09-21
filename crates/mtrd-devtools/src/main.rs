mod contracted;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mtrd::{DensityAnalysis, DensityError, GenerationManifest, MetroTopology, analyze_density};
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
    /// Inspect the triangular density mesh of a topology.
    Density {
        /// Generation manifest in YAML (defaults to built-in settings).
        #[arg(short = 'm', long = "manifest", value_name = "FILE")]
        generation_manifest: Option<PathBuf>,
        /// Source topology manifest in YAML or JSON.
        input: PathBuf,
        /// Destination analysis manifest (defaults to <input stem>.density.<extension>).
        output: Option<PathBuf>,
        /// Also render a density heatmap SVG.
        #[arg(short, long)]
        render: bool,
    },
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

    #[error("invalid generation manifest YAML in '{path}': {source}")]
    GenerationManifestYaml {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error(transparent)]
    ContractedManifest(#[from] ContractedManifestError),

    #[error(transparent)]
    Density(#[from] DensityError),

    #[error("failed to serialise density analysis: {0}")]
    DensityYaml(serde_yaml::Error),

    #[error("failed to serialise density analysis: {0}")]
    DensityJson(serde_json::Error),
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
        Command::Density {
            input,
            output,
            render,
            generation_manifest,
        } => density(
            &input,
            output.as_deref(),
            render,
            generation_manifest.as_deref(),
        ),
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

fn density(
    input: &Path,
    output: Option<&Path>,
    render: bool,
    generation_path: Option<&Path>,
) -> Result<Vec<PathBuf>, CliError> {
    let input_format = Format::from_path(input)?;
    let output = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| density_output_path(input));
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
    let generation = match generation_path {
        Some(path) => GenerationManifest::from_yaml(&read(path)?).map_err(|source| {
            CliError::GenerationManifestYaml {
                path: path.to_path_buf(),
                source,
            }
        })?,
        None => GenerationManifest::default(),
    };
    let analysis = analyze_density(&topology, &generation)?;
    let data = match output_format {
        Format::Yaml => serde_yaml::to_string(&analysis).map_err(CliError::DensityYaml)?,
        Format::Json => serde_json::to_string_pretty(&analysis).map_err(CliError::DensityJson)?,
    };
    write(&output, data)?;
    let mut outputs = vec![output.clone()];
    if render {
        let svg = render_output_path(&output);
        write(&svg, density_svg(&analysis))?;
        outputs.push(svg);
    }
    Ok(outputs)
}

fn density_svg(analysis: &DensityAnalysis) -> String {
    use std::fmt::Write as _;

    let [min_x, min_y, max_x, max_y] = analysis.bounds;
    let width = max_x - min_x;
    let height = max_y - min_y;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{min_x} {min_y} {width} {height}\">\n<rect x=\"{min_x}\" y=\"{min_y}\" width=\"{width}\" height=\"{height}\" fill=\"white\"/>\n"
    );
    let maximum = analysis
        .triangles
        .iter()
        .map(|triangle| triangle.mass / triangle.area)
        .fold(0.0_f64, f64::max);
    for triangle in &analysis.triangles {
        let [a, b, c] = triangle.vertices.map(|index| analysis.vertices[index]);
        let intensity = ((triangle.mass / triangle.area / maximum).sqrt() * 255.0).round() as u8;
        let red = 245_u8;
        let green = 245_u8.saturating_sub((u16::from(intensity) * 3 / 4) as u8);
        let blue = 245_u8.saturating_sub(intensity);
        writeln!(svg, "<polygon points=\"{},{} {},{} {},{}\" fill=\"#{red:02x}{green:02x}{blue:02x}\" stroke=\"#999999\" stroke-width=\"{}\"/>", a.x,a.y,b.x,b.y,c.x,c.y,width.max(height)/4000.0).unwrap();
    }
    for &[a, b] in &analysis.segments {
        writeln!(
            svg,
            "<path d=\"M {} {} L {} {}\" fill=\"none\" stroke=\"#263238\" stroke-width=\"{}\"/>",
            a.x,
            a.y,
            b.x,
            b.y,
            width.max(height) / 700.0
        )
        .unwrap();
    }
    for station in &analysis.stations {
        writeln!(svg, "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"#ffffff\" stroke=\"#263238\" stroke-width=\"{}\"/>",station.x,station.y,width.max(height)/450.0,width.max(height)/1500.0).unwrap();
    }
    svg.push_str("</svg>\n");
    svg
}

fn density_output_path(input: &Path) -> PathBuf {
    let extension = input
        .extension()
        .expect("input format has already been determined")
        .to_os_string();
    let mut output = input.to_path_buf();
    output.set_extension("density");
    output.as_mut_os_string().push(".");
    output.as_mut_os_string().push(extension);
    output
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
  languages: { set: [en], primary: en }
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
    names: { en: [Test] }
    position: [0, 0]
  - id: B
    names: { en: [Test] }
    position: [1, 1]
  - id: C
    names: { en: [Test] }
    position: [2, 0]
lines:
  - id: red
    names: { en: [Test] }
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
    fn density_writes_data_by_default_and_svg_only_when_requested() {
        let input = temporary_path("density-input.yaml");
        let output = temporary_path("density-output.json");
        let svg = render_output_path(&output);
        fs::write(&input, TOPOLOGY_YAML).unwrap();

        assert_eq!(
            density(&input, Some(&output), false, None).unwrap(),
            vec![output.clone()]
        );
        let data = fs::read_to_string(&output).unwrap();
        assert!(data.contains("\"triangles\""));
        assert!(!svg.exists());

        assert_eq!(
            density(&input, Some(&output), true, None).unwrap(),
            vec![output.clone(), svg.clone()]
        );
        let visual = fs::read_to_string(&svg).unwrap();
        assert!(visual.contains("<polygon"));
        assert!(visual.contains("<circle"));

        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
        fs::remove_file(svg).unwrap();
    }

    #[test]
    fn parses_commands() {
        for flag in ["-m", "--manifest"] {
            let cli = Cli::try_parse_from([
                "mtrd-devtools",
                "density",
                flag,
                "generation.yaml",
                "topology.yaml",
            ])
            .unwrap();
            assert!(
                matches!(cli.command, Command::Density { generation_manifest: Some(path), .. } if path == Path::new("generation.yaml"))
            );
        }
        for old_flag in ["-c", "--config"] {
            assert!(
                Cli::try_parse_from([
                    "mtrd-devtools",
                    "density",
                    old_flag,
                    "generation.yaml",
                    "topology.yaml",
                ])
                .is_err()
            );
        }
        assert!(matches!(
            Cli::try_parse_from(["mtrd-devtools", "density", "-r", "topology.yaml"])
                .unwrap()
                .command,
            Command::Density {
                output: None,
                render: true,
                ..
            }
        ));
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
    fn density_uses_separate_config_and_reports_invalid_yaml() {
        let input = temporary_path("config-input.yaml");
        let output = temporary_path("config-output.yaml");
        let config = temporary_path("generation.yaml");
        fs::write(&input, TOPOLOGY_YAML).unwrap();
        fs::write(&config, "density-reshape:\n  bandwidth: { exact: 10.0 }\n").unwrap();
        density(&input, Some(&output), false, Some(&config)).unwrap();
        let analysis: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
        assert_eq!(analysis["options"]["bandwidth"].as_f64(), Some(10.0));
        density(&input, Some(&output), false, None).unwrap();
        let defaults: serde_yaml::Value =
            serde_yaml::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
        assert_ne!(defaults["options"]["bandwidth"].as_f64(), Some(10.0));
        fs::write(&config, "density-reshape: {unknown: true}\n").unwrap();
        assert!(matches!(
            density(&input, Some(&output), false, Some(&config)),
            Err(CliError::GenerationManifestYaml { .. })
        ));
        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
        fs::remove_file(config).unwrap();
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
