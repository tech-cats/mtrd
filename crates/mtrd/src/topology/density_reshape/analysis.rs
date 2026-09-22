use std::collections::{HashMap, HashSet};

use serde::Serialize;
use thiserror::Error;

use crate::{DensityEstimator, MetroTopology, TopologyPosition, TopologyRenderError};

const MAX_TRIANGLES: usize = 250_000;
const MAX_RASTER_PIXELS: usize = 1_000_000;
const CUTOFF: f64 = 4.0;

#[derive(Debug, Error, PartialEq)]
pub enum DensityError {
    #[error(transparent)]
    InvalidTopology(#[from] TopologyRenderError),
    #[error("density parameter '{name}' must be {requirement}")]
    InvalidParameter {
        name: &'static str,
        requirement: &'static str,
    },
    #[error("{kind} exceeds the supported size limit of {limit}")]
    SizeLimit { kind: &'static str, limit: usize },
    #[error("density computation exceeded the supported numeric range")]
    NumericRange,
    #[error("point is outside the density warp domain")]
    OutsideWarpDomain,
    #[error("invalid density warp: {0}")]
    InvalidWarp(&'static str),
    #[error("{method} density warp did not converge")]
    WarpDidNotConverge { method: &'static str },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ResolvedDensityOptions {
    pub estimator: DensityEstimator,
    pub bandwidth: f64,
    pub mesh_cell_size: f64,
    pub raster_pixel_size: f64,
    pub padding: f64,
    pub station_weight: f64,
    pub segment_weight: f64,
    pub density_floor: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DensityTriangle {
    pub vertices: [usize; 3],
    pub area: f64,
    pub mass: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DensityAnalysis {
    pub options: ResolvedDensityOptions,
    pub bounds: [f64; 4],
    pub grid_size: [usize; 2],
    pub vertices: Vec<TopologyPosition>,
    pub vertex_density: Vec<f64>,
    pub triangles: Vec<DensityTriangle>,
    pub stations: Vec<TopologyPosition>,
    pub segments: Vec<[TopologyPosition; 2]>,
    pub total_mass: f64,
    pub expected_mass: f64,
    pub mass_relative_error: f64,
    pub raster_size: Option<[usize; 2]>,
}

/// Analyse a complete topology without modifying its source coordinates.
pub fn analyze_density(
    topology: &MetroTopology,
    generation: &crate::GenerationManifest,
) -> Result<DensityAnalysis, DensityError> {
    let source = topology.clone().canonicalize_coordinates()?;
    analyze_canonical_density(&source, generation)
}

pub(crate) fn analyze_canonical_density(
    source: &MetroTopology,
    generation: &crate::GenerationManifest,
) -> Result<DensityAnalysis, DensityError> {
    generation.validate()?;
    let stations: Vec<_> = source
        .stations
        .iter()
        .map(|station| station.position)
        .collect();
    let segments = unique_segments(source);
    let nearest = median_nearest(&stations);
    let scale = nearest.unwrap_or_else(|| source.options.lines.width.get().max(1.0));
    let input = &generation.density_reshape;
    let bandwidth = positive("bandwidth", input.bandwidth.unwrap_or(2.0 * scale))?;
    let mesh_cell_size = positive(
        "mesh-cell-size",
        input
            .mesh_cell_size
            .unwrap_or(bandwidth / (2.0 * 2.0_f64.sqrt())),
    )?;
    if mesh_cell_size > bandwidth / (2.0 * 2.0_f64.sqrt()) * (1.0 + 1e-12) {
        return Err(DensityError::InvalidParameter {
            name: "mesh-cell-size",
            requirement: "no greater than bandwidth / (2 sqrt(2))",
        });
    }
    let raster_pixel_size = positive(
        "raster-pixel-size",
        input.raster_pixel_size.unwrap_or(bandwidth / 4.0),
    )?;
    if raster_pixel_size > bandwidth / 4.0 * (1.0 + 1e-12) {
        return Err(DensityError::InvalidParameter {
            name: "raster-pixel-size",
            requirement: "no greater than bandwidth / 4",
        });
    }
    let padding = positive("padding", input.padding.unwrap_or(3.0 * bandwidth))?;
    let station_weight = nonnegative("station-weight", input.station_weight.unwrap_or(1.0))?;
    let segment_weight = nonnegative(
        "segment-weight",
        input.segment_weight.unwrap_or(1.0 / scale),
    )?;
    if station_weight == 0.0 && segment_weight == 0.0 {
        return Err(DensityError::InvalidParameter {
            name: "station-weight/segment-weight",
            requirement: "at least one positive demand weight",
        });
    }
    let mut bounds = station_bounds(&stations);
    bounds[0] -= padding;
    bounds[1] -= padding;
    bounds[2] += padding;
    bounds[3] += padding;
    let width = positive("domain-width", bounds[2] - bounds[0])?;
    let height = positive("domain-height", bounds[3] - bounds[1])?;
    let domain_area = positive("domain-area", width * height)?;
    let segment_length: f64 = segments.iter().map(|[a, b]| distance(*a, *b)).sum();
    let demand = stations.len() as f64 * station_weight + segment_length * segment_weight;
    let density_floor = positive(
        "density-floor",
        input
            .density_floor
            .unwrap_or(0.05 * demand.max(1.0) / domain_area),
    )?;
    let options = ResolvedDensityOptions {
        estimator: input.estimator,
        bandwidth,
        mesh_cell_size,
        raster_pixel_size,
        padding,
        station_weight,
        segment_weight,
        density_floor,
    };
    let (nx, ny) = grid_counts(
        width,
        height,
        mesh_cell_size,
        MAX_TRIANGLES / 2,
        "mesh triangles",
    )?;
    let mut vertices = Vec::with_capacity((nx + 1) * (ny + 1));
    for row in 0..=ny {
        for column in 0..=nx {
            vertices.push(TopologyPosition {
                x: bounds[0] + width * column as f64 / nx as f64,
                y: bounds[1] + height * row as f64 / ny as f64,
            });
        }
    }
    let mut triangle_indices = Vec::with_capacity(2 * nx * ny);
    for row in 0..ny {
        for column in 0..nx {
            let a = row * (nx + 1) + column;
            let b = a + 1;
            let c = a + nx + 1;
            let d = c + 1;
            if (row + column) % 2 == 0 {
                triangle_indices.extend([[a, b, d], [a, d, c]]);
            } else {
                triangle_indices.extend([[a, b, c], [b, d, c]]);
            }
        }
    }
    let raster = if input.estimator == DensityEstimator::RasterConvolution {
        Some(Raster::new(bounds, &stations, &segments, &options)?)
    } else {
        None
    };
    let field = |point| match &raster {
        Some(raster) => raster.sample(point) + density_floor,
        None => direct_density(point, &stations, &segments, &options),
    };
    let vertex_density: Vec<_> = vertices.iter().copied().map(field).collect();
    let mut triangles = Vec::with_capacity(triangle_indices.len());
    let mut total_mass = 0.0;
    let area = width * height / (2 * nx * ny) as f64;
    for indices in triangle_indices {
        let density = match input.estimator {
            DensityEstimator::VertexKde => {
                indices
                    .iter()
                    .map(|&index| vertex_density[index])
                    .sum::<f64>()
                    / 3.0
            }
            DensityEstimator::TriangleQuadrature | DensityEstimator::RasterConvolution => {
                let [a, b, c] = indices.map(|index| vertices[index]);
                let samples = [
                    barycentric(a, b, c, [2.0 / 3.0, 1.0 / 6.0, 1.0 / 6.0]),
                    barycentric(a, b, c, [1.0 / 6.0, 2.0 / 3.0, 1.0 / 6.0]),
                    barycentric(a, b, c, [1.0 / 6.0, 1.0 / 6.0, 2.0 / 3.0]),
                ];
                samples.into_iter().map(field).sum::<f64>() / 3.0
            }
        };
        let mass = area * density;
        if !mass.is_finite() || mass <= 0.0 {
            return Err(DensityError::NumericRange);
        }
        total_mass += mass;
        triangles.push(DensityTriangle {
            vertices: indices,
            area,
            mass,
        });
    }
    let expected_mass = density_floor * domain_area + demand;
    let mass_relative_error = (total_mass - expected_mass).abs() / expected_mass;
    if !total_mass.is_finite() || !mass_relative_error.is_finite() {
        return Err(DensityError::NumericRange);
    }
    let raster_size = raster.as_ref().map(|raster| [raster.nx, raster.ny]);
    Ok(DensityAnalysis {
        options,
        bounds,
        grid_size: [nx, ny],
        vertices,
        vertex_density,
        triangles,
        stations,
        segments,
        total_mass,
        expected_mass,
        mass_relative_error,
        raster_size,
    })
}

fn positive(name: &'static str, value: f64) -> Result<f64, DensityError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(DensityError::InvalidParameter {
            name,
            requirement: "finite and strictly positive",
        })
    }
}

fn nonnegative(name: &'static str, value: f64) -> Result<f64, DensityError> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(DensityError::InvalidParameter {
            name,
            requirement: "finite and nonnegative",
        })
    }
}

