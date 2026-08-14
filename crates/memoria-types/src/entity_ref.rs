use std::{fmt, str::FromStr};

const MAX_NAMESPACE_BYTES: usize = 64;
const MAX_OPAQUE_BYTES: usize = 384;
const MAX_ENTITY_REF_BYTES: usize = 512;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityRef(String);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityRefError {
    value: String,
}

impl EntityRefError {
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for EntityRefError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid entity reference `{}`", self.value)
    }
}

impl std::error::Error for EntityRefError {}

impl EntityRef {
    pub fn new(value: impl Into<String>) -> Result<Self, EntityRefError> {
        let value = value.into();
        validate(&value).map_err(|()| EntityRefError {
            value: value.clone(),
        })?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for EntityRef {
    type Err = EntityRefError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

fn validate(value: &str) -> Result<(), ()> {
    if value.is_empty() || value.len() > MAX_ENTITY_REF_BYTES {
        return Err(());
    }
    let Some((namespace, opaque)) = value.split_once(':') else {
        return Err(());
    };
    if namespace.is_empty()
        || namespace.len() > MAX_NAMESPACE_BYTES
        || !namespace
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_lowercase)
        || !namespace
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || opaque.is_empty()
        || opaque.len() > MAX_OPAQUE_BYTES
        || opaque.chars().any(|character| {
            character.is_control() || character.is_whitespace() || character == ':'
        })
    {
        return Err(());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::EntityRef;

    #[test]
    fn accepts_the_locked_grammar() {
        assert!(EntityRef::new("person:7K4Q").is_ok());
        assert!(EntityRef::new("org-1:对象/42").is_ok());
    }

    #[test]
    fn rejects_invalid_namespace_and_opaque_values() {
        for value in [
            ":opaque",
            "Person:opaque",
            "person_name:opaque",
            "person.:opaque",
            "person:",
            "person:has:colon",
            "person:has whitespace",
            "person:\u{0000}",
        ] {
            assert!(EntityRef::new(value).is_err(), "accepted {value:?}");
        }
    }
}
