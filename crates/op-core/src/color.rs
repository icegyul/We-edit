//! Colors carried by parameters and labels. A parameter color is RGBA in the sequence working
//! space, nonlinear (display-referred) Rec.709 unless the space says otherwise (DM-FX-005).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for Rgba {
    fn default() -> Self {
        Rgba::WHITE
    }
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::new(0.0, 0.0, 0.0, 1.0);
    pub const WHITE: Rgba = Rgba::new(1.0, 1.0, 1.0, 1.0);
    pub const TRANSPARENT: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Rgba {
        Rgba { r, g, b, a }
    }

    pub fn from_u8(r: u8, g: u8, b: u8) -> Rgba {
        Rgba::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0)
    }

    pub fn to_u8(self) -> [u8; 4] {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [q(self.r), q(self.g), q(self.b), q(self.a)]
    }

    pub fn to_hex(self) -> String {
        let [r, g, b, _] = self.to_u8();
        format!("#{r:02X}{g:02X}{b:02X}")
    }

    pub fn from_hex(text: &str) -> Option<Rgba> {
        let t = text.trim().trim_start_matches('#');
        if t.len() != 6 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(t, 16).ok()?;
        Some(Rgba::from_u8((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    pub fn lerp(self, o: Rgba, t: f32) -> Rgba {
        Rgba::new(
            self.r + (o.r - self.r) * t,
            self.g + (o.g - self.g) * t,
            self.b + (o.b - self.b) * t,
            self.a + (o.a - self.a) * t,
        )
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// Label colors for project items and clips. Names are our own palette.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Label {
    #[default]
    None,
    Violet,
    Iris,
    Caribbean,
    Lavender,
    Cerulean,
    Forest,
    Rose,
    Mango,
    Purple,
    Blue,
    Teal,
    Magenta,
    Tan,
    Green,
    Brown,
    Yellow,
}

impl Label {
    pub const ALL: [Label; 16] = [
        Label::Violet,
        Label::Iris,
        Label::Caribbean,
        Label::Lavender,
        Label::Cerulean,
        Label::Forest,
        Label::Rose,
        Label::Mango,
        Label::Purple,
        Label::Blue,
        Label::Teal,
        Label::Magenta,
        Label::Tan,
        Label::Green,
        Label::Brown,
        Label::Yellow,
    ];

    pub fn rgb(self) -> [u8; 3] {
        match self {
            Label::None => [120, 120, 120],
            Label::Violet => [142, 118, 214],
            Label::Iris => [110, 122, 214],
            Label::Caribbean => [40, 178, 150],
            Label::Lavender => [214, 150, 214],
            Label::Cerulean => [44, 150, 214],
            Label::Forest => [70, 150, 70],
            Label::Rose => [230, 110, 160],
            Label::Mango => [236, 150, 50],
            Label::Purple => [150, 60, 180],
            Label::Blue => [60, 90, 220],
            Label::Teal => [40, 160, 170],
            Label::Magenta => [220, 40, 160],
            Label::Tan => [200, 170, 120],
            Label::Green => [90, 190, 70],
            Label::Brown => [150, 90, 50],
            Label::Yellow => [225, 210, 60],
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Label::None => "label.none",
            Label::Violet => "label.violet",
            Label::Iris => "label.iris",
            Label::Caribbean => "label.caribbean",
            Label::Lavender => "label.lavender",
            Label::Cerulean => "label.cerulean",
            Label::Forest => "label.forest",
            Label::Rose => "label.rose",
            Label::Mango => "label.mango",
            Label::Purple => "label.purple",
            Label::Blue => "label.blue",
            Label::Teal => "label.teal",
            Label::Magenta => "label.magenta",
            Label::Tan => "label.tan",
            Label::Green => "label.green",
            Label::Brown => "label.brown",
            Label::Yellow => "label.yellow",
        }
    }
}

/// Marker colors.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum MarkerColor {
    #[default]
    Green,
    Red,
    Magenta,
    Orange,
    Yellow,
    White,
    Blue,
    Cyan,
}

impl MarkerColor {
    pub const ALL: [MarkerColor; 8] = [
        MarkerColor::Green,
        MarkerColor::Red,
        MarkerColor::Magenta,
        MarkerColor::Orange,
        MarkerColor::Yellow,
        MarkerColor::White,
        MarkerColor::Blue,
        MarkerColor::Cyan,
    ];

    pub fn rgb(self) -> [u8; 3] {
        match self {
            MarkerColor::Green => [96, 200, 90],
            MarkerColor::Red => [220, 60, 60],
            MarkerColor::Magenta => [210, 70, 200],
            MarkerColor::Orange => [240, 150, 40],
            MarkerColor::Yellow => [230, 220, 60],
            MarkerColor::White => [235, 235, 235],
            MarkerColor::Blue => [70, 110, 230],
            MarkerColor::Cyan => [60, 210, 220],
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            MarkerColor::Green => "color.green",
            MarkerColor::Red => "color.red",
            MarkerColor::Magenta => "color.magenta",
            MarkerColor::Orange => "color.orange",
            MarkerColor::Yellow => "color.yellow",
            MarkerColor::White => "color.white",
            MarkerColor::Blue => "color.blue",
            MarkerColor::Cyan => "color.cyan",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = Rgba::from_hex("#1A2b3C").unwrap();
        assert_eq!(c.to_hex(), "#1A2B3C");
        assert!(Rgba::from_hex("12345").is_none());
    }
}