fn station_bounds(stations: &[TopologyPosition]) -> [f64; 4] {
    let Some(first) = stations.first() else {
        return [0.0; 4];
    };
    let mut bounds = [first.x, first.y, first.x, first.y];
    for point in &stations[1..] {
        bounds[0] = bounds[0].min(point.x);
        bounds[1] = bounds[1].min(point.y);
        bounds[2] = bounds[2].max(point.x);
        bounds[3] = bounds[3].max(point.y);
    }
    bounds
}

fn grid_counts(
    width: f64,
    height: f64,
    cell: f64,
    limit: usize,
    kind: &'static str,
) -> Result<(usize, usize), DensityError> {
    let counts = [width / cell, height / cell];
    if counts
        .iter()
        .any(|&value| !value.is_finite() || value >= limit as f64)
    {
        return Err(DensityError::SizeLimit {
            kind,
            limit: if kind == "mesh triangles" {
                MAX_TRIANGLES
            } else {
                MAX_RASTER_PIXELS
            },
        });
    }
    let mut nx = counts[0].ceil().max(1.0) as usize;
    let mut ny = counts[1].ceil().max(1.0) as usize;
    while ((width / nx as f64) / (height / ny as f64))
        .max((height / ny as f64) / (width / nx as f64))
        > 1.25
    {
        if width / nx as f64 > height / ny as f64 {
            nx += 1;
        } else {
            ny += 1;
        }
        if nx.checked_mul(ny).is_none_or(|count| count > limit) {
            return Err(DensityError::SizeLimit {
                kind,
                limit: if kind == "mesh triangles" {
                    MAX_TRIANGLES
                } else {
                    MAX_RASTER_PIXELS
                },
            });
        }
    }
    if nx.checked_mul(ny).is_none_or(|count| count > limit) {
        return Err(DensityError::SizeLimit {
            kind,
            limit: if kind == "mesh triangles" {
                MAX_TRIANGLES
            } else {
                MAX_RASTER_PIXELS
            },
        });
    }
    Ok((nx, ny))
}

