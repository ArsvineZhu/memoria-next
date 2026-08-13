use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::DerivedError;

const TAG_ID_BYTES: usize = 32;

/// The stable global identity of a normalized Tag.
///
/// Tag identity is content-addressed so rebuilding a Derived dictionary does
/// not allocate a different identity for the same normalized value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TagId([u8; TAG_ID_BYTES]);

impl TagId {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; TAG_ID_BYTES]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; TAG_ID_BYTES] {
        &self.0
    }

    #[must_use]
    pub const fn into_bytes(self) -> [u8; TAG_ID_BYTES] {
        self.0
    }

    #[must_use]
    pub fn from_normalized(value: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"memoria-tag-id-v1\0");
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
        Self(hasher.finalize().into())
    }
}

impl fmt::Display for TagId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("T_")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A deterministic global dictionary from human-facing Tag values to TagId.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TagDictionary {
    by_value: BTreeMap<String, TagId>,
    by_id: BTreeMap<TagId, String>,
}

impl TagDictionary {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Normalize a user or projection value and assign its stable identity.
    pub fn intern(&mut self, value: impl AsRef<str>) -> Result<TagId, DerivedError> {
        let normalized = normalize_tag(value.as_ref())?;
        if let Some(tag_id) = self.by_value.get(&normalized) {
            return Ok(*tag_id);
        }
        let tag_id = TagId::from_normalized(&normalized);
        self.by_value.insert(normalized.clone(), tag_id);
        self.by_id.insert(tag_id, normalized);
        Ok(tag_id)
    }

    #[must_use]
    pub fn resolve(&self, value: &str) -> Option<TagId> {
        normalize_tag(value)
            .ok()
            .and_then(|normalized| self.by_value.get(&normalized).copied())
    }

    #[must_use]
    pub fn value(&self, tag_id: TagId) -> Option<&str> {
        self.by_id.get(&tag_id).map(String::as_str)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_value.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_value.is_empty()
    }
}

/// Normalize whitespace and case without attempting query-time language
/// understanding. The result is suitable for deterministic Tag identity.
pub fn normalize_tag(value: &str) -> Result<String, DerivedError> {
    let mut normalized = String::new();
    let mut pending_space = false;
    for character in value.chars() {
        if character.is_whitespace() {
            if !normalized.is_empty() {
                pending_space = true;
            }
            continue;
        }
        if pending_space {
            normalized.push(' ');
            pending_space = false;
        }
        for lower in character.to_lowercase() {
            normalized.push(lower);
        }
    }
    if normalized.is_empty() {
        return Err(DerivedError::InvalidProjectionValue {
            value: "Tag value must not be empty".to_owned(),
        });
    }
    Ok(normalized)
}
