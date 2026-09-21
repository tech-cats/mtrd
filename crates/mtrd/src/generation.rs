use serde::{Deserialize, Serialize};

use crate::{DensityError, manifest_format::canonicalize_yaml};

/// Density estimation settings for the pre-layout triangular mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DensityReshapeOptions {
    #[serde(default)]
    pub estimator: DensityEstimator,
    #[serde(default = "DensityValue::factor_one")]
    pub bandwidth: DensityValue,
    #[serde(default = "DensityValue::factor_one")]
    pub mesh_cell_size: DensityValue,
    #[serde(default = "DensityValue::factor_one")]
    pub raster_pixel_size: DensityValue,
    #[serde(default = "DensityValue::factor_one")]
    pub padding: DensityValue,
    #[serde(default = "DensityValue::factor_one")]
    pub station_weight: DensityValue,
    #[serde(default = "DensityValue::factor_one")]
    pub segment_weight: DensityValue,
    #[serde(default = "DensityValue::factor_one")]
    pub density_floor: DensityValue,
}

impl Default for DensityReshapeOptions {
    fn default() -> Self {
        Self {
            estimator: DensityEstimator::default(),
            bandwidth: DensityValue::factor_one(),
            mesh_cell_size: DensityValue::factor_one(),
            raster_pixel_size: DensityValue::factor_one(),
            padding: DensityValue::factor_one(),
            station_weight: DensityValue::factor_one(),
            segment_weight: DensityValue::factor_one(),
            density_floor: DensityValue::factor_one(),
        }
    }
}

impl DensityReshapeOptions {
    pub(crate) fn validate_scalars(&self) -> Result<(), (&'static str, &'static str)> {
        for (name, value, allows_zero) in [
            ("bandwidth", self.bandwidth, false),
            ("mesh-cell-size", self.mesh_cell_size, false),
            ("raster-pixel-size", self.raster_pixel_size, false),
            ("padding", self.padding, false),
            ("station-weight", self.station_weight, true),
            ("segment-weight", self.segment_weight, true),
            ("density-floor", self.density_floor, false),
        ] {
            let raw = value.raw();
            if !raw.is_finite() || raw < 0.0 || (raw == 0.0 && !allows_zero) {
                return Err((
                    name,
                    if allows_zero {
                        "finite and nonnegative"
                    } else {
                        "finite and strictly positive"
                    },
                ));
            }
        }
        if self.station_weight.raw() == 0.0 && self.segment_weight.raw() == 0.0 {
            return Err((
                "station-weight/segment-weight",
                "at least one positive demand weight",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DensityEstimator {
    #[default]
    VertexKde,
    TriangleQuadrature,
    RasterConvolution,
}

/// An exact canonical-coordinate value or a multiplier of its derived default.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DensityValue {
    Exact(DensityExact),
    Factor(DensityFactor),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DensityExact {
    pub exact: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DensityFactor {
    pub factor: f64,
}

impl DensityValue {
    pub const fn factor_one() -> Self {
        Self::Factor(DensityFactor { factor: 1.0 })
    }

    pub fn resolve(self, derived: f64) -> f64 {
        match self {
            Self::Exact(value) => value.exact,
            Self::Factor(value) => value.factor * derived,
        }
    }

    fn raw(self) -> f64 {
        match self {
            Self::Exact(value) => value.exact,
            Self::Factor(value) => value.factor,
        }
    }
}

/// Settings for topology-to-schematic generation, stored separately from topology.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct GenerationManifest {
    #[serde(default)]
    pub density_reshape: DensityReshapeOptions,
}

impl GenerationManifest {
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self).map(canonicalize_yaml)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn validate(&self) -> Result<(), DensityError> {
        self.density_reshape
            .validate_scalars()
            .map_err(|(name, requirement)| DensityError::InvalidParameter { name, requirement })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_unknown_generation_fields() {
        let config = GenerationManifest::from_yaml(
            "density-reshape:\n  estimator: raster-convolution\n  bandwidth: { exact: 12.0 }\n",
        )
        .unwrap();
        assert_eq!(
            GenerationManifest::from_yaml(&config.to_yaml().unwrap()).unwrap(),
            config
        );
        assert_eq!(
            GenerationManifest::from_json(&config.to_json().unwrap()).unwrap(),
            config
        );
        assert!(GenerationManifest::from_yaml("density_reshape: {}").is_err());
        assert!(GenerationManifest::from_yaml("unknown: true").is_err());
        assert!(GenerationManifest::from_yaml("density-reshape: {unknown: true}").is_err());
        assert_eq!(
            GenerationManifest::from_yaml(include_str!("../examples/generation.yaml")).unwrap(),
            GenerationManifest::default()
        );
    }

    #[test]
    fn validates_density_controls_independently_of_topology() {
        let config =
            GenerationManifest::from_yaml("density-reshape:\n  bandwidth: { exact: -1.0 }\n")
                .unwrap();
        assert!(matches!(
            config.validate(),
            Err(DensityError::InvalidParameter {
                name: "bandwidth",
                ..
            })
        ));
    }
}