fn median_nearest(points: &[TopologyPosition]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let mut nearest: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(i, &a)| {
            points
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, &b)| distance(a, b))
                .fold(f64::INFINITY, f64::min)
        })
        .collect();
    nearest.sort_by(f64::total_cmp);
    let upper = nearest.len() / 2;
    if nearest.len() % 2 == 0 {
        Some((nearest[upper - 1] + nearest[upper]) / 2.0)
    } else {
        Some(nearest[upper])
    }
}

fn unique_segments(topology: &MetroTopology) -> Vec<[TopologyPosition; 2]> {
    let lookup: HashMap<_, _> = topology
        .stations
        .iter()
        .enumerate()
        .map(|(index, station)| (station.id.as_str(), index))
        .collect();
    let mut seen = HashSet::new();
    let mut segments = Vec::new();
    for line in &topology.lines {
        for path in &line.paths {
            let pairs = path.stations.windows(2).map(|pair| (&pair[0], &pair[1]));
            let closing = path
                .closed
                .then(|| (path.stations.last().unwrap(), &path.stations[0]));
            for (start, end) in pairs.chain(closing) {
                let a = lookup[start.as_str()];
                let b = lookup[end.as_str()];
                let key = (a.min(b), a.max(b));
                if seen.insert(key) {
                    segments.push([
                        topology.stations[key.0].position,
                        topology.stations[key.1].position,
                    ]);
                }
            }
        }
    }
    segments
}

