use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Current version of the `meta.json` schema.
pub const META_SCHEMA_VERSION: u32 = 1;

/// Contents of `<scene>.meta.json`.
///
/// The file is the source of truth for markers, eases and tunables; scene
/// code only provides defaults. Unknown keys are preserved verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Meta {
    /// Schema version, [`META_SCHEMA_VERSION`].
    pub version: u32,
    #[serde(default)]
    /// Marker times in seconds from the start of the scene.
    pub markers: BTreeMap<String, f64>,
    #[serde(default)]
    /// Named eases as `[x1, y1, x2, y2]`.
    pub eases: BTreeMap<String, [f64; 4]>,
    #[serde(default)]
    /// Tunable values.
    pub tunables: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Soundtrack placement.
    pub audio: Option<AudioMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Incoming transition length, edited on the timeline.
    pub transition: Option<TransitionMeta>,
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(skip))]
    /// Keys this version does not know, kept as they are.
    pub extra: BTreeMap<String, Value>,
}

/// Length of the transition into this scene, edited on the timeline.
///
/// Absent on scenes that do not start with one. An older `meta.json` has
/// no such key and still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TransitionMeta {
    /// Blend length in seconds.
    pub seconds: f64,
}

/// Where the soundtrack plays.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AudioMeta {
    /// Audio file, relative to the project.
    pub file: String,
    #[serde(default)]
    /// Start of the audio in seconds on the project timeline.
    pub offset: f64,
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(skip))]
    /// Keys this version does not know, kept as they are.
    pub extra: BTreeMap<String, Value>,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            version: META_SCHEMA_VERSION,
            markers: BTreeMap::new(),
            eases: BTreeMap::new(),
            tunables: BTreeMap::new(),
            audio: None,
            transition: None,
            extra: BTreeMap::new(),
        }
    }
}

/// Why a `meta.json` could not be used.
#[derive(Debug)]
pub enum MetaError {
    /// Not valid JSON, or wrong types.
    Json(serde_json::Error),
    /// Written by a newer oxframe.
    UnsupportedVersion(u32),
    /// A name does not match `[a-z0-9_-]+`.
    InvalidName {
        /// `"markers"`, `"eases"` or `"tunables"`.
        section: &'static str,
        /// The offending name.
        name: String,
    },
}

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "invalid meta.json: {e}"),
            Self::UnsupportedVersion(v) => write!(
                f,
                "meta.json schema version {v} is not supported (expected {META_SCHEMA_VERSION})"
            ),
            Self::InvalidName { section, name } => write!(
                f,
                "invalid name {name:?} in `{section}`: names must match [a-z0-9_-]+"
            ),
        }
    }
}

impl std::error::Error for MetaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

/// Names of markers, eases and tunables: `[a-z0-9_-]+`.
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

impl Meta {
    /// Parses and validates a `meta.json` file.
    ///
    /// # Errors
    ///
    /// If the text is not valid JSON of this shape, has another schema
    /// version, or contains an invalid name.
    pub fn parse(text: &str) -> Result<Self, MetaError> {
        let meta: Meta = serde_json::from_str(text).map_err(MetaError::Json)?;
        if meta.version != META_SCHEMA_VERSION {
            return Err(MetaError::UnsupportedVersion(meta.version));
        }
        meta.validate()?;
        Ok(meta)
    }

    /// Checks that all names are valid.
    ///
    /// # Errors
    ///
    /// [`MetaError::InvalidName`] for the first name that is not `[a-z0-9_-]+`.
    pub fn validate(&self) -> Result<(), MetaError> {
        let sections: [(&'static str, Vec<&String>); 3] = [
            ("markers", self.markers.keys().collect()),
            ("eases", self.eases.keys().collect()),
            ("tunables", self.tunables.keys().collect()),
        ];
        for (section, names) in sections {
            if let Some(bad) = names.into_iter().find(|n| !is_valid_name(n)) {
                return Err(MetaError::InvalidName {
                    section,
                    name: bad.clone(),
                });
            }
        }
        Ok(())
    }

    /// Deterministic formatting: keys sorted at every level, two-space
    /// indent, trailing newline — so diffs in git stay readable.
    pub fn to_pretty_string(&self) -> String {
        // Going through `Value` sorts the top-level keys too (serde_json's
        // default map is a `BTreeMap`).
        let value = serde_json::to_value(self).expect("Meta is always serializable");
        let mut out = serde_json::to_string_pretty(&value).expect("Value is always serializable");
        out.push('\n');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
  "version": 1,
  "markers":  { "beat": 1.25 },
  "eases":    { "slide": [0.65, 0, 0.35, 1] },
  "tunables": { "speed": 1.4 },
  "audio":    { "file": "assets/voice.mp3", "offset": 0.0 }
}"#;

    #[test]
    fn parses_spec_sample() {
        let meta = Meta::parse(SAMPLE).unwrap();
        assert_eq!(meta.markers["beat"], 1.25);
        assert_eq!(meta.eases["slide"], [0.65, 0.0, 0.35, 1.0]);
        assert_eq!(meta.tunables["speed"], 1.4);
        assert_eq!(meta.audio.unwrap().file, "assets/voice.mp3");
        assert!(meta.transition.is_none());
    }

    #[test]
    fn preserves_unknown_keys_and_sorts() {
        let text =
            r#"{"zeta": {"b": 1, "a": 2}, "version": 1, "audio": {"file": "a.mp3", "gain": 0.5}}"#;
        let meta = Meta::parse(text).unwrap();
        let out = meta.to_pretty_string();
        let expected = r#"{
  "audio": {
    "file": "a.mp3",
    "gain": 0.5,
    "offset": 0.0
  },
  "eases": {},
  "markers": {},
  "tunables": {},
  "version": 1,
  "zeta": {
    "a": 2,
    "b": 1
  }
}
"#;
        assert_eq!(out, expected);
        assert_eq!(Meta::parse(&out).unwrap(), meta);
    }

    #[test]
    fn rejects_bad_names_and_versions() {
        assert!(matches!(
            Meta::parse(r#"{"version": 1, "markers": {"Beat": 1}}"#),
            Err(MetaError::InvalidName { .. })
        ));
        assert!(matches!(
            Meta::parse(r#"{"version": 2}"#),
            Err(MetaError::UnsupportedVersion(2))
        ));
    }
}
