use std::{fmt, str::FromStr};

use data_encoding::{BASE32_NOPAD, HEXLOWER};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::MemoriaError;

const RANDOM_ID_BYTES: usize = 16;

macro_rules! define_id {
    ($name:ident, $kind:literal, $prefix:literal, $bytes:expr) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; $bytes]);

        impl $name {
            const TEXT_PREFIX: &'static str = concat!($prefix, "_");

            #[must_use]
            pub fn new() -> Self {
                Self::try_new().expect("the operating system random source is unavailable")
            }

            pub fn try_new() -> Result<Self, MemoriaError> {
                let mut bytes = [0_u8; $bytes];
                getrandom::fill(&mut bytes).map_err(MemoriaError::Randomness)?;
                Ok(Self(bytes))
            }

            #[must_use]
            pub const fn from_bytes(bytes: [u8; $bytes]) -> Self {
                Self(bytes)
            }

            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; $bytes] {
                &self.0
            }

            #[must_use]
            pub const fn into_bytes(self) -> [u8; $bytes] {
                self.0
            }

            #[must_use]
            pub const fn prefix() -> &'static str {
                Self::TEXT_PREFIX
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(Self::TEXT_PREFIX)?;
                formatter.write_str(&BASE32_NOPAD.encode(&self.0))
            }
        }

        impl FromStr for $name {
            type Err = MemoriaError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let encoded = value.strip_prefix(Self::TEXT_PREFIX).ok_or_else(|| {
                    MemoriaError::InvalidId {
                        kind: $kind,
                        expected_prefix: Self::TEXT_PREFIX,
                        value: value.to_owned(),
                    }
                })?;
                let bytes = BASE32_NOPAD.decode(encoded.as_bytes()).map_err(|_| {
                    MemoriaError::InvalidId {
                        kind: $kind,
                        expected_prefix: Self::TEXT_PREFIX,
                        value: value.to_owned(),
                    }
                })?;
                let bytes: [u8; $bytes] =
                    bytes.try_into().map_err(|_| MemoriaError::InvalidId {
                        kind: $kind,
                        expected_prefix: Self::TEXT_PREFIX,
                        value: value.to_owned(),
                    })?;
                Ok(Self(bytes))
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

define_id!(StoreId, "store id", "ST", RANDOM_ID_BYTES);
define_id!(SpaceId, "space id", "SP", RANDOM_ID_BYTES);
define_id!(MemoryId, "memory id", "M", RANDOM_ID_BYTES);

const REVISION_ID_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RevisionId([u8; REVISION_ID_BYTES]);

impl RevisionId {
    const TEXT_PREFIX: &'static str = "R_";

    #[must_use]
    pub const fn from_bytes(bytes: [u8; REVISION_ID_BYTES]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; REVISION_ID_BYTES] {
        &self.0
    }

    #[must_use]
    pub const fn into_bytes(self) -> [u8; REVISION_ID_BYTES] {
        self.0
    }

    #[must_use]
    pub const fn prefix() -> &'static str {
        Self::TEXT_PREFIX
    }
}

impl fmt::Display for RevisionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(Self::TEXT_PREFIX)?;
        formatter.write_str(&HEXLOWER.encode(&self.0))
    }
}

impl FromStr for RevisionId {
    type Err = MemoriaError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let encoded =
            value
                .strip_prefix(Self::TEXT_PREFIX)
                .ok_or_else(|| MemoriaError::InvalidId {
                    kind: "revision id",
                    expected_prefix: Self::TEXT_PREFIX,
                    value: value.to_owned(),
                })?;
        let bytes = HEXLOWER
            .decode(encoded.as_bytes())
            .map_err(|_| MemoriaError::InvalidId {
                kind: "revision id",
                expected_prefix: Self::TEXT_PREFIX,
                value: value.to_owned(),
            })?;
        let bytes: [u8; REVISION_ID_BYTES] =
            bytes.try_into().map_err(|_| MemoriaError::InvalidId {
                kind: "revision id",
                expected_prefix: Self::TEXT_PREFIX,
                value: value.to_owned(),
            })?;
        Ok(Self(bytes))
    }
}

impl Serialize for RevisionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for RevisionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::{MemoryId, RevisionId, SpaceId, StoreId};

    #[test]
    fn each_id_type_has_a_distinct_prefix() {
        assert_ne!(StoreId::prefix(), SpaceId::prefix());
        assert_ne!(SpaceId::prefix(), MemoryId::prefix());
        assert_ne!(MemoryId::prefix(), RevisionId::prefix());
    }

    #[test]
    fn ids_round_trip_from_fixed_bytes() {
        let id = RevisionId::from_bytes([0xAB; 32]);
        assert_eq!(id.to_string().parse::<RevisionId>().unwrap(), id);
        assert_eq!(id.as_bytes(), &[0xAB; 32]);
    }

    #[test]
    fn ids_reject_wrong_prefix_and_malformed_payloads() {
        let memory = MemoryId::from_bytes([0xCD; 16]).to_string();
        assert!(memory.parse::<SpaceId>().is_err());
        assert!("M_not-an-id".parse::<MemoryId>().is_err());
    }
}