fn distance(a: TopologyPosition, b: TopologyPosition) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn barycentric(
    a: TopologyPosition,
    b: TopologyPosition,
    c: TopologyPosition,
    w: [f64; 3],
) -> TopologyPosition {
    TopologyPosition {
        x: a.x * w[0] + b.x * w[1] + c.x * w[2],
        y: a.y * w[0] + b.y * w[1] + c.y * w[2],
    }
}

fn gaussian(distance_squared: f64, h: f64) -> f64 {
    (-distance_squared / (2.0 * h * h)).exp() / (2.0 * std::f64::consts::PI * h * h)
}

fn direct_density(
    point: TopologyPosition,
    stations: &[TopologyPosition],
    segments: &[[TopologyPosition; 2]],
    options: &ResolvedDensityOptions,
) -> f64 {
    let h = options.bandwidth;
    let cutoff_squared = (CUTOFF * h).powi(2);
    let mut density = options.density_floor;
    for station in stations {
        let d2 = (point.x - station.x).powi(2) + (point.y - station.y).powi(2);
        if d2 <= cutoff_squared {
            density += options.station_weight * gaussian(d2, h);
        }
    }
    for &[a, b] in segments {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let length_squared = dx * dx + dy * dy;
        let t = (((point.x - a.x) * dx + (point.y - a.y) * dy) / length_squared).clamp(0.0, 1.0);
        let d2 = (point.x - a.x - t * dx).powi(2) + (point.y - a.y - t * dy).powi(2);
        if d2 > cutoff_squared {
            continue;
        }
        let length = length_squared.sqrt();
        let from = (t * length - CUTOFF * h).max(0.0);
        let to = (t * length + CUTOFF * h).min(length);
        let span = to - from;
        let count = (span / (h / 2.0)).ceil().max(1.0) as usize;
        // Two-point Gaussian quadrature on short arclength subsegments.
        for k in 0..count {
            for sign in [-1.0, 1.0] {
                let arclength =
                    from + span * (k as f64 + 0.5 + sign / (2.0 * 3.0_f64.sqrt())) / count as f64;
                let u = arclength / length;
                let x = a.x + u * dx;
                let y = a.y + u * dy;
                let r2 = (point.x - x).powi(2) + (point.y - y).powi(2);
                density += options.segment_weight * span / (2 * count) as f64 * gaussian(r2, h);
            }
        }
    }
    density
}

struct Raster {
    bounds: [f64; 4],
    nx: usize,
    ny: usize,
    dx: f64,
    dy: f64,
    values: Vec<f64>,
}

