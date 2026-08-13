use std::{fmt, str::FromStr};

use crate::syntax::MdxError;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TemporalPrecision {
    Year,
    Month,
    Day,
    Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemporalValue {
    raw: String,
    precision: TemporalPrecision,
    year: i32,
    month: Option<u8>,
    day: Option<u8>,
}

impl TemporalValue {
    pub fn year(year: i32) -> Result<Self, MdxError> {
        Self::parse(&format!("{year:04}"))
    }

    pub fn month(year: i32, month: u8) -> Result<Self, MdxError> {
        Self::parse(&format!("{year:04}-{month:02}"))
    }

    pub fn day(year: i32, month: u8, day: u8) -> Result<Self, MdxError> {
        Self::parse(&format!("{year:04}-{month:02}-{day:02}"))
    }

    pub fn parse(value: &str) -> Result<Self, MdxError> {
        let invalid = || MdxError::InvalidTemporalValue {
            value: value.to_owned(),
            span: 0..value.len(),
        };
        if value.len() == 4 && value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Ok(Self {
                raw: value.to_owned(),
                precision: TemporalPrecision::Year,
                year: value.parse().map_err(|_| invalid())?,
                month: None,
                day: None,
            });
        }
        if value.len() == 7 && value.as_bytes()[4] == b'-' {
            let year = value[..4].parse().map_err(|_| invalid())?;
            let month = value[5..].parse().map_err(|_| invalid())?;
            if !(1..=12).contains(&month) {
                return Err(invalid());
            }
            return Ok(Self {
                raw: value.to_owned(),
                precision: TemporalPrecision::Month,
                year,
                month: Some(month),
                day: None,
            });
        }
        if value.len() == 10 && value.as_bytes()[4] == b'-' && value.as_bytes()[7] == b'-' {
            let year = value[..4].parse().map_err(|_| invalid())?;
            let month = value[5..7].parse().map_err(|_| invalid())?;
            let day = value[8..].parse().map_err(|_| invalid())?;
            if !(1..=12).contains(&month) || !(1..=days_in_month(year, month)).contains(&day) {
                return Err(invalid());
            }
            return Ok(Self {
                raw: value.to_owned(),
                precision: TemporalPrecision::Day,
                year,
                month: Some(month),
                day: Some(day),
            });
        }
        if value.len() >= 20
            && value.as_bytes().get(4) == Some(&b'-')
            && value.as_bytes().get(7) == Some(&b'-')
            && value.as_bytes().get(10) == Some(&b'T')
        {
            return Self::parse_timestamp(value);
        }
        Err(invalid())
    }

    fn parse_timestamp(value: &str) -> Result<Self, MdxError> {
        let invalid = || MdxError::InvalidTemporalValue {
            value: value.to_owned(),
            span: 0..value.len(),
        };
        let date = Self::parse(&value[..10]).map_err(|_| invalid())?;
        let bytes = value.as_bytes();
        if bytes.get(13) != Some(&b':') || bytes.get(16) != Some(&b':') {
            return Err(invalid());
        }
        let hour = parse_two_digits(&value[11..13]).ok_or_else(invalid)?;
        let minute = parse_two_digits(&value[14..16]).ok_or_else(invalid)?;
        let second = parse_two_digits(&value[17..19]).ok_or_else(invalid)?;
        if hour > 23 || minute > 59 || second > 59 {
            return Err(invalid());
        }

        let mut cursor = 19;
        if bytes.get(cursor) == Some(&b'.') {
            cursor += 1;
            let fraction_start = cursor;
            while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                cursor += 1;
            }
            if fraction_start == cursor {
                return Err(invalid());
            }
        }
        match bytes.get(cursor) {
            Some(b'Z') if cursor + 1 == value.len() => {}
            Some(b'+' | b'-') if cursor + 6 == value.len() => {
                if bytes.get(cursor + 3) != Some(&b':') {
                    return Err(invalid());
                }
                let offset_hour =
                    parse_two_digits(&value[cursor + 1..cursor + 3]).ok_or_else(invalid)?;
                let offset_minute =
                    parse_two_digits(&value[cursor + 4..cursor + 6]).ok_or_else(invalid)?;
                if offset_hour > 23 || offset_minute > 59 {
                    return Err(invalid());
                }
            }
            _ => return Err(invalid()),
        }
        let _ = (hour, minute, second);
        Ok(Self {
            raw: value.to_owned(),
            precision: TemporalPrecision::Timestamp,
            year: date.year,
            month: date.month,
            day: date.day,
        })
    }

    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    #[must_use]
    pub fn precision(&self) -> TemporalPrecision {
        self.precision
    }

    #[must_use]
    pub fn year_value(&self) -> i32 {
        self.year
    }

    #[must_use]
    pub fn month_value(&self) -> Option<u8> {
        self.month
    }

    #[must_use]
    pub fn day_value(&self) -> Option<u8> {
        self.day
    }
}

impl fmt::Display for TemporalValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.raw)
    }
}

impl FromStr for TemporalValue {
    type Err = MdxError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn parse_two_digits(value: &str) -> Option<u8> {
    if value.len() != 2 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}
