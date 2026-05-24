//! Recipe + Step — the serializable data model for a workflow.
//!
//! Recipes are JSON-portable (saved under `$APP_CONFIG/atelier/recipes/`)
//! so users can share, version, and rerun them across machines.

use serde::{Deserialize, Serialize};

/// A named, ordered chain of `Step`s applied to a single PDF (or batch).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: u32,
    pub steps: Vec<Step>,
}

fn default_version() -> u32 {
    1
}

/// One transformation applied to a PDF. New kinds are added here as the
/// underlying `pdf::*` primitives are wired through `run::apply_step`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Step {
    Ocr {
        #[serde(default = "default_lang")]
        language: String,
    },
    AutoRedact {
        #[serde(default)]
        patterns: Vec<String>,
        #[serde(default)]
        presets: Vec<String>,
    },
    Bates {
        prefix: String,
        #[serde(default = "default_bates_start")]
        start: u64,
        #[serde(default = "default_bates_digits")]
        digits: u8,
    },
    Watermark {
        text: String,
        #[serde(default = "default_opacity")]
        opacity: f32,
    },
    Flatten {
        #[serde(default = "default_flatten_dpi")]
        dpi: u32,
    },
    Compactor,
}

fn default_lang() -> String {
    "eng".into()
}
fn default_bates_start() -> u64 {
    1
}
fn default_bates_digits() -> u8 {
    6
}
fn default_opacity() -> f32 {
    0.25
}
fn default_flatten_dpi() -> u32 {
    150
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_serializes_round_trip() {
        let r = Recipe {
            name: "Nightly Discovery".into(),
            version: 1,
            steps: vec![
                Step::Ocr {
                    language: "eng".into(),
                },
                Step::AutoRedact {
                    patterns: vec![],
                    presets: vec!["ssn".into(), "email".into()],
                },
                Step::Bates {
                    prefix: "ACME".into(),
                    start: 1,
                    digits: 6,
                },
                Step::Flatten { dpi: 150 },
            ],
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Nightly Discovery");
        assert_eq!(back.steps.len(), 4);
        assert_eq!(back, r);
    }

    #[test]
    fn step_tag_is_kebab_in_json() {
        let s = Step::AutoRedact {
            patterns: vec![],
            presets: vec![],
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["kind"], "auto-redact");
    }

    #[test]
    fn recipe_version_defaults_when_missing() {
        // Forward-compat: older saved recipes without a version field still load.
        let json = r#"{"name":"Old","steps":[{"kind":"compactor"}]}"#;
        let r: Recipe = serde_json::from_str(json).unwrap();
        assert_eq!(r.version, 1);
        assert_eq!(r.steps.len(), 1);
    }

    #[test]
    fn all_step_kinds_round_trip() {
        let steps = vec![
            Step::Ocr {
                language: "deu".into(),
            },
            Step::AutoRedact {
                patterns: vec![r"\bACME-\d+\b".into()],
                presets: vec!["email".into()],
            },
            Step::Bates {
                prefix: "X".into(),
                start: 100,
                digits: 4,
            },
            Step::Watermark {
                text: "CONFIDENTIAL".into(),
                opacity: 0.4,
            },
            Step::Flatten { dpi: 300 },
            Step::Compactor,
        ];
        for s in steps {
            let j = serde_json::to_string(&s).unwrap();
            let back: Step = serde_json::from_str(&j).unwrap();
            assert_eq!(back, s);
        }
    }
}