impl Raster {
    fn new(
        bounds: [f64; 4],
        stations: &[TopologyPosition],
        segments: &[[TopologyPosition; 2]],
        options: &ResolvedDensityOptions,
    ) -> Result<Self, DensityError> {
        let (nx, ny) = grid_counts(
            bounds[2] - bounds[0],
            bounds[3] - bounds[1],
            options.raster_pixel_size,
            MAX_RASTER_PIXELS,
            "raster pixels",
        )?;
        if nx.checked_mul(ny).is_none_or(|n| n > MAX_RASTER_PIXELS) {
            return Err(DensityError::SizeLimit {
                kind: "raster pixels",
                limit: MAX_RASTER_PIXELS,
            });
        }
        let dx = (bounds[2] - bounds[0]) / nx as f64;
        let dy = (bounds[3] - bounds[1]) / ny as f64;
        if CUTOFF * options.bandwidth / dx > 512.0 || CUTOFF * options.bandwidth / dy > 512.0 {
            return Err(DensityError::InvalidParameter {
                name: "raster-pixel-size/padding",
                requirement: "a Gaussian radius of at most 512 raster pixels",
            });
        }
        let mut raster = Self {
            bounds,
            nx,
            ny,
            dx,
            dy,
            values: vec![0.0; nx * ny],
        };
        for &point in stations {
            raster.deposit(point, options.station_weight);
        }
        for &[a, b] in segments {
            let length = distance(a, b);
            let count = (length / (dx.min(dy) / 2.0)).ceil().max(1.0) as usize;
            for k in 0..count {
                let t = (k as f64 + 0.5) / count as f64;
                raster.deposit(
                    TopologyPosition {
                        x: a.x + t * (b.x - a.x),
                        y: a.y + t * (b.y - a.y),
                    },
                    options.segment_weight * length / count as f64,
                );
            }
        }
        raster.convolve(options.bandwidth);
        Ok(raster)
    }

    fn deposit(&mut self, point: TopologyPosition, mass: f64) {
        let fx = ((point.x - self.bounds[0]) / self.dx - 0.5).clamp(0.0, (self.nx - 1) as f64);
        let fy = ((point.y - self.bounds[1]) / self.dy - 0.5).clamp(0.0, (self.ny - 1) as f64);
        let x = fx.floor() as usize;
        let y = fy.floor() as usize;
        let rx = fx - x as f64;
        let ry = fy - y as f64;
        for (ix, wx) in [(x, 1.0 - rx), ((x + 1).min(self.nx - 1), rx)] {
            for (iy, wy) in [(y, 1.0 - ry), ((y + 1).min(self.ny - 1), ry)] {
                self.values[iy * self.nx + ix] += mass * wx * wy / (self.dx * self.dy);
            }
        }
    }

    fn convolve(&mut self, h: f64) {
        let rx = (CUTOFF * h / self.dx).ceil() as usize;
        let ry = (CUTOFF * h / self.dy).ceil() as usize;
        let weights = |radius: usize, spacing: f64| {
            let mut w: Vec<f64> = (0..=radius)
                .map(|i| (-0.5 * (i as f64 * spacing / h).powi(2)).exp())
                .collect();
            let sum = w[0] + 2.0 * w.iter().skip(1).sum::<f64>();
            for value in &mut w {
                *value /= sum;
            }
            w
        };
        let xweights = weights(rx, self.dx);
        let yweights = weights(ry, self.dy);
        let mut intermediate = vec![0.0; self.values.len()];
        for y in 0..self.ny {
            for x in 0..self.nx {
                let mut value = xweights[0] * self.values[y * self.nx + x];
                for (k, &weight) in xweights.iter().enumerate().skip(1) {
                    value += weight
                        * (self.values[y * self.nx + reflect(x as isize - k as isize, self.nx)]
                            + self.values[y * self.nx + reflect(x as isize + k as isize, self.nx)]);
                }
                intermediate[y * self.nx + x] = value;
            }
        }
        for y in 0..self.ny {
            for x in 0..self.nx {
                let mut value = yweights[0] * intermediate[y * self.nx + x];
                for k in 1..=ry {
                    value += yweights[k]
                        * (intermediate[reflect(y as isize - k as isize, self.ny) * self.nx + x]
                            + intermediate
                                [reflect(y as isize + k as isize, self.ny) * self.nx + x]);
                }
                self.values[y * self.nx + x] = value;
            }
        }
    }

