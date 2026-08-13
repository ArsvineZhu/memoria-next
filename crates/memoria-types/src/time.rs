use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::MemoriaError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Timestamp {
    unix_seconds: i64,
    subsec_nanos: u32,
}

impl Timestamp {
    #[must_use]
    pub const fn from_unix_seconds(unix_seconds: i64) -> Self {
        Self {
            unix_seconds,
            subsec_nanos: 0,
        }
    }

    pub fn from_parts(unix_seconds: i64, subsec_nanos: u32) -> Result<Self, MemoriaError> {
        if subsec_nanos >= 1_000_000_000 {
            return Err(MemoriaError::InvalidTimestamp {
                value: format!("{unix_seconds}.{subsec_nanos:09}"),
            });
        }
        Ok(Self {
            unix_seconds,
            subsec_nanos,
        })
    }

    pub fn now() -> Result<Self, MemoriaError> {
        Self::from_system_time(SystemTime::now())
    }

    pub fn from_system_time(value: SystemTime) -> Result<Self, MemoriaError> {
        let duration =
            value
                .duration_since(UNIX_EPOCH)
                .map_err(|error| MemoriaError::InvalidTimestamp {
                    value: format!("before Unix epoch by {}s", error.duration().as_secs()),
                })?;
        let unix_seconds =
            i64::try_from(duration.as_secs()).map_err(|_| MemoriaError::InvalidTimestamp {
                value: duration.as_secs().to_string(),
            })?;
        Ok(Self {
            unix_seconds,
            subsec_nanos: duration.subsec_nanos(),
        })
    }

    #[must_use]
    pub const fn unix_seconds(self) -> i64 {
        self.unix_seconds
    }

    #[must_use]
    pub const fn subsec_nanos(self) -> u32 {
        self.subsec_nanos
    }

    #[must_use]
    pub fn as_system_time(self) -> SystemTime {
        UNIX_EPOCH + Duration::new(self.unix_seconds as u64, self.subsec_nanos)
    }
}

pub type UnixTimestamp = Timestamp;
pub type UtcTimestamp = Timestamp;

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::Timestamp;

    #[test]
    fn timestamp_round_trips_system_time_parts() {
        let system_time = UNIX_EPOCH + Duration::new(42, 123_456_700);
        let timestamp = Timestamp::from_system_time(system_time).unwrap();
        assert_eq!(timestamp.unix_seconds(), 42);
        assert_eq!(timestamp.subsec_nanos(), 123_456_700);
        assert_eq!(timestamp.as_system_time(), system_time);
    }

    #[test]
    fn timestamp_rejects_out_of_range_nanoseconds() {
        assert!(Timestamp::from_parts(42, 1_000_000_000).is_err());
    }
}
