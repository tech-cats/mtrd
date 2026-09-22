mod contracted;

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use mtrd::{
    DensityAnalysis, DensityError, DensityWarpAnalysis, GenerationManifest, MetroTopology,
    analyze_density, analyze_density_warp,
};
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
    /// Export default generation settings resolved from a topology.
    Derive {
        /// Source topology manifest in YAML or JSON.
        input: PathBuf,
        /// Destination YAML or JSON file (defaults to stdout as YAML).
        output: Option<PathBuf>,
    },
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
    /// Inspect and optionally render the pre-layout density warp.
    Warp {
        /// Generation manifest in YAML (defaults to built-in settings).
        #[arg(short = 'm', long = "manifest", value_name = "FILE")]
        generation_manifest: Option<PathBuf>,
        /// Source topology manifest in YAML or JSON.
        input: PathBuf,
        /// Destination warp analysis (defaults to <input stem>.warp.<extension>).
        output: Option<PathBuf>,
        /// Also render the deformed mesh and network as SVG.
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

    #[error("failed to write to stdout: {0}")]
    Stdout(#[source] std::io::Error),

    #[error("failed to serialise generation manifest as YAML: {0}")]
    GenerationYaml(serde_yaml::Error),

    #[error("failed to serialise generation manifest as JSON: {0}")]
    GenerationJson(serde_json::Error),

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
    #[error("failed to serialise density warp analysis: {0}")]
    WarpYaml(serde_yaml::Error),
    #[error("failed to serialise density warp analysis: {0}")]
    WarpJson(serde_json::Error),
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
        Command::Derive { input, output } => derive(&input, output.as_deref()),
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
        Command::Warp {
            input,
            output,
            render,
            generation_manifest,
        } => warp(
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

fn derive(input: &Path, output: Option<&Path>) -> Result<Vec<PathBuf>, CliError> {
    let input_format = Format::from_path(input)?;
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
    let resolved = analyze_density(&topology, &GenerationManifest::default())?.options;
    let mut manifest = GenerationManifest::default();
    let density = &mut manifest.density_reshape;
    density.estimator = resolved.estimator;
    density.bandwidth = Some(resolved.bandwidth);
    density.mesh_cell_size = Some(resolved.mesh_cell_size);
    density.raster_pixel_size = Some(resolved.raster_pixel_size);
    density.padding = Some(resolved.padding);
    density.station_weight = Some(resolved.station_weight);
    density.segment_weight = Some(resolved.segment_weight);
    density.density_floor = Some(resolved.density_floor);
    let format = output
        .map(Format::from_path)
        .transpose()?
        .unwrap_or(Format::Yaml);
    let contents = match format {
        Format::Yaml => manifest.to_yaml().map_err(CliError::GenerationYaml)?,
        Format::Json => format!(
            "{}\n",
            serde_json::to_string_pretty(&manifest).map_err(CliError::GenerationJson)?
        ),
    };
    match output {
        Some(path) => {
            write(path, contents)?;
            Ok(vec![path.to_path_buf()])
        }
        None => {
            io::stdout()
                .write_all(contents.as_bytes())
                .map_err(CliError::Stdout)?;
            Ok(Vec::new())
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

    const MAX_DISPLAY_SIZE: f64 = 1200.0;

    let [min_x, min_y, max_x, max_y] = analysis.bounds;
    let width = max_x - min_x;
    let height = max_y - min_y;
    let display_scale = MAX_DISPLAY_SIZE / width.max(height);
    let display_width = width * display_scale;
    let display_height = height * display_scale;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{min_x} {min_y} {width} {height}\" width=\"{display_width}\" height=\"{display_height}\" role=\"img\">\n<rect x=\"{min_x}\" y=\"{min_y}\" width=\"{width}\" height=\"{height}\" fill=\"white\"/>\n"
    );
    let maximum = analysis
        .triangles
        .iter()
        .map(|triangle| triangle.mass / triangle.area)
        .fold(0.0_f64, f64::max);
    for triangle in &analysis.triangles {
        let [a, b, c] = triangle.vertices.map(|index| analysis.vertices[index]);
        let intensity = ((triangle.mass / triangle.area / maximum).sqrt() * 255.0).round() as u8;
        let red = 255 - intensity;
        let green = 255 - intensity;
        let blue = 255_u8;
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

fn warp(
    input: &Path,
    output: Option<&Path>,
    render: bool,
    generation_path: Option<&Path>,
) -> Result<Vec<PathBuf>, CliError> {
    let input_format = Format::from_path(input)?;
    let output = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| warp_output_path(input));
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
    let analysis = analyze_density_warp(&topology, &generation)?;
    let data = match output_format {
        Format::Yaml => serde_yaml::to_string(&analysis).map_err(CliError::WarpYaml)?,
        Format::Json => serde_json::to_string_pretty(&analysis).map_err(CliError::WarpJson)?,
    };
    write(&output, data)?;
    let mut outputs = vec![output.clone()];
    if render {
        let svg = render_output_path(&output);
        write(&svg, warp_svg(&analysis))?;
        outputs.push(svg);
    }
    Ok(outputs)
}

fn warp_output_path(input: &Path) -> PathBuf {
    let extension = input
        .extension()
        .expect("input format has already been determined")
        .to_os_string();
    let mut output = input.to_path_buf();
    output.set_extension("warp");
    output.as_mut_os_string().push(".");
    output.as_mut_os_string().push(extension);
    output
}

fn warp_svg(analysis: &DensityWarpAnalysis) -> String {
    const MAX_DISPLAY_SIZE: f64 = 1200.0;
    let [min_x, min_y, max_x, max_y] = analysis.bounds;
    let width = max_x - min_x;
    let height = max_y - min_y;
    let display_scale = MAX_DISPLAY_SIZE / width.max(height);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{min_x} {min_y} {width} {height}\" width=\"{}\" height=\"{}\" role=\"img\">\n<title>Density-warped topology (pre-layout)</title>\n<rect x=\"{min_x}\" y=\"{min_y}\" width=\"{width}\" height=\"{height}\" fill=\"white\"/>\n",
        width * display_scale,
        height * display_scale,
    );
    let mut seen = HashSet::new();
    write!(
        svg,
        "<path class=\"warp-mesh\" fill=\"none\" stroke=\"#cbd5e1\" stroke-width=\"{}\" d=\"",
        width.max(height) / 3000.0
    )
    .unwrap();
    for &triangle in &analysis.triangles {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let edge = (a.min(b), a.max(b));
            if seen.insert(edge) {
                let first = analysis.warped_vertices[a];
                let second = analysis.warped_vertices[b];
                write!(svg, "M{} {}L{} {}", first.x, first.y, second.x, second.y).unwrap();
            }
        }
    }
    svg.push_str("\"/>\n");
    for segment in &analysis.segments {
        write!(
            svg,
            "<polyline class=\"warped-segment\" fill=\"none\" stroke=\"#263238\" stroke-linecap=\"round\" stroke-linejoin=\"round\" stroke-width=\"{}\" points=\"",
            width.max(height) / 700.0
        )
        .unwrap();
        for point in &segment.points {
            write!(svg, "{},{} ", point.x, point.y).unwrap();
        }
        svg.push_str("\"/>\n");
    }
    for station in &analysis.stations {
        writeln!(
            svg,
            "<circle class=\"warped-station\" data-station-id=\"{}\" cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"white\" stroke=\"#263238\" stroke-width=\"{}\"/>",
            xml_escape(&station.id),
            station.warped.x,
            station.warped.y,
            width.max(height) / 450.0,
            width.max(height) / 1500.0,
        )
        .unwrap();
    }
    svg.push_str("</svg>\n");
    svg
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
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
        let root = visual.lines().next().unwrap();
        let dimension = |name: &str| -> f64 {
            root.split_once(&format!(" {name}=\""))
                .unwrap()
                .1
                .split_once('"')
                .unwrap()
                .0
                .parse()
                .unwrap()
        };
        let display_width = dimension("width");
        let display_height = dimension("height");
        assert_eq!(display_width.max(display_height), 1200.0);
        let topology = MetroTopology::from_yaml(TOPOLOGY_YAML).unwrap();
        let bounds = analyze_density(&topology, &GenerationManifest::default())
            .unwrap()
            .bounds;
        let aspect_ratio = (bounds[2] - bounds[0]) / (bounds[3] - bounds[1]);
        assert!((display_width / display_height - aspect_ratio).abs() < 1e-10);

        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
        fs::remove_file(svg).unwrap();
    }

    #[test]
    fn warp_exports_both_methods_and_renders_deformed_network() {
        let input = temporary_path("warp-input.yaml");
        let output = temporary_path("warp-output.json");
        let config = temporary_path("warp-generation.yaml");
        let svg = render_output_path(&output);
        fs::write(&input, TOPOLOGY_YAML).unwrap();
        fs::write(
            &config,
            "density-reshape: {method: triangle-area, equalization-strength: 0.5}\n",
        )
        .unwrap();
        assert_eq!(
            warp(&input, Some(&output), true, Some(&config)).unwrap(),
            [output.clone(), svg.clone()]
        );
        let data: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
        assert_eq!(data["method"], "triangle-area");
        assert_eq!(data["equalization-strength"], 0.5);
        assert!(
            data["stations"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        let visual = fs::read_to_string(&svg).unwrap();
        assert!(visual.contains("class=\"warp-mesh\""));
        assert!(visual.contains("class=\"warped-segment\""));
        assert!(visual.contains("class=\"warped-station\""));
        assert!(visual.contains("viewBox=\""));

        fs::write(&config, "density-reshape: {method: diffusion}\n").unwrap();
        warp(&input, Some(&output), false, Some(&config)).unwrap();
        let data: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
        assert_eq!(data["method"], "diffusion");

        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
        fs::remove_file(config).unwrap();
        fs::remove_file(svg).unwrap();
    }

    #[test]
    fn parses_commands() {
        assert!(matches!(
            Cli::try_parse_from(["mtrd-devtools", "derive", "topology.yaml"])
                .unwrap()
                .command,
            Command::Derive { input, output: None } if input == Path::new("topology.yaml")
        ));
        assert!(matches!(
            Cli::try_parse_from(["mtrd-devtools", "derive", "topology.yaml", "generation.json"])
                .unwrap()
                .command,
            Command::Derive { input, output: Some(output) }
                if input == Path::new("topology.yaml") && output == Path::new("generation.json")
        ));
        assert!(Cli::try_parse_from(["mtrd-devtools", "derive"]).is_err());
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
            Cli::try_parse_from([
                "mtrd-devtools",
                "warp",
                "-m",
                "generation.yaml",
                "-r",
                "topology.yaml",
            ])
            .unwrap()
            .command,
            Command::Warp {
                output: None,
                render: true,
                generation_manifest: Some(_),
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
    fn derive_writes_resolved_scalars_in_yaml_and_json() {
        let input = temporary_path("derive-input.yaml");
        let yaml = temporary_path("derived.yaml");
        let json = temporary_path("derived.json");
        fs::write(&input, TOPOLOGY_YAML).unwrap();

        assert_eq!(
            derive(&input, Some(&yaml)).unwrap().as_slice(),
            std::slice::from_ref(&yaml)
        );
        assert_eq!(
            derive(&input, Some(&json)).unwrap().as_slice(),
            std::slice::from_ref(&json)
        );
        let yaml_contents = fs::read_to_string(&yaml).unwrap();
        let json_contents = fs::read_to_string(&json).unwrap();
        assert!(!yaml_contents.contains("factor:"));
        assert!(!yaml_contents.contains("exact:"));
        assert!(!json_contents.contains("factor"));
        assert!(!json_contents.contains("exact"));
        let from_yaml = GenerationManifest::from_yaml(&yaml_contents).unwrap();
        let from_json = GenerationManifest::from_json(&json_contents).unwrap();
        assert_eq!(from_yaml, from_json);
        let topology = MetroTopology::from_yaml(TOPOLOGY_YAML).unwrap();
        let resolved = analyze_density(&topology, &GenerationManifest::default())
            .unwrap()
            .options;
        assert_eq!(
            analyze_density(&topology, &from_yaml).unwrap().options,
            resolved
        );
        assert!(matches!(
            derive(&input, Some(Path::new("generation.txt"))),
            Err(CliError::UnsupportedFormat(_))
        ));

        fs::remove_file(input).unwrap();
        fs::remove_file(yaml).unwrap();
        fs::remove_file(json).unwrap();
    }

    #[test]
    fn density_uses_separate_config_and_reports_invalid_yaml() {
        let input = temporary_path("config-input.yaml");
        let output = temporary_path("config-output.yaml");
        let config = temporary_path("generation.yaml");
        fs::write(&input, TOPOLOGY_YAML).unwrap();
        fs::write(&config, "density-reshape:\n  bandwidth: 10.0\n").unwrap();
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