    fn sample(&self, point: TopologyPosition) -> f64 {
        let fx = ((point.x - self.bounds[0]) / self.dx - 0.5).clamp(0.0, (self.nx - 1) as f64);
        let fy = ((point.y - self.bounds[1]) / self.dy - 0.5).clamp(0.0, (self.ny - 1) as f64);
        let x = fx.floor() as usize;
        let y = fy.floor() as usize;
        let rx = fx - x as f64;
        let ry = fy - y as f64;
        [(x, 1.0 - rx), ((x + 1).min(self.nx - 1), rx)]
            .iter()
            .flat_map(|&(ix, wx)| {
                [(y, 1.0 - ry), ((y + 1).min(self.ny - 1), ry)]
                    .map(move |(iy, wy)| self.values[iy * self.nx + ix] * wx * wy)
            })
            .sum()
    }
}

fn reflect(index: isize, length: usize) -> usize {
    let period = 2 * length as isize;
    let position = index.rem_euclid(period);
    if position < length as isize {
        position as usize
    } else {
        (period - 1 - position) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GenerationManifest, TopologyLine, TopologyPath};

    fn topology() -> MetroTopology {
        MetroTopology::from_yaml(
            r##"
options:
  languages: { set: [en], primary: en }
  lines: { width: 2.0 }
  stations:
    common:
      fill: { diameter: 4.0, color: { type: unified, value: "#fff" } }
      stroke: { width: 1.0, alignment: center, color: { type: follow-line } }
    interchange:
      fill: { width: 4.0, color: "#fff" }
      stroke: { width: 1.0, alignment: outside, color: "#000" }
stations:
  - { id: A, names: { en: [A] }, position: [0, 0] }
  - { id: B, names: { en: [B] }, position: [8, 0] }
  - { id: C, names: { en: [C] }, position: [8, 8] }
  - { id: D, names: { en: [D] }, position: [0, 8] }
lines:
  - id: red
    names: { en: [Red] }
    color: "#f00"
    paths: [{ stations: [A, B, C, D], closed: false }]
"##,
        )
        .unwrap()
    }

    #[test]
    fn all_estimators_produce_positive_mass_on_a_quasi_square_mesh() {
        let topology = topology();
        let mut config = GenerationManifest::default();
        for estimator in [
            DensityEstimator::VertexKde,
            DensityEstimator::TriangleQuadrature,
            DensityEstimator::RasterConvolution,
        ] {
            config.density_reshape.estimator = estimator;
            let result = analyze_density(&topology, &config).unwrap();
            let [nx, ny] = result.grid_size;
            assert_eq!(result.vertices.len(), (nx + 1) * (ny + 1));
            assert_eq!(result.triangles.len(), 2 * nx * ny);
            assert_eq!(result.triangles[0].vertices, [0, 1, nx + 2]);
            assert_eq!(result.triangles[2].vertices, [1, 2, nx + 2]);
            assert!(
                result
                    .triangles
                    .iter()
                    .all(|triangle| triangle.area > 0.0 && triangle.mass > 0.0)
            );
            assert!(
                result.mass_relative_error < 0.03,
                "{estimator:?}: {}",
                result.mass_relative_error
            );
            let dx = (result.bounds[2] - result.bounds[0]) / nx as f64;
            let dy = (result.bounds[3] - result.bounds[1]) / ny as f64;
            assert!(dx.max(dy) / dx.min(dy) <= 1.25);
            assert!(dx.hypot(dy) <= result.options.bandwidth / 2.0 + 1e-9);
            for triangle in &result.triangles {
                let [a, b, c] = triangle.vertices.map(|index| result.vertices[index]);
                assert!((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x) > 0.0);
            }
        }
    }

    #[test]
    fn shared_segments_are_counted_once_and_station_density_is_independent() {
        let mut topology = topology();
        let mut config = GenerationManifest::default();
        let baseline = analyze_density(&topology, &config).unwrap();
        topology.lines.push(TopologyLine {
            id: "blue".into(),
            names: [("en".into(), vec!["Blue".into()])].into(),
            color: "#00f".into(),
            paths: vec![TopologyPath {
                stations: vec!["C".into(), "B".into(), "A".into()],
                closed: false,
            }],
        });
        let shared = analyze_density(&topology, &config).unwrap();
        assert_eq!(baseline.segments, shared.segments);
        assert_eq!(baseline.vertex_density, shared.vertex_density);
        assert_eq!(baseline.total_mass, shared.total_mass);
        config.density_reshape.segment_weight = Some(0.0);
        let station_only = analyze_density(&topology, &config).unwrap();
        assert!(station_only.total_mass < shared.total_mass);
    }

    #[test]
    fn scalar_values_override_defaults_and_invalid_controls_fail() {
        let topology = topology();
        let mut config = GenerationManifest::default();
        let default = analyze_density(&topology, &config).unwrap();
        config.density_reshape.bandwidth = Some(2.0 * default.options.bandwidth);
        let doubled = analyze_density(&topology, &config).unwrap();
        assert_eq!(doubled.options.bandwidth, 2.0 * default.options.bandwidth);
        config.density_reshape.bandwidth = Some(10.0);
        assert_eq!(
            analyze_density(&topology, &config)
                .unwrap()
                .options
                .bandwidth,
            10.0
        );
        config.density_reshape.mesh_cell_size = Some(10.0);
        assert!(matches!(
            analyze_density(&topology, &config),
            Err(DensityError::InvalidParameter {
                name: "mesh-cell-size",
                ..
            })
        ));
        config.density_reshape.mesh_cell_size = None;
        config.density_reshape.raster_pixel_size = Some(f64::NAN);
        assert!(matches!(
            analyze_density(&topology, &config),
            Err(DensityError::InvalidParameter {
                name: "raster-pixel-size",
                ..
            })
        ));
    }

    #[test]
    fn strict_configuration_round_trips_and_rejects_wrapped_values() {
        let options: crate::DensityReshapeOptions = serde_yaml::from_str(
            "estimator: raster-convolution\nbandwidth: 12.0\nmesh-cell-size: 0.5\n",
        )
        .unwrap();
        let yaml = serde_yaml::to_string(&options).unwrap();
        assert_eq!(
            serde_yaml::from_str::<crate::DensityReshapeOptions>(&yaml).unwrap(),
            options
        );
        assert!(
            serde_yaml::from_str::<crate::DensityReshapeOptions>("bandwidth: { exact: 1 }")
                .is_err()
        );
        assert!(
            serde_yaml::from_str::<crate::DensityReshapeOptions>("bandwidth: { factor: 2 }")
                .is_err()
        );
        assert!(
            serde_yaml::from_str::<crate::DensityReshapeOptions>("estimator: missing").is_err()
        );
        assert!(serde_yaml::from_str::<crate::DensityReshapeOptions>("unknown: 1").is_err());
    }

    #[test]
    fn handles_empty_and_single_station_topologies_without_nan_defaults() {
        let mut topology = topology();
        let config = GenerationManifest::default();
        topology.stations.clear();
        topology.lines.clear();
        let empty = analyze_density(&topology, &config).unwrap();
        assert!(empty.total_mass.is_finite());
        assert!(empty.triangles.iter().all(|triangle| triangle.mass > 0.0));
        topology.stations.push(crate::TopologyStation {
            id: "alone".into(),
            names: [("en".into(), vec!["Alone".into()])].into(),
            position: TopologyPosition { x: 0.0, y: 0.0 },
        });
        let one = analyze_density(&topology, &config).unwrap();
        assert!(one.options.bandwidth > 0.0);
        assert!(one.total_mass.is_finite());
    }

    #[test]
    fn rejects_mesh_and_raster_size_overrides_before_allocation() {
        let topology = topology();
        let mut config = GenerationManifest::default();
        config.density_reshape.mesh_cell_size = Some(1e-9);
        assert!(matches!(
            analyze_density(&topology, &config),
            Err(DensityError::SizeLimit {
                kind: "mesh triangles",
                ..
            })
        ));
        config.density_reshape.mesh_cell_size = None;
        config.density_reshape.estimator = DensityEstimator::RasterConvolution;
        config.density_reshape.raster_pixel_size = Some(1e-9);
        assert!(matches!(
            analyze_density(&topology, &config),
            Err(DensityError::SizeLimit {
                kind: "raster pixels",
                ..
            })
        ));
    }
}
