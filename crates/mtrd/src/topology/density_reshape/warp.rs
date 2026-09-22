use std::collections::{HashMap, HashSet};

use rustdct::DctPlanner;
use serde::Serialize;

use crate::{DensityWarpMethod, GenerationManifest, MetroTopology, TopologyPosition};

use super::analysis::{DensityAnalysis, DensityError, analyze_canonical_density};

const AREA_FLOOR: f64 = 1e-8;
const OVERLAP_BUCKET_LIMIT: usize = 2_000_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WarpedStation {
    pub id: String,
    pub source: TopologyPosition,
    pub warped: TopologyPosition,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WarpedSegment {
    pub source: [TopologyPosition; 2],
    pub points: Vec<TopologyPosition>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DensityWarpDiagnostics {
    pub maximum_area_residual: f64,
    pub minimum_area_ratio: f64,
    pub maximum_condition_number: f64,
    pub mean_station_displacement: f64,
    pub maximum_station_displacement: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DensityWarpAnalysis {
    pub method: DensityWarpMethod,
    pub equalization_strength: f64,
    pub bounds: [f64; 4],
    pub grid_size: [usize; 2],
    pub source_vertices: Vec<TopologyPosition>,
    pub warped_vertices: Vec<TopologyPosition>,
    pub triangles: Vec<[usize; 3]>,
    pub stations: Vec<WarpedStation>,
    pub segments: Vec<WarpedSegment>,
    pub diagnostics: DensityWarpDiagnostics,
}

/// Inspect the pre-layout deformation without changing the source topology.
pub fn analyze_density_warp(
    topology: &MetroTopology,
    generation: &GenerationManifest,
) -> Result<DensityWarpAnalysis, DensityError> {
    let source = topology.clone().canonicalize_coordinates()?;
    let analysis = analyze_canonical_density(&source, generation)?;
    let method = generation.density_reshape.method;
    let strength = generation.density_reshape.equalization_strength;
    let warp = DensityWarp::build(&analysis, method, strength)?;
    let stations = source
        .stations
        .iter()
        .map(|station| {
            Ok(WarpedStation {
                id: station.id.clone(),
                source: station.position,
                warped: warp.transform(station.position)?,
            })
        })
        .collect::<Result<Vec<_>, DensityError>>()?;
    let segments = analysis
        .segments
        .iter()
        .map(|&source| {
            Ok(WarpedSegment {
                source,
                points: warp.segment_polyline(source)?,
            })
        })
        .collect::<Result<Vec<_>, DensityError>>()?;
    let diagnostics = diagnostics(&analysis, &warp, &stations, strength)?;
    Ok(DensityWarpAnalysis {
        method,
        equalization_strength: strength,
        bounds: analysis.bounds,
        grid_size: analysis.grid_size,
        source_vertices: analysis.vertices,
        warped_vertices: warp.vertices,
        triangles: analysis.triangles.iter().map(|t| t.vertices).collect(),
        stations,
        segments,
        diagnostics,
    })
}

/// A continuous piecewise-affine map over the canonical density mesh.
#[derive(Debug, Clone)]
pub(crate) struct DensityWarp {
    identity: bool,
    bounds: [f64; 4],
    grid_size: [usize; 2],
    source: Vec<TopologyPosition>,
    vertices: Vec<TopologyPosition>,
    triangles: Vec<[usize; 3]>,
}

impl DensityWarp {
    #[cfg(test)]
    pub(super) fn test_identity(bounds: [f64; 4]) -> Self {
        let [left, top, right, bottom] = bounds;
        let source = vec![
            TopologyPosition { x: left, y: top },
            TopologyPosition { x: right, y: top },
            TopologyPosition { x: left, y: bottom },
            TopologyPosition {
                x: right,
                y: bottom,
            },
        ];
        Self {
            identity: true,
            bounds,
            grid_size: [1, 1],
            source: source.clone(),
            vertices: source,
            triangles: vec![[0, 1, 3], [0, 3, 2]],
        }
    }

    pub(crate) fn build(
        analysis: &DensityAnalysis,
        method: DensityWarpMethod,
        strength: f64,
    ) -> Result<Self, DensityError> {
        let source = analysis.vertices.clone();
        let vertices = if strength == 0.0 {
            source.clone()
        } else {
            match method {
                DensityWarpMethod::Diffusion => diffusion(analysis, strength)?,
                DensityWarpMethod::TriangleArea => triangle_area(analysis, strength)?,
            }
        };
        let warp = Self {
            identity: vertices == source,
            bounds: analysis.bounds,
            grid_size: analysis.grid_size,
            source,
            vertices,
            triangles: analysis.triangles.iter().map(|t| t.vertices).collect(),
        };
        warp.validate()?;
        Ok(warp)
    }

    pub(crate) fn transform(
        &self,
        position: TopologyPosition,
    ) -> Result<TopologyPosition, DensityError> {
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(DensityError::NumericRange);
        }
        let [min_x, min_y, max_x, max_y] = self.bounds;
        let tolerance = (max_x - min_x).max(max_y - min_y) * 1e-10;
        if position.x < min_x - tolerance
            || position.x > max_x + tolerance
            || position.y < min_y - tolerance
            || position.y > max_y + tolerance
        {
            return Err(DensityError::OutsideWarpDomain);
        }
        if self.identity {
            return Ok(position);
        }
        let [nx, ny] = self.grid_size;
        let x = position.x.clamp(min_x, max_x);
        let y = position.y.clamp(min_y, max_y);
        let column = (((x - min_x) / (max_x - min_x) * nx as f64).floor() as usize).min(nx - 1);
        let row = (((y - min_y) / (max_y - min_y) * ny as f64).floor() as usize).min(ny - 1);
        let first = 2 * (row * nx + column);
        for &indices in &self.triangles[first..first + 2] {
            let [a, b, c] = indices.map(|index| self.source[index]);
            let weights = barycentric_weights(TopologyPosition { x, y }, a, b, c);
            if weights.iter().all(|&weight| weight >= -1e-10) {
                let [a, b, c] = indices.map(|index| self.vertices[index]);
                return Ok(weighted(a, b, c, weights));
            }
        }
        Err(DensityError::InvalidWarp("source triangle lookup failed"))
    }

    pub(crate) fn segment_polyline(
        &self,
        segment: [TopologyPosition; 2],
    ) -> Result<Vec<TopologyPosition>, DensityError> {
        let [a, b] = segment;
        let [min_x, min_y, max_x, max_y] = self.bounds;
        let [nx, ny] = self.grid_size;
        let mut parameters = vec![0.0, 1.0];
        for column in 1..nx {
            if b.x != a.x {
                let x = min_x + (max_x - min_x) * column as f64 / nx as f64;
                let t = (x - a.x) / (b.x - a.x);
                if t > 0.0 && t < 1.0 {
                    parameters.push(t);
                }
            }
        }
        for row in 1..ny {
            if b.y != a.y {
                let y = min_y + (max_y - min_y) * row as f64 / ny as f64;
                let t = (y - a.y) / (b.y - a.y);
                if t > 0.0 && t < 1.0 {
                    parameters.push(t);
                }
            }
        }
        sort_parameters(&mut parameters);
        let intervals = parameters.clone();
        for pair in intervals.windows(2) {
            let mid = point_on_segment(a, b, (pair[0] + pair[1]) / 2.0);
            let column =
                (((mid.x - min_x) / (max_x - min_x) * nx as f64).floor() as usize).min(nx - 1);
            let row =
                (((mid.y - min_y) / (max_y - min_y) * ny as f64).floor() as usize).min(ny - 1);
            let base = row * (nx + 1) + column;
            let [d0, d1] = if (row + column).is_multiple_of(2) {
                [base, base + nx + 2]
            } else {
                [base + 1, base + nx + 1]
            }
            .map(|index| self.source[index]);
            if let Some(t) = segment_intersection_parameter(a, b, d0, d1)
                && t > pair[0] + 1e-12
                && t < pair[1] - 1e-12
            {
                parameters.push(t);
            }
        }
        sort_parameters(&mut parameters);
        parameters
            .into_iter()
            .map(|t| self.transform(point_on_segment(a, b, t)))
            .collect()
    }

    fn validate(&self) -> Result<(), DensityError> {
        let [nx, ny] = self.grid_size;
        let [min_x, min_y, max_x, max_y] = self.bounds;
        let tolerance = (max_x - min_x).max(max_y - min_y) * 1e-8;
        if self.vertices.iter().any(|point| {
            !point.x.is_finite()
                || !point.y.is_finite()
                || point.x < min_x - tolerance
                || point.x > max_x + tolerance
                || point.y < min_y - tolerance
                || point.y > max_y + tolerance
        }) {
            return Err(DensityError::InvalidWarp("vertex left the output domain"));
        }
        for row in 0..=ny {
            for column in 0..=nx {
                if row != 0 && row != ny && column != 0 && column != nx {
                    continue;
                }
                let index = row * (nx + 1) + column;
                let point = self.vertices[index];
                let source = self.source[index];
                if (row == 0 || row == ny) && (point.y - source.y).abs() > tolerance
                    || (column == 0 || column == nx) && (point.x - source.x).abs() > tolerance
                {
                    return Err(DensityError::InvalidWarp("boundary is not rectangular"));
                }
            }
        }
        for column in 0..nx {
            for row in [0, ny] {
                let first = self.vertices[row * (nx + 1) + column];
                let second = self.vertices[row * (nx + 1) + column + 1];
                if second.x - first.x <= tolerance {
                    return Err(DensityError::InvalidWarp("boundary order changed"));
                }
            }
        }
        for row in 0..ny {
            for column in [0, nx] {
                let first = self.vertices[row * (nx + 1) + column];
                let second = self.vertices[(row + 1) * (nx + 1) + column];
                if second.y - first.y <= tolerance {
                    return Err(DensityError::InvalidWarp("boundary order changed"));
                }
            }
        }
        for &indices in &self.triangles {
            let original = indices.map(|index| self.source[index]);
            let output = indices.map(|index| self.vertices[index]);
            let a0 = signed_area(original);
            let a1 = signed_area(output);
            if !a1.is_finite() || a1 <= AREA_FLOOR * a0 {
                return Err(DensityError::InvalidWarp("triangle has non-positive area"));
            }
        }
        self.validate_nonlocal_overlap()?;
        Ok(())
    }

    fn validate_nonlocal_overlap(&self) -> Result<(), DensityError> {
        let [nx, ny] = self.grid_size;
        let [min_x, min_y, max_x, max_y] = self.bounds;
        let dx = (max_x - min_x) / nx as f64;
        let dy = (max_y - min_y) / ny as f64;
        let mut buckets: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        let mut entries = 0usize;
        for (index, &triangle) in self.triangles.iter().enumerate() {
            let points = triangle.map(|vertex| self.vertices[vertex]);
            let left = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let right = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
            let top = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let bottom = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
            let x0 = (((left - min_x) / dx).floor() as usize).min(nx - 1);
            let x1 = (((right - min_x) / dx).floor() as usize).min(nx - 1);
            let y0 = (((top - min_y) / dy).floor() as usize).min(ny - 1);
            let y1 = (((bottom - min_y) / dy).floor() as usize).min(ny - 1);
            for row in y0..=y1 {
                for column in x0..=x1 {
                    buckets.entry((column, row)).or_default().push(index);
                    entries += 1;
                    if entries > OVERLAP_BUCKET_LIMIT {
                        return Err(DensityError::InvalidWarp(
                            "overlap check exceeded size limit",
                        ));
                    }
                }
            }
        }
        let mut checked = HashSet::new();
        for bucket in buckets.values() {
            for (offset, &first) in bucket.iter().enumerate() {
                for &second in &bucket[offset + 1..] {
                    let pair = (first.min(second), first.max(second));
                    if checked.insert(pair)
                        && triangles_overlap(
                            self.triangles[first].map(|index| self.vertices[index]),
                            self.triangles[second].map(|index| self.vertices[index]),
                        )
                    {
                        return Err(DensityError::InvalidWarp("output triangles overlap"));
                    }
                }
            }
        }
        Ok(())
    }
}

fn point_on_segment(a: TopologyPosition, b: TopologyPosition, t: f64) -> TopologyPosition {
    TopologyPosition {
        x: a.x + t * (b.x - a.x),
        y: a.y + t * (b.y - a.y),
    }
}

fn sort_parameters(parameters: &mut Vec<f64>) {
    parameters.sort_by(f64::total_cmp);
    parameters.dedup_by(|a, b| (*a - *b).abs() <= 1e-11);
}

fn cross(a: TopologyPosition, b: TopologyPosition, c: TopologyPosition) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn signed_area([a, b, c]: [TopologyPosition; 3]) -> f64 {
    cross(a, b, c) / 2.0
}

fn barycentric_weights(
    p: TopologyPosition,
    a: TopologyPosition,
    b: TopologyPosition,
    c: TopologyPosition,
) -> [f64; 3] {
    let denominator = cross(a, b, c);
    [
        cross(b, c, p) / denominator,
        cross(c, a, p) / denominator,
        cross(a, b, p) / denominator,
    ]
}

fn weighted(
    a: TopologyPosition,
    b: TopologyPosition,
    c: TopologyPosition,
    weights: [f64; 3],
) -> TopologyPosition {
    TopologyPosition {
        x: a.x * weights[0] + b.x * weights[1] + c.x * weights[2],
        y: a.y * weights[0] + b.y * weights[1] + c.y * weights[2],
    }
}

fn segment_intersection_parameter(
    a: TopologyPosition,
    b: TopologyPosition,
    c: TopologyPosition,
    d: TopologyPosition,
) -> Option<f64> {
    let denominator = (b.x - a.x) * (d.y - c.y) - (b.y - a.y) * (d.x - c.x);
    if denominator.abs() <= 1e-14 {
        return None;
    }
    let t = ((c.x - a.x) * (d.y - c.y) - (c.y - a.y) * (d.x - c.x)) / denominator;
    let u = ((c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)) / denominator;
    (0.0..=1.0).contains(&u).then_some(t)
}

fn triangles_overlap(a: [TopologyPosition; 3], b: [TopologyPosition; 3]) -> bool {
    let tolerance = signed_area(a).max(signed_area(b)) * 1e-10;
    for i in 0..3 {
        for j in 0..3 {
            let (p, q) = (a[i], a[(i + 1) % 3]);
            let (r, s) = (b[j], b[(j + 1) % 3]);
            let orientations = [
                cross(p, q, r),
                cross(p, q, s),
                cross(r, s, p),
                cross(r, s, q),
            ];
            if orientations[0] * orientations[1] < -tolerance
                && orientations[2] * orientations[3] < -tolerance
            {
                return true;
            }
        }
    }
    let inside = |p: TopologyPosition, triangle: [TopologyPosition; 3]| {
        (0..3).all(|i| cross(triangle[i], triangle[(i + 1) % 3], p) > tolerance)
    };
    a.iter().any(|&p| inside(p, b))
        || b.iter().any(|&p| inside(p, a))
        || inside(weighted(a[0], a[1], a[2], [1.0 / 3.0; 3]), b)
        || inside(weighted(b[0], b[1], b[2], [1.0 / 3.0; 3]), a)
}

fn triangle_area(
    analysis: &DensityAnalysis,
    strength: f64,
) -> Result<Vec<TopologyPosition>, DensityError> {
    let source = &analysis.vertices;
    let [nx, ny] = analysis.grid_size;
    let mut x: Vec<f64> = source.iter().flat_map(|p| [p.x, p.y]).collect();
    let mut fixed = vec![false; source.len()];
    for row in 0..=ny {
        for column in 0..=nx {
            fixed[row * (nx + 1) + column] = row == 0 || row == ny || column == 0 || column == nx;
        }
    }
    let cell = analysis.options.mesh_cell_size;
    let stages = (strength / 0.1).ceil().max(1.0) as usize;
    for stage in 1..=stages {
        let alpha = strength * stage as f64 / stages as f64;
        let targets: Vec<_> = analysis
            .triangles
            .iter()
            .map(|triangle| {
                (1.0 - alpha) * triangle.area
                    + alpha * triangle.mass / analysis.total_mass
                        * (analysis.bounds[2] - analysis.bounds[0])
                        * (analysis.bounds[3] - analysis.bounds[1])
            })
            .collect();
        let mut history: Vec<(Vec<f64>, Vec<f64>, f64)> = Vec::new();
        let mut previous_energy = f64::INFINITY;
        let mut converged = false;
        for _ in 0..300 {
            let Some((energy, gradient)) =
                area_energy(&x, source, analysis, &targets, &fixed, cell)
            else {
                return Err(DensityError::InvalidWarp("triangle-area iterate folded"));
            };
            let norm = gradient.iter().map(|v| v.abs()).fold(0.0_f64, f64::max) * cell;
            if norm < 1e-3 || (previous_energy - energy).abs() < 1e-8 * (1.0 + energy.abs()) {
                converged = true;
                break;
            }
            previous_energy = energy;
            let mut direction = lbfgs_direction(&gradient, &history, cell);
            for (index, &is_fixed) in fixed.iter().enumerate() {
                if is_fixed {
                    direction[2 * index] = 0.0;
                    direction[2 * index + 1] = 0.0;
                }
            }
            let mut slope = dot(&gradient, &direction);
            if slope >= 0.0 || !slope.is_finite() {
                direction = gradient.iter().map(|v| -v * cell * cell / 20.0).collect();
                slope = dot(&gradient, &direction);
                history.clear();
            }
            let largest = direction.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
            let mut step = (cell / (4.0 * largest)).min(1.0);
            let mut accepted = None;
            for _ in 0..32 {
                let trial: Vec<_> = x
                    .iter()
                    .zip(&direction)
                    .map(|(&value, &delta)| value + step * delta)
                    .collect();
                if let Some((new_energy, new_gradient)) =
                    area_energy(&trial, source, analysis, &targets, &fixed, cell)
                    && new_energy <= energy + 1e-4 * step * slope
                {
                    accepted = Some((trial, new_gradient));
                    break;
                }
                step *= 0.5;
            }
            let Some((trial, new_gradient)) = accepted else {
                return Err(DensityError::WarpDidNotConverge {
                    method: "triangle-area",
                });
            };
            let s: Vec<_> = trial.iter().zip(&x).map(|(a, b)| a - b).collect();
            let y: Vec<_> = new_gradient
                .iter()
                .zip(&gradient)
                .map(|(a, b)| a - b)
                .collect();
            let ys = dot(&y, &s);
            if ys > 1e-15 {
                history.push((s, y, 1.0 / ys));
                if history.len() > 8 {
                    history.remove(0);
                }
            }
            x = trial;
        }
        if !converged {
            return Err(DensityError::WarpDidNotConverge {
                method: "triangle-area",
            });
        }
    }
    Ok(x.as_chunks::<2>()
        .0
        .iter()
        .map(|pair| TopologyPosition {
            x: pair[0],
            y: pair[1],
        })
        .collect())
}

fn area_energy(
    x: &[f64],
    source: &[TopologyPosition],
    analysis: &DensityAnalysis,
    targets: &[f64],
    fixed: &[bool],
    cell: f64,
) -> Option<(f64, Vec<f64>)> {
    let mut energy = 0.0;
    let mut gradient = vec![0.0; x.len()];
    for (triangle, &target) in analysis.triangles.iter().zip(targets) {
        let [ia, ib, ic] = triangle.vertices;
        let [ax, ay, bx, by, cx, cy] = [
            x[2 * ia],
            x[2 * ia + 1],
            x[2 * ib],
            x[2 * ib + 1],
            x[2 * ic],
            x[2 * ic + 1],
        ];
        let area = ((bx - ax) * (cy - ay) - (by - ay) * (cx - ax)) / 2.0;
        if !area.is_finite() || area <= AREA_FLOOR * triangle.area {
            return None;
        }
        let residual = (area / target).ln();
        const BARRIER_WEIGHT: f64 = 1e-4;
        const SHAPE_WEIGHT: f64 = 0.02;
        energy += residual * residual - BARRIER_WEIGHT * (area / triangle.area).ln();
        let coefficient = (2.0 * residual - BARRIER_WEIGHT) / area;
        for (index, dx, dy) in [
            (ia, (by - cy) / 2.0, (cx - bx) / 2.0),
            (ib, (cy - ay) / 2.0, (ax - cx) / 2.0),
            (ic, (ay - by) / 2.0, (bx - ax) / 2.0),
        ] {
            gradient[2 * index] += coefficient * dx;
            gradient[2 * index + 1] += coefficient * dy;
        }
        let target_scale = target / triangle.area;
        for (i, j) in [(ia, ib), (ib, ic), (ic, ia)] {
            let original = squared_distance(source[i], source[j]);
            let desired = target_scale * original;
            let vx = x[2 * i] - x[2 * j];
            let vy = x[2 * i + 1] - x[2 * j + 1];
            let residual = (vx * vx + vy * vy) / desired - 1.0;
            energy += SHAPE_WEIGHT * residual * residual;
            let factor = 4.0 * SHAPE_WEIGHT * residual / desired;
            gradient[2 * i] += factor * vx;
            gradient[2 * i + 1] += factor * vy;
            gradient[2 * j] -= factor * vx;
            gradient[2 * j + 1] -= factor * vy;
        }
    }
    const FIDELITY_WEIGHT: f64 = 0.005;
    for (index, &point) in source.iter().enumerate() {
        if fixed[index] {
            gradient[2 * index] = 0.0;
            gradient[2 * index + 1] = 0.0;
            continue;
        }
        for (component, original) in [point.x, point.y].into_iter().enumerate() {
            let position = 2 * index + component;
            let displacement = x[position] - original;
            energy += FIDELITY_WEIGHT * displacement * displacement / (cell * cell);
            gradient[position] += 2.0 * FIDELITY_WEIGHT * displacement / (cell * cell);
        }
    }
    energy.is_finite().then_some((energy, gradient))
}

fn lbfgs_direction(gradient: &[f64], history: &[(Vec<f64>, Vec<f64>, f64)], cell: f64) -> Vec<f64> {
    let mut q = gradient.to_vec();
    let mut alphas = vec![0.0; history.len()];
    for (index, (s, y, rho)) in history.iter().enumerate().rev() {
        let alpha = rho * dot(s, &q);
        alphas[index] = alpha;
        for (value, correction) in q.iter_mut().zip(y) {
            *value -= alpha * correction;
        }
    }
    let scale = history.last().map_or(cell * cell / 20.0, |(s, y, _)| {
        (dot(s, y) / dot(y, y)).clamp(cell * cell * 1e-8, cell * cell * 1e3)
    });
    for value in &mut q {
        *value *= scale;
    }
    for ((s, y, rho), alpha) in history.iter().zip(alphas) {
        let beta = rho * dot(y, &q);
        for (value, correction) in q.iter_mut().zip(s) {
            *value += (alpha - beta) * correction;
        }
    }
    q.into_iter().map(|value| -value).collect()
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

fn diffusion(
    analysis: &DensityAnalysis,
    strength: f64,
) -> Result<Vec<TopologyPosition>, DensityError> {
    let [min_x, min_y, max_x, max_y] = analysis.bounds;
    let width = max_x - min_x;
    let height = max_y - min_y;
    let nx = (width / analysis.options.raster_pixel_size).ceil().max(2.0) as usize;
    let ny = (height / analysis.options.raster_pixel_size)
        .ceil()
        .max(2.0) as usize;
    if nx.checked_mul(ny).is_none_or(|count| count > 1_000_000) {
        return Err(DensityError::SizeLimit {
            kind: "diffusion pixels",
            limit: 1_000_000,
        });
    }
    let dx = width / nx as f64;
    let dy = height / ny as f64;
    let mut raster = Vec::with_capacity(nx * ny);
    for row in 0..ny {
        for column in 0..nx {
            let point = TopologyPosition {
                x: min_x + (column as f64 + 0.5) * dx,
                y: min_y + (row as f64 + 0.5) * dy,
            };
            raster.push(sample_vertex_density(analysis, point));
        }
    }
    let mean = raster.iter().sum::<f64>() / raster.len() as f64;
    if !mean.is_finite() || mean <= 0.0 {
        return Err(DensityError::NumericRange);
    }
    for value in &mut raster {
        *value = (1.0 - strength) * mean + strength * *value;
    }
    let deviation = raster
        .iter()
        .map(|value| ((value - mean) / mean).abs())
        .fold(0.0_f64, f64::max);
    if deviation < 1e-8 {
        return Ok(analysis.vertices.clone());
    }
    let spectrum = HeatSpectrum::new(nx, ny, dx, dy, raster);
    let lambda = (4.0 * (std::f64::consts::PI / (2.0 * nx as f64)).sin().powi(2) / dx.powi(2))
        .min(4.0 * (std::f64::consts::PI / (2.0 * ny as f64)).sin().powi(2) / dy.powi(2));
    let end_time = (deviation / 1e-4).max(1.0).ln() / lambda;
    let mut points = analysis.vertices.clone();
    let mut time = 0.0;
    let base_time = dx.min(dy).powi(2);
    let [mesh_nx, mesh_ny] = analysis.grid_size;
    for _ in 0..512 {
        if time >= end_time * (1.0 - 1e-12) {
            return Ok(points);
        }
        let field = spectrum.field(time)?;
        let velocity = velocity_field(&field, nx, ny, dx, dy)?;
        let max_velocity = velocity
            .iter()
            .map(|v| v[0].hypot(v[1]))
            .fold(0.0_f64, f64::max);
        let max_step = analysis.options.mesh_cell_size / (4.0 * max_velocity.max(1e-30));
        let mut step = (base_time + time / 2.0).min(max_step).min(end_time - time);
        let mut accepted = None;
        for _ in 0..24 {
            let half_field = spectrum.field(time + step / 2.0)?;
            let half_velocity = velocity_field(&half_field, nx, ny, dx, dy)?;
            let mut trial = Vec::with_capacity(points.len());
            for (index, &point) in points.iter().enumerate() {
                let v0 = sample_velocity(&velocity, point, analysis.bounds, nx, ny);
                let midpoint = TopologyPosition {
                    x: point.x + step * v0[0] / 2.0,
                    y: point.y + step * v0[1] / 2.0,
                };
                let v1 = sample_velocity(&half_velocity, midpoint, analysis.bounds, nx, ny);
                let mut output = TopologyPosition {
                    x: point.x + step * v1[0],
                    y: point.y + step * v1[1],
                };
                let row = index / (mesh_nx + 1);
                let column = index % (mesh_nx + 1);
                if row == 0 || row == mesh_ny {
                    output.y = analysis.vertices[index].y;
                }
                if column == 0 || column == mesh_nx {
                    output.x = analysis.vertices[index].x;
                }
                trial.push(output);
            }
            if trial.iter().all(|p| {
                p.x.is_finite()
                    && p.y.is_finite()
                    && p.x >= min_x - 1e-9 * width
                    && p.x <= max_x + 1e-9 * width
                    && p.y >= min_y - 1e-9 * height
                    && p.y <= max_y + 1e-9 * height
            }) && analysis.triangles.iter().all(|triangle| {
                signed_area(triangle.vertices.map(|index| trial[index]))
                    > AREA_FLOOR * triangle.area
            }) {
                accepted = Some(trial);
                break;
            }
            step *= 0.5;
        }
        let Some(trial) = accepted else {
            return Err(DensityError::WarpDidNotConverge {
                method: "diffusion",
            });
        };
        points = trial;
        time += step;
    }
    Err(DensityError::WarpDidNotConverge {
        method: "diffusion",
    })
}

fn sample_vertex_density(analysis: &DensityAnalysis, point: TopologyPosition) -> f64 {
    let [nx, ny] = analysis.grid_size;
    let [min_x, min_y, max_x, max_y] = analysis.bounds;
    let x = ((point.x - min_x) / (max_x - min_x) * nx as f64).clamp(0.0, nx as f64);
    let y = ((point.y - min_y) / (max_y - min_y) * ny as f64).clamp(0.0, ny as f64);
    let column = (x.floor() as usize).min(nx - 1);
    let row = (y.floor() as usize).min(ny - 1);
    let fx = x - column as f64;
    let fy = y - row as f64;
    let base = row * (nx + 1) + column;
    let values = &analysis.vertex_density;
    values[base] * (1.0 - fx) * (1.0 - fy)
        + values[base + 1] * fx * (1.0 - fy)
        + values[base + nx + 1] * (1.0 - fx) * fy
        + values[base + nx + 2] * fx * fy
}

struct HeatSpectrum {
    nx: usize,
    ny: usize,
    eigenvalues: Vec<f64>,
    coefficients: Vec<f64>,
    x_forward: std::sync::Arc<dyn rustdct::TransformType2And3<f64>>,
    y_forward: std::sync::Arc<dyn rustdct::TransformType2And3<f64>>,
}

impl HeatSpectrum {
    fn new(nx: usize, ny: usize, dx: f64, dy: f64, values: Vec<f64>) -> Self {
        let mut planner = DctPlanner::<f64>::new();
        let x_forward = planner.plan_dct2(nx);
        let y_forward = planner.plan_dct2(ny);
        let mut spectrum = Self {
            nx,
            ny,
            eigenvalues: Vec::with_capacity(nx * ny),
            coefficients: values,
            x_forward,
            y_forward,
        };
        transform_2d(
            &mut spectrum.coefficients,
            nx,
            ny,
            &spectrum.x_forward,
            &spectrum.y_forward,
            true,
        );
        for row in 0..ny {
            for column in 0..nx {
                let kx =
                    2.0 * (std::f64::consts::PI * column as f64 / (2.0 * nx as f64)).sin() / dx;
                let ky = 2.0 * (std::f64::consts::PI * row as f64 / (2.0 * ny as f64)).sin() / dy;
                spectrum.eigenvalues.push(kx * kx + ky * ky);
            }
        }
        spectrum
    }

    fn field(&self, time: f64) -> Result<Vec<f64>, DensityError> {
        let mut values: Vec<_> = self
            .coefficients
            .iter()
            .zip(&self.eigenvalues)
            .map(|(&coefficient, &lambda)| coefficient * (-lambda * time).exp())
            .collect();
        transform_2d(
            &mut values,
            self.nx,
            self.ny,
            &self.x_forward,
            &self.y_forward,
            false,
        );
        let divisor = self.nx as f64 * self.ny as f64 / 4.0;
        for value in &mut values {
            *value /= divisor;
            if !value.is_finite() || *value <= 0.0 {
                return Err(DensityError::InvalidWarp(
                    "diffused density is non-positive",
                ));
            }
        }
        Ok(values)
    }
}

fn transform_2d(
    values: &mut [f64],
    nx: usize,
    ny: usize,
    x_plan: &std::sync::Arc<dyn rustdct::TransformType2And3<f64>>,
    y_plan: &std::sync::Arc<dyn rustdct::TransformType2And3<f64>>,
    forward: bool,
) {
    for row in values.chunks_exact_mut(nx) {
        if forward {
            x_plan.process_dct2(row);
        } else {
            x_plan.process_dct3(row);
        }
    }
    let mut column = vec![0.0; ny];
    for x in 0..nx {
        for y in 0..ny {
            column[y] = values[y * nx + x];
        }
        if forward {
            y_plan.process_dct2(&mut column);
        } else {
            y_plan.process_dct3(&mut column);
        }
        for y in 0..ny {
            values[y * nx + x] = column[y];
        }
    }
}

fn velocity_field(
    density: &[f64],
    nx: usize,
    ny: usize,
    dx: f64,
    dy: f64,
) -> Result<Vec<[f64; 2]>, DensityError> {
    let mut velocity = Vec::with_capacity(density.len());
    for row in 0..ny {
        for column in 0..nx {
            let index = row * nx + column;
            let horizontal = if column == 0 || column + 1 == nx {
                0.0
            } else {
                (density[index + 1] - density[index - 1]) / (2.0 * dx)
            };
            let vertical = if row == 0 || row + 1 == ny {
                0.0
            } else {
                (density[index + nx] - density[index - nx]) / (2.0 * dy)
            };
            let point = [-horizontal / density[index], -vertical / density[index]];
            if point.iter().any(|v| !v.is_finite()) {
                return Err(DensityError::NumericRange);
            }
            velocity.push(point);
        }
    }
    Ok(velocity)
}

fn sample_velocity(
    velocity: &[[f64; 2]],
    point: TopologyPosition,
    bounds: [f64; 4],
    nx: usize,
    ny: usize,
) -> [f64; 2] {
    let [min_x, min_y, max_x, max_y] = bounds;
    let x = ((point.x - min_x) / (max_x - min_x) * nx as f64 - 0.5).clamp(0.0, (nx - 1) as f64);
    let y = ((point.y - min_y) / (max_y - min_y) * ny as f64 - 0.5).clamp(0.0, (ny - 1) as f64);
    let column = (x.floor() as usize).min(nx - 1);
    let row = (y.floor() as usize).min(ny - 1);
    let fx = x - column as f64;
    let fy = y - row as f64;
    let mut output = [0.0; 2];
    for (xx, wx) in [(column, 1.0 - fx), ((column + 1).min(nx - 1), fx)] {
        for (yy, wy) in [(row, 1.0 - fy), ((row + 1).min(ny - 1), fy)] {
            for (component, value) in output.iter_mut().enumerate() {
                *value += wx * wy * velocity[yy * nx + xx][component];
            }
        }
    }
    if point.x <= min_x || point.x >= max_x {
        output[0] = 0.0;
    }
    if point.y <= min_y || point.y >= max_y {
        output[1] = 0.0;
    }
    output
}

fn diagnostics(
    analysis: &DensityAnalysis,
    warp: &DensityWarp,
    stations: &[WarpedStation],
    alpha: f64,
) -> Result<DensityWarpDiagnostics, DensityError> {
    let area =
        (analysis.bounds[2] - analysis.bounds[0]) * (analysis.bounds[3] - analysis.bounds[1]);
    let mut maximum_area_residual = 0.0_f64;
    let mut minimum_area_ratio = f64::INFINITY;
    let mut maximum_condition_number = 0.0_f64;
    for triangle in &analysis.triangles {
        let target = area
            * ((1.0 - alpha) * triangle.area / area + alpha * triangle.mass / analysis.total_mass);
        let output = triangle.vertices.map(|index| warp.vertices[index]);
        let area_ratio = signed_area(output) / triangle.area;
        maximum_area_residual =
            maximum_area_residual.max((area_ratio * triangle.area / target).ln().abs());
        minimum_area_ratio = minimum_area_ratio.min(area_ratio);
        let input = triangle.vertices.map(|index| analysis.vertices[index]);
        let condition = affine_condition_number(input, output);
        maximum_condition_number = maximum_condition_number.max(condition);
    }
    let displacements: Vec<_> = stations
        .iter()
        .map(|station| squared_distance(station.source, station.warped).sqrt())
        .collect();
    let result = DensityWarpDiagnostics {
        maximum_area_residual,
        minimum_area_ratio: if minimum_area_ratio.is_finite() {
            minimum_area_ratio
        } else {
            1.0
        },
        maximum_condition_number,
        mean_station_displacement: displacements.iter().sum::<f64>()
            / displacements.len().max(1) as f64,
        maximum_station_displacement: displacements.into_iter().fold(0.0_f64, f64::max),
    };
    if [
        result.maximum_area_residual,
        result.minimum_area_ratio,
        result.maximum_condition_number,
        result.mean_station_displacement,
        result.maximum_station_displacement,
    ]
    .iter()
    .any(|value| !value.is_finite())
    {
        return Err(DensityError::NumericRange);
    }
    Ok(result)
}

fn affine_condition_number(input: [TopologyPosition; 3], output: [TopologyPosition; 3]) -> f64 {
    let [p, q, r] = input;
    let [u, v, w] = output;
    let determinant = cross(p, q, r);
    let inverse = [
        (r.y - p.y) / determinant,
        -(r.x - p.x) / determinant,
        -(q.y - p.y) / determinant,
        (q.x - p.x) / determinant,
    ];
    let j = [
        (v.x - u.x) * inverse[0] + (w.x - u.x) * inverse[2],
        (v.x - u.x) * inverse[1] + (w.x - u.x) * inverse[3],
        (v.y - u.y) * inverse[0] + (w.y - u.y) * inverse[2],
        (v.y - u.y) * inverse[1] + (w.y - u.y) * inverse[3],
    ];
    let norm_squared = j.iter().map(|value| value * value).sum::<f64>();
    let area_ratio = signed_area(output) / signed_area(input);
    let discriminant = (norm_squared * norm_squared - 4.0 * area_ratio * area_ratio).max(0.0);
    (norm_squared + discriminant.sqrt()) / (2.0 * area_ratio)
}

fn squared_distance(a: TopologyPosition, b: TopologyPosition) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MetroTopology;

    fn example() -> MetroTopology {
        MetroTopology::from_yaml(include_str!("../../../examples/topology.yaml")).unwrap()
    }

    #[test]
    fn heat_spectrum_round_trips_initial_density() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 2.0, 3.0, 4.0, 5.0, 3.0, 4.0, 5.0, 6.0];
        let spectrum = HeatSpectrum::new(4, 3, 1.0, 1.0, values.clone());
        let recovered = spectrum.field(0.0).unwrap();
        for (actual, expected) in recovered.iter().zip(values) {
            assert!((actual - expected).abs() < 1e-10);
        }
    }

    #[test]
    fn both_methods_keep_a_valid_mesh_and_warp_network_segments() {
        let source = example();
        for method in [
            DensityWarpMethod::Diffusion,
            DensityWarpMethod::TriangleArea,
        ] {
            let mut generation = GenerationManifest::default();
            generation.density_reshape.method = method;
            let result = analyze_density_warp(&source, &generation).unwrap();
            assert_eq!(result, analyze_density_warp(&source, &generation).unwrap());
            assert_eq!(result.method, method);
            assert_eq!(result.equalization_strength, 0.5);
            assert!(result.diagnostics.minimum_area_ratio > 0.0);
            assert!(
                result
                    .stations
                    .iter()
                    .any(|station| station.source != station.warped)
            );
            assert!(
                result
                    .segments
                    .iter()
                    .all(|segment| segment.points.len() >= 2)
            );
            assert!(result.segments.iter().any(|segment| {
                let first = segment.points[0];
                let last = *segment.points.last().unwrap();
                segment
                    .points
                    .iter()
                    .any(|&point| cross(first, last, point).abs() > 1e-6)
            }));
        }
    }

    #[test]
    fn zero_strength_is_exact_identity_for_both_methods() {
        for method in [
            DensityWarpMethod::Diffusion,
            DensityWarpMethod::TriangleArea,
        ] {
            let mut generation = GenerationManifest::default();
            generation.density_reshape.method = method;
            generation.density_reshape.equalization_strength = 0.0;
            let result = analyze_density_warp(&example(), &generation).unwrap();
            assert_eq!(result.source_vertices, result.warped_vertices);
            assert!(
                result
                    .stations
                    .iter()
                    .all(|station| station.source == station.warped)
            );
        }
    }

    #[test]
    fn uniform_density_stays_near_identity_and_two_centers_expand() {
        let source = example().canonicalize_coordinates().unwrap();
        let baseline = analyze_canonical_density(&source, &GenerationManifest::default()).unwrap();
        let [left, top, right, bottom] = baseline.bounds;
        let width = right - left;
        let height = bottom - top;
        let centres = [
            TopologyPosition {
                x: left + 0.35 * width,
                y: top + 0.45 * height,
            },
            TopologyPosition {
                x: left + 0.65 * width,
                y: top + 0.55 * height,
            },
        ];
        for method in [
            DensityWarpMethod::Diffusion,
            DensityWarpMethod::TriangleArea,
        ] {
            let mut uniform = baseline.clone();
            uniform.vertex_density.fill(1.0);
            for triangle in &mut uniform.triangles {
                triangle.mass = triangle.area;
            }
            uniform.total_mass = width * height;
            let identity = DensityWarp::build(&uniform, method, 0.5).unwrap();
            let displacement = identity
                .source
                .iter()
                .zip(&identity.vertices)
                .map(|(&a, &b)| squared_distance(a, b).sqrt())
                .fold(0.0_f64, f64::max);
            assert!(
                displacement < 1e-4 * width.max(height),
                "{method:?}: {displacement}"
            );

            let mut clustered = baseline.clone();
            let radius = width.min(height) * 0.12;
            clustered.vertex_density = clustered
                .vertices
                .iter()
                .map(|&point| {
                    1.0 + centres
                        .iter()
                        .map(|&centre| {
                            20.0 * (-squared_distance(point, centre) / (2.0 * radius * radius))
                                .exp()
                        })
                        .sum::<f64>()
                })
                .collect();
            for triangle in &mut clustered.triangles {
                triangle.mass = triangle.area
                    * triangle
                        .vertices
                        .iter()
                        .map(|&index| clustered.vertex_density[index])
                        .sum::<f64>()
                    / 3.0;
            }
            clustered.total_mass = clustered.triangles.iter().map(|t| t.mass).sum();
            let warp = DensityWarp::build(&clustered, method, 0.5).unwrap();
            let local_ratio = |point: TopologyPosition| {
                let centroid = |indices: [usize; 3]| {
                    weighted(
                        clustered.vertices[indices[0]],
                        clustered.vertices[indices[1]],
                        clustered.vertices[indices[2]],
                        [1.0 / 3.0; 3],
                    )
                };
                let triangle = clustered
                    .triangles
                    .iter()
                    .min_by(|a, b| {
                        squared_distance(centroid(a.vertices), point)
                            .total_cmp(&squared_distance(centroid(b.vertices), point))
                    })
                    .unwrap();
                signed_area(triangle.vertices.map(|index| warp.vertices[index])) / triangle.area
            };
            let sparse = TopologyPosition {
                x: left + 0.5 * width,
                y: top + 0.15 * height,
            };
            assert!(local_ratio(centres[0]) > local_ratio(sparse));
            assert!(local_ratio(centres[1]) > local_ratio(sparse));
        }
    }

    #[test]
    fn detects_nonlocal_triangle_overlap() {
        let first = [
            TopologyPosition { x: 0.0, y: 0.0 },
            TopologyPosition { x: 2.0, y: 0.0 },
            TopologyPosition { x: 0.0, y: 2.0 },
        ];
        let second = [
            TopologyPosition { x: 0.5, y: 0.5 },
            TopologyPosition { x: 2.5, y: 0.5 },
            TopologyPosition { x: 0.5, y: 2.5 },
        ];
        assert!(triangles_overlap(first, second));
    }

    #[test]
    fn rejects_reordered_boundary_vertices() {
        let source = example().canonicalize_coordinates().unwrap();
        let analysis = analyze_canonical_density(&source, &GenerationManifest::default()).unwrap();
        let mut warp = DensityWarp::build(&analysis, DensityWarpMethod::Diffusion, 0.0).unwrap();
        warp.vertices[1].x = warp.vertices[2].x + 1.0;
        assert!(matches!(
            warp.validate(),
            Err(DensityError::InvalidWarp("boundary order changed"))
        ));
    }

    #[test]
    fn handles_empty_and_single_station_topologies() {
        let mut source = example();
        source.lines.clear();
        source.stations.clear();
        let empty = analyze_density_warp(&source, &GenerationManifest::default()).unwrap();
        assert!(empty.stations.is_empty());
        assert!(empty.segments.is_empty());
        assert_eq!(empty.source_vertices, empty.warped_vertices);

        source.stations = example().stations.into_iter().take(1).collect();
        let single = analyze_density_warp(&source, &GenerationManifest::default()).unwrap();
        assert_eq!(single.stations.len(), 1);
        assert!(single.diagnostics.minimum_area_ratio > 0.0);
    }
}
