use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::LocalizedNames;

/// Languages required for every station and line name map.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Languages {
    pub set: BTreeSet<String>,
    pub primary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LanguageError {
    #[error("languages.set must not be empty or contain an empty language")]
    InvalidSet,
    #[error("languages.primary '{0}' must belong to languages.set")]
    InvalidPrimary(String),
    #[error("languages.secondary '{0}' must belong to languages.set and differ from primary")]
    InvalidSecondary(String),
    #[error(
        "{kind} '{id}' languages differ from options.languages.set (missing: {missing:?}, unexpected: {unexpected:?})"
    )]
    NameLanguages {
        kind: &'static str,
        id: String,
        missing: Vec<String>,
        unexpected: Vec<String>,
    },
    #[error("{kind} '{id}' language '{language}' has no non-empty canonical name")]
    EmptyCanonicalName {
        kind: &'static str,
        id: String,
        language: String,
    },
}

impl Languages {
    pub(crate) fn validate(&self) -> Result<(), LanguageError> {
        if self.set.is_empty() || self.set.iter().any(|language| language.trim().is_empty()) {
            return Err(LanguageError::InvalidSet);
        }
        if !self.set.contains(&self.primary) {
            return Err(LanguageError::InvalidPrimary(self.primary.clone()));
        }
        if let Some(secondary) = &self.secondary
            && (!self.set.contains(secondary) || secondary == &self.primary)
        {
            return Err(LanguageError::InvalidSecondary(secondary.clone()));
        }
        Ok(())
    }

    pub(crate) fn validate_names(
        &self,
        kind: &'static str,
        id: &str,
        names: &LocalizedNames,
    ) -> Result<(), LanguageError> {
        let missing = self
            .set
            .iter()
            .filter(|language| !names.contains_key(*language))
            .cloned()
            .collect::<Vec<_>>();
        let unexpected = names
            .keys()
            .filter(|language| !self.set.contains(*language))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() || !unexpected.is_empty() {
            return Err(LanguageError::NameLanguages {
                kind,
                id: id.to_owned(),
                missing,
                unexpected,
            });
        }
        for (language, values) in names {
            if values.first().is_none_or(|value| value.trim().is_empty()) {
                return Err(LanguageError::EmptyCanonicalName {
                    kind,
                    id: id.to_owned(),
                    language: language.clone(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn languages() -> Languages {
        Languages {
            set: ["de-ch".into(), "en".into()].into(),
            primary: "de-ch".into(),
            secondary: Some("en".into()),
        }
    }

    #[test]
    fn validates_language_options_and_exact_name_keys() {
        let options = languages();
        let names = [
            ("en".into(), vec!["Zurich".into()]),
            ("de-ch".into(), vec!["Zürich".into()]),
        ]
        .into();
        assert_eq!(options.validate(), Ok(()));
        assert_eq!(options.validate_names("station", "z", &names), Ok(()));

        let mut invalid = options.clone();
        invalid.set.clear();
        assert_eq!(invalid.validate(), Err(LanguageError::InvalidSet));
        invalid = options.clone();
        invalid.primary = "fr-ch".into();
        assert_eq!(
            invalid.validate(),
            Err(LanguageError::InvalidPrimary("fr-ch".into()))
        );
        invalid = options.clone();
        invalid.secondary = Some("de-ch".into());
        assert_eq!(
            invalid.validate(),
            Err(LanguageError::InvalidSecondary("de-ch".into()))
        );

        let mut invalid_names = names.clone();
        invalid_names.remove("en");
        invalid_names.insert("fr-ch".into(), vec!["Zurich".into()]);
        assert_eq!(
            options.validate_names("station", "z", &invalid_names),
            Err(LanguageError::NameLanguages {
                kind: "station",
                id: "z".into(),
                missing: vec!["en".into()],
                unexpected: vec!["fr-ch".into()],
            })
        );
        invalid_names = names;
        invalid_names.insert("en".into(), vec![]);
        assert_eq!(
            options.validate_names("line", "l", &invalid_names),
            Err(LanguageError::EmptyCanonicalName {
                kind: "line",
                id: "l".into(),
                language: "en".into(),
            })
        );
    }

    #[test]
    fn round_trips_optional_secondary_in_yaml_and_json() {
        let options = languages();
        let yaml = serde_yaml::to_string(&options).unwrap();
        let json = serde_json::to_string(&options).unwrap();
        assert_eq!(serde_yaml::from_str::<Languages>(&yaml).unwrap(), options);
        assert_eq!(serde_json::from_str::<Languages>(&json).unwrap(), options);
        let without_secondary = "set: [en]\nprimary: en\n";
        let decoded = serde_yaml::from_str::<Languages>(without_secondary).unwrap();
        assert_eq!(decoded.secondary, None);
        assert!(
            !serde_json::to_string(&decoded)
                .unwrap()
                .contains("secondary")
        );
        assert!(serde_yaml::from_str::<Languages>("set: [en]\nsecondary: en\n").is_err());
        assert!(serde_yaml::from_str::<Languages>("set: [en]\nprimary: en\nextra: x\n").is_err());
    }
}
