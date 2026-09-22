use serde::{Deserialize, Deserializer, Serialize};

use crate::{DensityError, manifest_format::canonicalize_yaml};

/// Density estimation settings for the pre-layout triangular mesh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DensityReshapeOptions {
    #[serde(default)]
    pub estimator: DensityEstimator,
    #[serde(default)]
    pub method: DensityWarpMethod,
    #[serde(
        default = "default_equalization_strength",
        alias = "equalisation-strength"
    )]
    pub equalization_strength: f64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "scalar"
    )]
    pub bandwidth: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "scalar"
    )]
    pub mesh_cell_size: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "scalar"
    )]
    pub raster_pixel_size: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "scalar"
    )]
    pub padding: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "scalar"
    )]
    pub station_weight: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "scalar"
    )]
    pub segment_weight: Option<f64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "scalar"
    )]
    pub density_floor: Option<f64>,
}

impl Default for DensityReshapeOptions {
    fn default() -> Self {
        Self {
            estimator: DensityEstimator::default(),
            method: DensityWarpMethod::default(),
            equalization_strength: default_equalization_strength(),
            bandwidth: None,
            mesh_cell_size: None,
            raster_pixel_size: None,
            padding: None,
            station_weight: None,
            segment_weight: None,
            density_floor: None,
        }
    }
}

fn scalar<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    f64::deserialize(deserializer).map(Some)
}

impl DensityReshapeOptions {
    pub(crate) fn validate_scalars(&self) -> Result<(), (&'static str, &'static str)> {
        if !self.equalization_strength.is_finite()
            || !(0.0..=1.0).contains(&self.equalization_strength)
        {
            return Err(("equalization-strength", "finite and between 0 and 1"));
        }
        for (name, value, allows_zero) in [
            ("bandwidth", self.bandwidth, false),
            ("mesh-cell-size", self.mesh_cell_size, false),
            ("raster-pixel-size", self.raster_pixel_size, false),
            ("padding", self.padding, false),
            ("station-weight", self.station_weight, true),
            ("segment-weight", self.segment_weight, true),
            ("density-floor", self.density_floor, false),
        ] {
            if value
                .is_some_and(|raw| !raw.is_finite() || raw < 0.0 || (raw == 0.0 && !allows_zero))
            {
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
        if self.station_weight == Some(0.0) && self.segment_weight == Some(0.0) {
            return Err((
                "station-weight/segment-weight",
                "at least one positive demand weight",
            ));
        }
        Ok(())
    }
}

fn default_equalization_strength() -> f64 {
    0.5
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DensityWarpMethod {
    #[default]
    Diffusion,
    TriangleArea,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DensityEstimator {
    #[default]
    VertexKde,
    TriangleQuadrature,
    RasterConvolution,
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
            "density-reshape:\n  estimator: raster-convolution\n  bandwidth: 12.0\n",
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
        assert!(
            GenerationManifest::from_yaml("density-reshape: {bandwidth: {factor: 2.0}}").is_err()
        );
        assert!(
            GenerationManifest::from_yaml("density-reshape: {bandwidth: {exact: 2.0}}").is_err()
        );
        assert!(GenerationManifest::from_yaml("density-reshape: {bandwidth: null}").is_err());
        assert!(
            GenerationManifest::from_json("{\"density-reshape\":{\"bandwidth\":null}}").is_err()
        );
        assert_eq!(
            GenerationManifest::from_yaml(include_str!("../examples/generation.yaml")).unwrap(),
            GenerationManifest::default()
        );
    }

    #[test]
    fn validates_density_controls_independently_of_topology() {
        let config =
            GenerationManifest::from_yaml("density-reshape:\n  bandwidth: -1.0\n").unwrap();
        assert!(matches!(
            config.validate(),
            Err(DensityError::InvalidParameter {
                name: "bandwidth",
                ..
            })
        ));
        for value in ["-0.1", "1.1", ".nan"] {
            let config = GenerationManifest::from_yaml(&format!(
                "density-reshape: {{equalization-strength: {value}}}"
            ))
            .unwrap();
            assert!(matches!(
                config.validate(),
                Err(DensityError::InvalidParameter {
                    name: "equalization-strength",
                    ..
                })
            ));
        }
    }

    #[test]
    fn method_and_strength_have_canonical_defaults_and_british_input_alias() {
        let defaults = GenerationManifest::default();
        assert_eq!(
            defaults.density_reshape.method,
            DensityWarpMethod::Diffusion
        );
        assert_eq!(defaults.density_reshape.equalization_strength, 0.5);
        let alias = GenerationManifest::from_yaml(
            "density-reshape: {method: triangle-area, equalisation-strength: 0.25}",
        )
        .unwrap();
        assert_eq!(
            alias.density_reshape.method,
            DensityWarpMethod::TriangleArea
        );
        assert_eq!(alias.density_reshape.equalization_strength, 0.25);
        let canonical = alias.to_yaml().unwrap();
        assert!(canonical.contains("equalization-strength: 0.25"));
        assert!(!canonical.contains("equalisation-strength"));
        assert!(
            GenerationManifest::from_yaml(
                "density-reshape: {method: other, equalization-strength: 0.5}"
            )
            .is_err()
        );
    }
}
