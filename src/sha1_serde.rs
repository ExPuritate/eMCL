use std::{ascii, fmt::Display, num::ParseIntError};

use serde::{Deserialize, Serialize, de::Visitor};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sha1 {
    data: [u8; 20],
}

impl PartialEq<sha1::digest::Output<sha1::Sha1>> for Sha1 {
    fn eq(&self, other: &sha1::digest::Output<sha1::Sha1>) -> bool {
        self.data.eq(other)
    }
}

const HEX_DIGITS: [ascii::Char; 16] = [
    ascii::Char::Digit0,
    ascii::Char::Digit1,
    ascii::Char::Digit2,
    ascii::Char::Digit3,
    ascii::Char::Digit4,
    ascii::Char::Digit5,
    ascii::Char::Digit6,
    ascii::Char::Digit7,
    ascii::Char::Digit8,
    ascii::Char::Digit9,
    ascii::Char::SmallA,
    ascii::Char::SmallB,
    ascii::Char::SmallC,
    ascii::Char::SmallD,
    ascii::Char::SmallE,
    ascii::Char::SmallF,
];

fn byte_to_ascii_chars(byte: u8) -> [ascii::Char; 2] {
    let high = HEX_DIGITS[(byte >> 4) as usize];
    let low = HEX_DIGITS[(byte & 0x0F) as usize];
    [high, low]
}

#[derive(Debug, thiserror::Error)]
pub enum ParseSha1Error {
    InvalidLength(usize),
    ParseIntError(ParseIntError),
}

impl Display for ParseSha1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl Sha1 {
    pub fn from_str(v: &str) -> Result<Self, ParseSha1Error> {
        if v.len() != 40 {
            return Err(ParseSha1Error::InvalidLength(v.len()));
        }

        let mut data = [0u8; 20];
        for i in 0..20 {
            let start = i * 2;
            let end = start + 2;
            data[i] =
                u8::from_str_radix(&v[start..end], 16).map_err(ParseSha1Error::ParseIntError)?;
        }

        Ok(Self { data })
    }
    pub fn to_ascii_chars(&self) -> [ascii::Char; 40] {
        let mut buffer = [ascii::Char::Null; 40];
        for (i, data) in self.data.iter().enumerate() {
            let first = i * 2;
            [buffer[first], buffer[first + 1]] = byte_to_ascii_chars(*data);
        }
        buffer
    }
}

impl Serialize for Sha1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_ascii_chars().as_str())
    }
}

impl<'de> Deserialize<'de> for Sha1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(Sha1Visitor)
    }
}

struct Sha1Visitor;

impl<'de> Visitor<'de> for Sha1Visitor {
    type Value = Sha1;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("SHA1")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Sha1::from_str(v).map_err(|x| match x {
            ParseSha1Error::InvalidLength(len) => E::invalid_length(len, &self),
            ParseSha1Error::ParseIntError(e) => E::custom(e),
        })
    }
}

#[cfg(test)]
impl Sha1 {
    pub fn random() -> Self {
        // cSpell:disable-next-line
        use rand::{RngExt, rngs::ThreadRng};

        Self {
            data: ThreadRng::default().random(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_serde() {
        for _ in 0..20 {
            let random = Sha1::random();
            assert_eq!(
                Sha1::from_str(random.to_ascii_chars().as_str()).unwrap(),
                random
            );
        }
    }
}
