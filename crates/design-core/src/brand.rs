//! The Brand Kit (plan section 13).
//!
//! Token names are stable and are the vocabulary the agent designs with. The
//! JSON field names are snake_case, but the tokens follow the plan's list
//! exactly, which is why `text_inverse` is addressed as `$brand.textInverse`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::color::Color;
use crate::error::DomainError;
use crate::ids::{AssetId, BrandId, ProjectId};
use crate::version::Version;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrandColors {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub background: Color,
    pub surface: Color,
    pub text: Color,
    pub text_inverse: Color,
    pub muted: Color,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrandTypography {
    pub display: String,
    pub body: String,
    pub mono: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrandRules {
    /// Distance from the canvas edge that critical content should respect.
    pub safe_margin: f32,
    pub minimum_font_size: f32,
}

impl Default for BrandRules {
    fn default() -> Self {
        Self {
            safe_margin: 80.0,
            minimum_font_size: 24.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BrandKit {
    pub id: BrandId,
    pub project_id: ProjectId,
    pub name: String,
    pub colors: BrandColors,
    pub typography: BrandTypography,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_asset_id: Option<AssetId>,
    /// A user instruction that takes precedence over editorial defaults
    /// (plan section 51.4).
    #[serde(default)]
    pub voice: String,
    #[serde(default)]
    pub rules: BrandRules,
    pub version: Version,
}

/// Every color token the agent may use, in the plan's spelling.
pub const COLOR_TOKENS: [&str; 8] = [
    "brand.primary",
    "brand.secondary",
    "brand.accent",
    "brand.background",
    "brand.surface",
    "brand.text",
    "brand.textInverse",
    "brand.muted",
];

/// Every font token the agent may use.
pub const FONT_TOKENS: [&str; 3] = ["brand.display", "brand.body", "brand.mono"];

impl BrandKit {
    pub fn color(&self, token: &str) -> Result<Color, DomainError> {
        let c = &self.colors;
        match token {
            "brand.primary" => Ok(c.primary),
            "brand.secondary" => Ok(c.secondary),
            "brand.accent" => Ok(c.accent),
            "brand.background" => Ok(c.background),
            "brand.surface" => Ok(c.surface),
            "brand.text" => Ok(c.text),
            "brand.textInverse" => Ok(c.text_inverse),
            "brand.muted" => Ok(c.muted),
            other => Err(DomainError::UnknownBrandToken(other.to_string())),
        }
    }

    pub fn font(&self, token: &str) -> Result<&str, DomainError> {
        match token {
            "brand.display" => Ok(&self.typography.display),
            "brand.body" => Ok(&self.typography.body),
            "brand.mono" => Ok(&self.typography.mono),
            other => Err(DomainError::UnknownBrandToken(other.to_string())),
        }
    }

    /// Resolved palette for the renderer. Tokens are resolved at render time so
    /// the document itself stays portable (plan section 14).
    pub fn color_palette(&self) -> HashMap<String, Color> {
        COLOR_TOKENS
            .iter()
            .map(|token| {
                let color = self
                    .color(token)
                    .expect("COLOR_TOKENS and BrandKit::color must stay in sync");
                ((*token).to_string(), color)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{BrandId, ProjectId};

    fn kit() -> BrandKit {
        BrandKit {
            id: BrandId::new(),
            project_id: ProjectId::new(),
            name: "Default".to_string(),
            colors: BrandColors {
                primary: Color::rgb(0xFF, 0x54, 0x36),
                secondary: Color::rgb(0xFD, 0xBA, 0x2D),
                accent: Color::rgb(0x7C, 0x3A, 0xED),
                background: Color::rgb(0x10, 0x10, 0x10),
                surface: Color::rgb(0x19, 0x19, 0x19),
                text: Color::rgb(0xFF, 0xFF, 0xFF),
                text_inverse: Color::rgb(0x10, 0x10, 0x10),
                muted: Color::rgb(0xA3, 0xA3, 0xA3),
            },
            typography: BrandTypography {
                display: "DejaVu Sans".to_string(),
                body: "DejaVu Sans".to_string(),
                mono: "DejaVu Sans Mono".to_string(),
            },
            logo_asset_id: None,
            voice: "Direct, technical and brief. Do not use hype.".to_string(),
            rules: BrandRules::default(),
            version: Version::INITIAL,
        }
    }

    #[test]
    fn resolves_every_documented_token() {
        let kit = kit();
        for token in COLOR_TOKENS {
            assert!(kit.color(token).is_ok(), "token {token} must resolve");
        }
        for token in FONT_TOKENS {
            assert!(kit.font(token).is_ok(), "token {token} must resolve");
        }
        assert_eq!(kit.color_palette().len(), COLOR_TOKENS.len());
    }

    #[test]
    fn uses_the_plans_camel_case_spelling_for_text_inverse() {
        let kit = kit();
        assert!(kit.color("brand.textInverse").is_ok());
        // The JSON field is snake_case, but it is not a token.
        assert!(kit.color("brand.text_inverse").is_err());
    }

    #[test]
    fn rejects_an_invented_token() {
        let err = kit().color("brand.headline").unwrap_err();
        assert_eq!(err.code(), "UNKNOWN_BRAND_TOKEN");
    }
}
