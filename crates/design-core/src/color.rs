//! Colors (plan sections 13 and 43).
//!
//! A color is either a literal or a `$brand.*` token. Both serialize as a plain
//! string so documents stay readable and match the plan's JSON examples.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

use crate::error::DomainError;

/// Serialized as a hex string, so documents stay readable by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, TS)]
#[ts(export, type = "string")]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

impl FromStr for Color {
    type Err = DomainError;

    /// Accepts `#RGB`, `#RRGGBB` and `#RRGGBBAA`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix('#').ok_or_else(|| {
            DomainError::InvalidColor(format!("expected a leading '#', got {s:?}"))
        })?;

        let parse = |slice: &str| -> Result<u8, DomainError> {
            u8::from_str_radix(slice, 16)
                .map_err(|_| DomainError::InvalidColor(format!("invalid hex in {s:?}")))
        };

        match hex.len() {
            3 => {
                let expand = |c: char| parse(&format!("{c}{c}"));
                let mut chars = hex.chars();
                Ok(Color::rgb(
                    expand(chars.next().unwrap())?,
                    expand(chars.next().unwrap())?,
                    expand(chars.next().unwrap())?,
                ))
            }
            6 => Ok(Color::rgb(
                parse(&hex[0..2])?,
                parse(&hex[2..4])?,
                parse(&hex[4..6])?,
            )),
            8 => Ok(Color {
                r: parse(&hex[0..2])?,
                g: parse(&hex[2..4])?,
                b: parse(&hex[4..6])?,
                a: parse(&hex[6..8])?,
            }),
            _ => Err(DomainError::InvalidColor(format!(
                "expected 3, 6 or 8 hex digits, got {} in {s:?}",
                hex.len()
            ))),
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.a == 255 {
            write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
        } else {
            write!(
                f,
                "#{:02X}{:02X}{:02X}{:02X}",
                self.r, self.g, self.b, self.a
            )
        }
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// Plan section 43: the agent should prefer tokens, the human editor may use literals.
///
/// Serialized as a string: either a literal like `#FF5436` or a token like
/// `$brand.primary`.
#[derive(Debug, Clone, PartialEq, TS)]
#[ts(export, type = "string")]
pub enum DesignColor {
    Literal(Color),
    BrandToken(String),
}

impl DesignColor {
    pub fn token(name: &str) -> Self {
        Self::BrandToken(name.to_string())
    }
}

impl fmt::Display for DesignColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(c) => write!(f, "{c}"),
            Self::BrandToken(t) => write!(f, "${t}"),
        }
    }
}

impl FromStr for DesignColor {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.strip_prefix('$') {
            Some(token) if token.starts_with("brand.") => Ok(Self::BrandToken(token.to_string())),
            Some(_) => Err(DomainError::InvalidColor(format!(
                "unknown token namespace in {s:?}, expected $brand.*"
            ))),
            None => Ok(Self::Literal(s.parse()?)),
        }
    }
}

impl Serialize for DesignColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for DesignColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_literal_forms() {
        assert_eq!(
            "#FF5436".parse::<Color>().unwrap(),
            Color::rgb(0xFF, 0x54, 0x36)
        );
        assert_eq!(
            "#f53".parse::<Color>().unwrap(),
            Color::rgb(0xFF, 0x55, 0x33)
        );
        assert_eq!("#00000080".parse::<Color>().unwrap().a, 0x80);
    }

    #[test]
    fn rejects_malformed_literals() {
        assert!("FF5436".parse::<Color>().is_err());
        assert!("#FF54".parse::<Color>().is_err());
        assert!("#GGGGGG".parse::<Color>().is_err());
    }

    #[test]
    fn distinguishes_tokens_from_literals() {
        assert_eq!(
            "$brand.primary".parse::<DesignColor>().unwrap(),
            DesignColor::token("brand.primary")
        );
        assert!(matches!(
            "#101010".parse::<DesignColor>().unwrap(),
            DesignColor::Literal(_)
        ));
        // Plan section 42: an unknown namespace is an error, never a silent fallback.
        assert!("$theme.primary".parse::<DesignColor>().is_err());
    }

    #[test]
    fn round_trips_through_json() {
        for raw in ["#FF5436", "$brand.primary", "#0000007F"] {
            let parsed: DesignColor = raw.parse().unwrap();
            let json = serde_json::to_string(&parsed).unwrap();
            let back: DesignColor = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, back, "round trip failed for {raw}");
        }
    }
}
