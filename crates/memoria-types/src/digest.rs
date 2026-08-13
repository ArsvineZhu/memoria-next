use std::{fmt, str::FromStr};

use data_encoding::HEXLOWER;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::MemoriaError;

const DIGEST_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SourceBlobHash([u8; DIGEST_BYTES]);

impl SourceBlobHash {
    #[must_use]
    pub fn new(source: &[u8]) -> Self {
        Self::from_bytes(source)
    }

    #[must_use]
    pub fn from_bytes(source: &[u8]) -> Self {
        let digest = Sha256::digest(source);
        Self(digest.into())
    }

    #[must_use]
    pub const fn from_digest(digest: [u8; DIGEST_BYTES]) -> Self {
        Self(digest)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
        &self.0
    }

    #[must_use]
    pub const fn into_bytes(self) -> [u8; DIGEST_BYTES] {
        self.0
    }

    #[must_use]
    pub fn as_hex(&self) -> String {
        HEXLOWER.encode(&self.0)
    }
}

impl fmt::Display for SourceBlobHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_hex())
    }
}

impl FromStr for SourceBlobHash {
    type Err = MemoriaError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes =
            HEXLOWER
                .decode(value.as_bytes())
                .map_err(|_| MemoriaError::InvalidSourceBlobHash {
                    value: value.to_owned(),
                })?;
        let bytes: [u8; DIGEST_BYTES] =
            bytes
                .try_into()
                .map_err(|_| MemoriaError::InvalidSourceBlobHash {
                    value: value.to_owned(),
                })?;
        Ok(Self(bytes))
    }
}

impl Serialize for SourceBlobHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_hex())
    }
}

impl<'de> Deserialize<'de> for SourceBlobHash {
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
    use super::SourceBlobHash;

    #[test]
    fn source_blob_hash_is_sha256_and_round_trips_as_lower_hex() {
        let hash = SourceBlobHash::from_bytes(b"hello");
        assert_eq!(
            hash.to_string(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(hash.to_string().parse::<SourceBlobHash>().unwrap(), hash);
    }

    #[test]
    fn source_blob_hash_rejects_wrong_length_or_non_hex_values() {
        assert!("00".parse::<SourceBlobHash>().is_err());
        assert!("not-a-digest".parse::<SourceBlobHash>().is_err());
    }
}
