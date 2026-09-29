//! The catalog of components this editor implements: fixed effects, video and audio effects,
//! transitions and graphics. Each definition has a stable ID (our own), the display name users
//! know, its category, optional interchange identities (`match_names`) used by project importers,
//! and an ordered parameter list (DM-FX-001).
//!
//! Display names and parameter labels are English; the UI translates them through its catalogs.
//! An effect is "implemented" only when the renderer or mixer processes it; the catalog test
//! guarantees that every entry here is known to one of them (see op-render and op-audio).

use crate::color::Rgba;
use crate::params::Value;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum EffectKind {
    /// Motion and Opacity: present on every video clip.
    VideoFixed,
    VideoEffect,
    VideoTransition,
    /// Volume, Channel Volume, Panner: present on every audio clip.
    AudioFixed,
    AudioEffect,
    AudioTransition,
    /// Text and shape layers of graphics clips.
    Graphic,
}

impl EffectKind {
    pub fn is_video(self) -> bool {
        matches!(
            self,
            EffectKind::VideoFixed
                | EffectKind::VideoEffect
                | EffectKind::VideoTransition
                | EffectKind::Graphic
        )
    }
    pub fn is_transition(self) -> bool {
        matches!(
            self,
            EffectKind::VideoTransition | EffectKind::AudioTransition
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Unit {
    None,
    Percent,
    Db,
    Pixels,
    Hz,
    Ms,
    Seconds,
    Degrees,
    Ratio,
}

impl Unit {
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::None => "",
            Unit::Percent => " %",
            Unit::Db => " dB",
            Unit::Pixels => "",
            Unit::Hz => " Hz",
            Unit::Ms => " ms",
            Unit::Seconds => " s",
            Unit::Degrees => "°",
            Unit::Ratio => ":1",
        }
    }
}

/// What a point parameter is measured against.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PointSpace {
    /// Normalized to the sequence frame (shown in sequence pixels).
    Sequence,
    /// Normalized to the clip's own frame (shown in source pixels).
    Layer,
    /// A color-wheel offset inside the unit disk.
    Wheel,
}

#[derive(Clone, Copy, Debug)]
pub enum ParamKind {
    Float {
        default: f64,
        min: f64,
        max: f64,
        soft_min: f64,
        soft_max: f64,
        unit: Unit,
        decimals: u8,
    },
    Int {
        default: i64,
        min: i64,
        max: i64,
    },
    Angle {
        default: f64,
    },
    Bool {
        default: bool,
    },
    Choice {
        default: u32,
        options: &'static [&'static str],
    },
    Color {
        default: [f32; 4],
    },
    Point {
        default: [f64; 2],
        space: PointSpace,
    },
    Text {
        default: &'static str,
        multiline: bool,
    },
    /// A file path (LUTs).
    File,
    Curve,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
    pub animatable: bool,
    /// Section title in Effect Controls; consecutive parameters with the same group are folded
    /// together. Empty for top-level parameters.
    pub group: &'static str,
}

impl ParamSpec {
    pub fn default_value(&self) -> Value {
        match self.kind {
            ParamKind::Float { default, .. } => Value::Float(default),
            ParamKind::Int { default, .. } => Value::Int(default),
            ParamKind::Angle { default } => Value::Float(default),
            ParamKind::Bool { default } => Value::Bool(default),
            ParamKind::Choice { default, .. } => Value::Choice(default),
            ParamKind::Color { default } => {
                Value::Color(Rgba::new(default[0], default[1], default[2], default[3]))
            }
            ParamKind::Point { default, .. } => Value::Point(default),
            ParamKind::Text { default, .. } => Value::Text(default.to_string()),
            ParamKind::File => Value::Text(String::new()),
            ParamKind::Curve => Value::Curve(vec![[0.0, 0.0], [1.0, 1.0]]),
        }
    }

    /// Clamps a value to the parameter's hard limits.
    pub fn clamp(&self, v: Value) -> Value {
        match (self.kind, v) {
            (ParamKind::Float { min, max, .. }, Value::Float(f)) => Value::Float(f.clamp(min, max)),
            (ParamKind::Int { min, max, .. }, Value::Int(i)) => Value::Int(i.clamp(min, max)),
            (ParamKind::Choice { options, .. }, Value::Choice(c)) => {
                Value::Choice(c.min(options.len().saturating_sub(1) as u32))
            }
            (_, v) => v,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EffectDef {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: EffectKind,
    pub category: &'static str,
    /// `MatchName` identities observed in other applications' project and preset files for the
    /// same component (docs/evidence). Only attested names are listed; importers keep any other
    /// component as an opaque, unrendered instance (DM-FX-003).
    pub match_names: &'static [&'static str],
    pub params: &'static [ParamSpec],
    /// Processed on the GPU (shown with the accelerated badge).
    pub accelerated: bool,
}

impl EffectDef {
    pub fn param(&self, key: &str) -> Option<&'static ParamSpec> {
        self.params.iter().find(|p| p.key == key)
    }
}

// ----------------------------------------------------------------------------------- spec helpers

const fn f(
    key: &'static str,
    label: &'static str,
    default: f64,
    min: f64,
    max: f64,
    unit: Unit,
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Float {
            default,
            min,
            max,
            soft_min: min,
            soft_max: max,
            unit,
            decimals: 1,
        },
        animatable: true,
        group: "",
    }
}

const fn fs(
    key: &'static str,
    label: &'static str,
    default: f64,
    min: f64,
    max: f64,
    soft_min: f64,
    soft_max: f64,
    unit: Unit,
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Float {
            default,
            min,
            max,
            soft_min,
            soft_max,
            unit,
            decimals: 1,
        },
        animatable: true,
        group: "",
    }
}

const fn pct(key: &'static str, label: &'static str, default: f64) -> ParamSpec {
    f(key, label, default, 0.0, 100.0, Unit::Percent)
}

const fn int(
    key: &'static str,
    label: &'static str,
    default: i64,
    min: i64,
    max: i64,
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Int { default, min, max },
        animatable: true,
        group: "",
    }
}

const fn angle(key: &'static str, label: &'static str, default: f64) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Angle { default },
        animatable: true,
        group: "",
    }
}

const fn boolean(key: &'static str, label: &'static str, default: bool) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Bool { default },
        animatable: false,
        group: "",
    }
}

const fn choice(
    key: &'static str,
    label: &'static str,
    default: u32,
    options: &'static [&'static str],
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Choice { default, options },
        animatable: false,
        group: "",
    }
}

const fn color(key: &'static str, label: &'static str, r: f32, g: f32, b: f32) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Color {
            default: [r, g, b, 1.0],
        },
        animatable: true,
        group: "",
    }
}

const fn point(
    key: &'static str,
    label: &'static str,
    x: f64,
    y: f64,
    space: PointSpace,
) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Point {
            default: [x, y],
            space,
        },
        animatable: true,
        group: "",
    }
}

const fn text(key: &'static str, label: &'static str, default: &'static str) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Text {
            default,
            multiline: true,
        },
        animatable: false,
        group: "",
    }
}

const fn file(key: &'static str, label: &'static str) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::File,
        animatable: false,
        group: "",
    }
}

const fn curve(key: &'static str, label: &'static str) -> ParamSpec {
    ParamSpec {
        key,
        label,
        kind: ParamKind::Curve,
        animatable: false,
        group: "",
    }
}

const fn db(key: &'static str, label: &'static str, default: f64, min: f64, max: f64) -> ParamSpec {
    f(key, label, default, min, max, Unit::Db)
}

const fn hz(key: &'static str, label: &'static str, default: f64) -> ParamSpec {
    f(key, label, default, 20.0, 20000.0, Unit::Hz)
}

const fn g(mut p: ParamSpec, group: &'static str) -> ParamSpec {
    p.group = group;
    p
}

const fn still(mut p: ParamSpec) -> ParamSpec {
    p.animatable = false;
    p
}

// ------------------------------------------------------------------------------------ categories

pub const CAT_ADJUST: &str = "Adjust";
pub const CAT_BLUR: &str = "Blur & Sharpen";
pub const CAT_CHANNEL: &str = "Channel";
pub const CAT_COLOR: &str = "Color Correction";
pub const CAT_DISTORT: &str = "Distort";
pub const CAT_GENERATE: &str = "Generate";
pub const CAT_IMAGE: &str = "Image Control";
pub const CAT_KEYING: &str = "Keying";
pub const CAT_NOISE: &str = "Noise & Grain";
pub const CAT_PERSPECTIVE: &str = "Perspective";
pub const CAT_STYLIZE: &str = "Stylize";
pub const CAT_TIME: &str = "Time";
pub const CAT_TRANSFORM: &str = "Transform";
pub const CAT_TRANSITION: &str = "Transition";
pub const CAT_VIDEO: &str = "Video";
pub const CAT_TR_DISSOLVE: &str = "Dissolve";
pub const CAT_TR_IRIS: &str = "Iris";
pub const CAT_TR_SLIDE: &str = "Slide";
pub const CAT_TR_WIPE: &str = "Wipe";
pub const CAT_TR_ZOOM: &str = "Zoom";
pub const CAT_TR_3D: &str = "3D Motion";
pub const CAT_AUDIO_AMPLITUDE: &str = "Amplitude and Compression";
pub const CAT_AUDIO_DELAY: &str = "Delay and Echo";
pub const CAT_AUDIO_FILTER: &str = "Filter and EQ";
pub const CAT_AUDIO_MODULATION: &str = "Modulation";
pub const CAT_AUDIO_REVERB: &str = "Reverb";
pub const CAT_AUDIO_SPECIAL: &str = "Special";
pub const CAT_AUDIO_STEREO: &str = "Stereo Imagery";
pub const CAT_CROSSFADE: &str = "Crossfade";

pub const BLEND_MODES: &[&str] = &[
    "Normal",
    "Dissolve",
    "Darken",
    "Multiply",
    "Color Burn",
    "Linear Burn",
    "Darker Color",
    "Lighten",
    "Screen",
    "Color Dodge",
    "Linear Dodge (Add)",
    "Lighter Color",
    "Overlay",
    "Soft Light",
    "Hard Light",
    "Vivid Light",
    "Linear Light",
    "Pin Light",
    "Hard Mix",
    "Difference",
    "Exclusion",
    "Subtract",
    "Divide",
    "Hue",
    "Saturation",
    "Color",
    "Luminosity",
];

const DIRECTIONS: &[&str] = &["From West", "From East", "From North", "From South"];
const DIRECTIONS8: &[&str] = &[
    "From West",
    "From East",
    "From North",
    "From South",
    "From North West",
    "From North East",
    "From South West",
    "From South East",
];

// ------------------------------------------------------------------------------------- fixed

pub const MOTION: &str = "op.fixed.motion";
pub const OPACITY: &str = "op.fixed.opacity";
pub const VOLUME: &str = "op.fixed.volume";
pub const CHANNEL_VOLUME: &str = "op.fixed.channel_volume";
pub const PANNER: &str = "op.fixed.panner";
pub const TEXT: &str = "op.graphic.text";
pub const SHAPE: &str = "op.graphic.shape";
pub const CROSS_DISSOLVE: &str = "op.tr.cross_dissolve";
pub const CONSTANT_POWER: &str = "op.atr.constant_power";

static MOTION_PARAMS: [ParamSpec; 7] = [
    point("position", "Position", 0.5, 0.5, PointSpace::Sequence),
    fs(
        "scale",
        "Scale",
        100.0,
        0.0,
        10000.0,
        0.0,
        600.0,
        Unit::Percent,
    ),
    fs(
        "scale_width",
        "Scale Width",
        100.0,
        0.0,
        10000.0,
        0.0,
        600.0,
        Unit::Percent,
    ),
    boolean("uniform_scale", "Uniform Scale", true),
    angle("rotation", "Rotation", 0.0),
    point("anchor", "Anchor Point", 0.5, 0.5, PointSpace::Layer),
    f(
        "anti_flicker",
        "Anti-flicker Filter",
        0.0,
        0.0,
        1.0,
        Unit::None,
    ),
];

static OPACITY_PARAMS: [ParamSpec; 2] = [
    pct("opacity", "Opacity", 100.0),
    choice("blend_mode", "Blend Mode", 0, BLEND_MODES),
];

static VOLUME_PARAMS: [ParamSpec; 2] = [
    boolean("bypass", "Bypass", false),
    db("level", "Level", 0.0, -96.0, 15.0),
];

static CHANNEL_VOLUME_PARAMS: [ParamSpec; 3] = [
    boolean("bypass", "Bypass", false),
    db("left", "Left", 0.0, -96.0, 15.0),
    db("right", "Right", 0.0, -96.0, 15.0),
];

static PANNER_PARAMS: [ParamSpec; 1] = [f("balance", "Balance", 0.0, -100.0, 100.0, Unit::None)];

// ---------------------------------------------------------------------------------- graphics

pub const TEXT_ALIGN: &[&str] = &["Left", "Center", "Right"];
pub const FONT_STYLES: &[&str] = &["Regular", "Bold", "Italic", "Bold Italic"];
pub const SHAPES: &[&str] = &["Rectangle", "Ellipse"];

static TEXT_PARAMS: [ParamSpec; 25] = [
    text("text", "Source Text", "Text"),
    still(g(
        ParamSpec {
            key: "font",
            label: "Font",
            kind: ParamKind::Text {
                default: "",
                multiline: false,
            },
            animatable: false,
            group: "",
        },
        "Text",
    )),
    g(choice("font_style", "Font Style", 0, FONT_STYLES), "Text"),
    g(
        fs(
            "font_size",
            "Font Size",
            100.0,
            1.0,
            2000.0,
            8.0,
            400.0,
            Unit::Pixels,
        ),
        "Text",
    ),
    g(choice("align", "Alignment", 1, TEXT_ALIGN), "Text"),
    g(
        fs(
            "tracking",
            "Tracking",
            0.0,
            -1000.0,
            1000.0,
            -200.0,
            500.0,
            Unit::None,
        ),
        "Text",
    ),
    g(
        fs(
            "leading",
            "Leading",
            0.0,
            -500.0,
            500.0,
            -100.0,
            200.0,
            Unit::Pixels,
        ),
        "Text",
    ),
    g(color("fill", "Fill", 1.0, 1.0, 1.0), "Appearance"),
    g(boolean("stroke", "Stroke", false), "Appearance"),
    g(
        color("stroke_color", "Stroke Color", 0.0, 0.0, 0.0),
        "Appearance",
    ),
    g(
        fs(
            "stroke_width",
            "Stroke Width",
            4.0,
            0.0,
            200.0,
            0.0,
            40.0,
            Unit::Pixels,
        ),
        "Appearance",
    ),
    g(boolean("background", "Background", false), "Appearance"),
    g(
        color("background_color", "Background Color", 0.0, 0.0, 0.0),
        "Appearance",
    ),
    g(
        pct("background_opacity", "Background Opacity", 100.0),
        "Appearance",
    ),
    g(
        fs(
            "background_size",
            "Background Size",
            10.0,
            0.0,
            500.0,
            0.0,
            100.0,
            Unit::Pixels,
        ),
        "Appearance",
    ),
    g(boolean("shadow", "Shadow", false), "Appearance"),
    g(
        color("shadow_color", "Shadow Color", 0.0, 0.0, 0.0),
        "Appearance",
    ),
    g(pct("shadow_opacity", "Shadow Opacity", 75.0), "Appearance"),
    g(angle("shadow_angle", "Shadow Angle", 135.0), "Appearance"),
    g(
        fs(
            "shadow_distance",
            "Shadow Distance",
            10.0,
            0.0,
            1000.0,
            0.0,
            100.0,
            Unit::Pixels,
        ),
        "Appearance",
    ),
    g(
        fs(
            "shadow_blur",
            "Shadow Blur",
            20.0,
            0.0,
            500.0,
            0.0,
            100.0,
            Unit::Pixels,
        ),
        "Appearance",
    ),
    g(
        point("position", "Position", 0.5, 0.5, PointSpace::Sequence),
        "Transform",
    ),
    g(
        fs(
            "scale",
            "Scale",
            100.0,
            0.0,
            4000.0,
            0.0,
            400.0,
            Unit::Percent,
        ),
        "Transform",
    ),
    g(angle("rotation", "Rotation", 0.0), "Transform"),
    g(pct("opacity", "Opacity", 100.0), "Transform"),
];

static SHAPE_PARAMS: [ParamSpec; 13] = [
    choice("shape", "Shape", 0, SHAPES),
    fs(
        "width",
        "Width",
        400.0,
        0.0,
        20000.0,
        0.0,
        4000.0,
        Unit::Pixels,
    ),
    fs(
        "height",
        "Height",
        225.0,
        0.0,
        20000.0,
        0.0,
        4000.0,
        Unit::Pixels,
    ),
    fs(
        "corner_radius",
        "Corner Radius",
        0.0,
        0.0,
        5000.0,
        0.0,
        500.0,
        Unit::Pixels,
    ),
    g(color("fill", "Fill", 0.85, 0.25, 0.25), "Appearance"),
    g(boolean("stroke", "Stroke", false), "Appearance"),
    g(
        color("stroke_color", "Stroke Color", 1.0, 1.0, 1.0),
        "Appearance",
    ),
    g(
        fs(
            "stroke_width",
            "Stroke Width",
            4.0,
            0.0,
            200.0,
            0.0,
            40.0,
            Unit::Pixels,
        ),
        "Appearance",
    ),
    g(
        point("position", "Position", 0.5, 0.5, PointSpace::Sequence),
        "Transform",
    ),
    g(
        fs(
            "scale",
            "Scale",
            100.0,
            0.0,
            4000.0,
            0.0,
            400.0,
            Unit::Percent,
        ),
        "Transform",
    ),
    g(angle("rotation", "Rotation", 0.0), "Transform"),
    g(pct("opacity", "Opacity", 100.0), "Transform"),
    g(
        fs(
            "feather",
            "Feather",
            0.0,
            0.0,
            500.0,
            0.0,
            100.0,
            Unit::Pixels,
        ),
        "Transform",
    ),
];

// ------------------------------------------------------------------------------ video effects

static BRIGHTNESS_CONTRAST: [ParamSpec; 2] = [
    f("brightness", "Brightness", 0.0, -100.0, 100.0, Unit::None),
    f("contrast", "Contrast", 0.0, -100.0, 100.0, Unit::None),
];

static PROCAMP: [ParamSpec; 6] = [
    f("brightness", "Brightness", 0.0, -100.0, 100.0, Unit::None),
    f("contrast", "Contrast", 100.0, 0.0, 200.0, Unit::None),
    angle("hue", "Hue", 0.0),
    f("saturation", "Saturation", 100.0, 0.0, 200.0, Unit::None),
    boolean("split", "Split Screen", false),
    pct("split_percent", "Split Percent", 50.0),
];

static LEVELS: [ParamSpec; 5] = [
    f(
        "input_black",
        "Input Black Level",
        0.0,
        0.0,
        255.0,
        Unit::None,
    ),
    f(
        "input_white",
        "Input White Level",
        255.0,
        0.0,
        255.0,
        Unit::None,
    ),
    f(
        "output_black",
        "Output Black Level",
        0.0,
        0.0,
        255.0,
        Unit::None,
    ),
    f(
        "output_white",
        "Output White Level",
        255.0,
        0.0,
        255.0,
        Unit::None,
    ),
    f("gamma", "Gamma", 1.0, 0.1, 9.99, Unit::None),
];

static CHANNEL_MIXER: [ParamSpec; 13] = [
    f("rr", "Red-Red", 100.0, -200.0, 200.0, Unit::None),
    f("rg", "Red-Green", 0.0, -200.0, 200.0, Unit::None),
    f("rb", "Red-Blue", 0.0, -200.0, 200.0, Unit::None),
    f("rc", "Red-Const", 0.0, -200.0, 200.0, Unit::None),
    f("gr", "Green-Red", 0.0, -200.0, 200.0, Unit::None),
    f("gg", "Green-Green", 100.0, -200.0, 200.0, Unit::None),
    f("gb", "Green-Blue", 0.0, -200.0, 200.0, Unit::None),
    f("gc", "Green-Const", 0.0, -200.0, 200.0, Unit::None),
    f("br", "Blue-Red", 0.0, -200.0, 200.0, Unit::None),
    f("bg", "Blue-Green", 0.0, -200.0, 200.0, Unit::None),
    f("bb", "Blue-Blue", 100.0, -200.0, 200.0, Unit::None),
    f("bc", "Blue-Const", 0.0, -200.0, 200.0, Unit::None),
    boolean("monochrome", "Monochrome", false),
];

static EXTRACT: [ParamSpec; 4] = [
    f("black", "Black Input Level", 0.0, 0.0, 255.0, Unit::None),
    f("white", "White Input Level", 255.0, 0.0, 255.0, Unit::None),
    pct("softness", "Softness", 0.0),
    boolean("invert", "Invert", false),
];

static GAUSSIAN_BLUR: [ParamSpec; 3] = [
    fs(
        "blurriness",
        "Blurriness",
        0.0,
        0.0,
        3000.0,
        0.0,
        100.0,
        Unit::None,
    ),
    choice(
        "dimensions",
        "Blur Dimensions",
        0,
        &["Horizontal and Vertical", "Horizontal", "Vertical"],
    ),
    boolean("repeat_edge", "Repeat Edge Pixels", false),
];

static DIRECTIONAL_BLUR: [ParamSpec; 2] = [
    angle("direction", "Direction", 0.0),
    fs(
        "length",
        "Blur Length",
        0.0,
        0.0,
        1000.0,
        0.0,
        50.0,
        Unit::None,
    ),
];

static SHARPEN: [ParamSpec; 1] = [fs(
    "amount",
    "Sharpen Amount",
    0.0,
    0.0,
    4000.0,
    0.0,
    100.0,
    Unit::None,
)];

static UNSHARP_MASK: [ParamSpec; 3] = [
    fs(
        "amount",
        "Amount",
        0.0,
        0.0,
        500.0,
        0.0,
        500.0,
        Unit::Percent,
    ),
    fs("radius", "Radius", 1.0, 0.1, 250.0, 0.1, 50.0, Unit::Pixels),
    f("threshold", "Threshold", 0.0, 0.0, 255.0, Unit::None),
];

static INVERT: [ParamSpec; 2] = [
    choice(
        "channel",
        "Channel",
        0,
        &["RGB", "Red", "Green", "Blue", "Luminance", "Alpha"],
    ),
    pct("blend", "Blend With Original", 0.0),
];

const LUMETRI_BASIC: &str = "Basic Correction";
const LUMETRI_CREATIVE: &str = "Creative";
const LUMETRI_CURVES: &str = "Curves";
const LUMETRI_WHEELS: &str = "Color Wheels";
const LUMETRI_VIGNETTE: &str = "Vignette";

static LUMETRI: [ParamSpec; 34] = [
    g(file("input_lut", "Input LUT"), LUMETRI_BASIC),
    g(
        fs(
            "temperature",
            "Temperature",
            0.0,
            -150.0,
            150.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_BASIC,
    ),
    g(
        fs(
            "tint",
            "Tint",
            0.0,
            -150.0,
            150.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_BASIC,
    ),
    g(
        fs(
            "exposure",
            "Exposure",
            0.0,
            -7.0,
            7.0,
            -5.0,
            5.0,
            Unit::None,
        ),
        LUMETRI_BASIC,
    ),
    g(
        fs(
            "contrast",
            "Contrast",
            0.0,
            -150.0,
            150.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_BASIC,
    ),
    g(
        fs(
            "highlights",
            "Highlights",
            0.0,
            -150.0,
            150.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_BASIC,
    ),
    g(
        fs(
            "shadows",
            "Shadows",
            0.0,
            -150.0,
            150.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_BASIC,
    ),
    g(
        fs(
            "whites",
            "Whites",
            0.0,
            -150.0,
            150.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_BASIC,
    ),
    g(
        fs(
            "blacks",
            "Blacks",
            0.0,
            -150.0,
            150.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_BASIC,
    ),
    g(
        fs(
            "saturation",
            "Saturation",
            100.0,
            0.0,
            300.0,
            0.0,
            200.0,
            Unit::None,
        ),
        LUMETRI_BASIC,
    ),
    g(file("look", "Look"), LUMETRI_CREATIVE),
    g(
        f("look_intensity", "Intensity", 100.0, 0.0, 200.0, Unit::None),
        LUMETRI_CREATIVE,
    ),
    g(
        fs(
            "faded_film",
            "Faded Film",
            0.0,
            0.0,
            150.0,
            0.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_CREATIVE,
    ),
    g(
        f("sharpen", "Sharpen", 0.0, -100.0, 100.0, Unit::None),
        LUMETRI_CREATIVE,
    ),
    g(
        f("vibrance", "Vibrance", 0.0, -100.0, 100.0, Unit::None),
        LUMETRI_CREATIVE,
    ),
    g(
        fs(
            "creative_saturation",
            "Saturation",
            100.0,
            0.0,
            300.0,
            0.0,
            200.0,
            Unit::None,
        ),
        LUMETRI_CREATIVE,
    ),
    g(
        point("shadow_tint", "Shadow Tint", 0.0, 0.0, PointSpace::Wheel),
        LUMETRI_CREATIVE,
    ),
    g(
        point(
            "highlight_tint",
            "Highlight Tint",
            0.0,
            0.0,
            PointSpace::Wheel,
        ),
        LUMETRI_CREATIVE,
    ),
    g(
        fs(
            "tint_balance",
            "Tint Balance",
            0.0,
            -150.0,
            150.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_CREATIVE,
    ),
    g(curve("curve_master", "RGB Curves"), LUMETRI_CURVES),
    g(curve("curve_red", "Red Curve"), LUMETRI_CURVES),
    g(curve("curve_green", "Green Curve"), LUMETRI_CURVES),
    g(curve("curve_blue", "Blue Curve"), LUMETRI_CURVES),
    g(
        point("wheel_shadows", "Shadows", 0.0, 0.0, PointSpace::Wheel),
        LUMETRI_WHEELS,
    ),
    g(
        f(
            "wheel_shadows_luma",
            "Shadows Brightness",
            0.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_WHEELS,
    ),
    g(
        point("wheel_midtones", "Midtones", 0.0, 0.0, PointSpace::Wheel),
        LUMETRI_WHEELS,
    ),
    g(
        f(
            "wheel_midtones_luma",
            "Midtones Brightness",
            0.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_WHEELS,
    ),
    g(
        point(
            "wheel_highlights",
            "Highlights",
            0.0,
            0.0,
            PointSpace::Wheel,
        ),
        LUMETRI_WHEELS,
    ),
    g(
        f(
            "wheel_highlights_luma",
            "Highlights Brightness",
            0.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_WHEELS,
    ),
    g(
        f("vignette_amount", "Amount", 0.0, -5.0, 5.0, Unit::None),
        LUMETRI_VIGNETTE,
    ),
    g(
        f(
            "vignette_midpoint",
            "Midpoint",
            50.0,
            0.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_VIGNETTE,
    ),
    g(
        f(
            "vignette_roundness",
            "Roundness",
            0.0,
            -100.0,
            100.0,
            Unit::None,
        ),
        LUMETRI_VIGNETTE,
    ),
    g(
        f("vignette_feather", "Feather", 50.0, 0.0, 100.0, Unit::None),
        LUMETRI_VIGNETTE,
    ),
    g(boolean("high_quality", "High Quality", true), ""),
];

static TINT: [ParamSpec; 3] = [
    color("black", "Map Black To", 0.0, 0.0, 0.0),
    color("white", "Map White To", 1.0, 1.0, 1.0),
    pct("amount", "Amount to Tint", 100.0),
];

static COLOR_BALANCE: [ParamSpec; 10] = [
    f(
        "shadow_r",
        "Shadow Red Balance",
        0.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    f(
        "shadow_g",
        "Shadow Green Balance",
        0.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    f(
        "shadow_b",
        "Shadow Blue Balance",
        0.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    f(
        "mid_r",
        "Midtone Red Balance",
        0.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    f(
        "mid_g",
        "Midtone Green Balance",
        0.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    f(
        "mid_b",
        "Midtone Blue Balance",
        0.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    f(
        "high_r",
        "Highlight Red Balance",
        0.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    f(
        "high_g",
        "Highlight Green Balance",
        0.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    f(
        "high_b",
        "Highlight Blue Balance",
        0.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    boolean("preserve_luminosity", "Preserve Luminosity", false),
];

static LEAVE_COLOR: [ParamSpec; 5] = [
    pct("amount", "Amount to Decolor", 0.0),
    color("color", "Color To Leave", 1.0, 0.0, 0.0),
    pct("tolerance", "Tolerance", 15.0),
    pct("softness", "Edge Softness", 0.0),
    choice("match", "Match Colors", 0, &["Using RGB", "Using Hue"]),
];

static TRANSFORM: [ParamSpec; 10] = [
    point("anchor", "Anchor Point", 0.5, 0.5, PointSpace::Layer),
    point("position", "Position", 0.5, 0.5, PointSpace::Layer),
    boolean("uniform_scale", "Uniform Scale", true),
    fs(
        "scale_height",
        "Scale Height",
        100.0,
        -10000.0,
        10000.0,
        0.0,
        600.0,
        Unit::Percent,
    ),
    fs(
        "scale_width",
        "Scale Width",
        100.0,
        -10000.0,
        10000.0,
        0.0,
        600.0,
        Unit::Percent,
    ),
    f("skew", "Skew", 0.0, -70.0, 70.0, Unit::None),
    angle("skew_axis", "Skew Axis", 0.0),
    angle("rotation", "Rotation", 0.0),
    pct("opacity", "Opacity", 100.0),
    choice("sampling", "Sampling", 0, &["Bilinear", "Bicubic"]),
];

static MIRROR: [ParamSpec; 2] = [
    point("center", "Reflection Center", 0.5, 0.5, PointSpace::Layer),
    angle("angle", "Reflection Angle", 0.0),
];

static CORNER_PIN: [ParamSpec; 4] = [
    point("ul", "Upper Left", 0.0, 0.0, PointSpace::Layer),
    point("ur", "Upper Right", 1.0, 0.0, PointSpace::Layer),
    point("ll", "Lower Left", 0.0, 1.0, PointSpace::Layer),
    point("lr", "Lower Right", 1.0, 1.0, PointSpace::Layer),
];

static OFFSET: [ParamSpec; 2] = [
    point("center", "Shift Center To", 0.5, 0.5, PointSpace::Layer),
    pct("blend", "Blend With Original", 0.0),
];

static SPHERIZE: [ParamSpec; 2] = [
    fs(
        "radius",
        "Radius",
        0.0,
        0.0,
        2500.0,
        0.0,
        1000.0,
        Unit::Pixels,
    ),
    point("center", "Center of Sphere", 0.5, 0.5, PointSpace::Layer),
];

static TWIRL: [ParamSpec; 3] = [
    angle("angle", "Angle", 0.0),
    f("radius", "Twirl Radius", 75.0, 0.0, 100.0, Unit::None),
    point("center", "Twirl Center", 0.5, 0.5, PointSpace::Layer),
];

static WAVE_WARP: [ParamSpec; 6] = [
    choice(
        "wave",
        "Wave Type",
        0,
        &["Sine", "Square", "Triangle", "Sawtooth"],
    ),
    fs(
        "height",
        "Wave Height",
        10.0,
        -1000.0,
        1000.0,
        -100.0,
        100.0,
        Unit::Pixels,
    ),
    fs(
        "width",
        "Wave Width",
        40.0,
        1.0,
        1000.0,
        1.0,
        500.0,
        Unit::Pixels,
    ),
    angle("direction", "Direction", 90.0),
    fs(
        "speed",
        "Wave Speed",
        1.0,
        -10.0,
        10.0,
        -5.0,
        5.0,
        Unit::None,
    ),
    angle("phase", "Phase", 0.0),
];

static RAMP: [ParamSpec; 6] = [
    point("start", "Start of Ramp", 0.5, 0.0, PointSpace::Layer),
    color("start_color", "Start Color", 0.0, 0.0, 0.0),
    point("end", "End of Ramp", 0.5, 1.0, PointSpace::Layer),
    color("end_color", "End Color", 1.0, 1.0, 1.0),
    choice("shape", "Ramp Shape", 0, &["Linear Ramp", "Radial Ramp"]),
    pct("blend", "Blend With Original", 0.0),
];

static GAMMA: [ParamSpec; 1] = [f("gamma", "Gamma", 10.0, 1.0, 28.0, Unit::None)];

static COLOR_REPLACE: [ParamSpec; 4] = [
    color("target", "Target Color", 1.0, 1.0, 1.0),
    color("replace", "Replace Color", 1.0, 0.0, 0.0),
    pct("similarity", "Similarity", 0.0),
    boolean("solid", "Solid Colors", false),
];

pub const ULTRA_KEY_OUTPUT: &[&str] = &["Composite", "Alpha Channel", "Color Channel"];

static ULTRA_KEY: [ParamSpec; 15] = [
    choice("output", "Output", 0, ULTRA_KEY_OUTPUT),
    choice(
        "setting",
        "Setting",
        0,
        &["Default", "Relaxed", "Aggressive", "Custom"],
    ),
    color("key_color", "Key Color", 0.0, 0.8, 0.2),
    g(
        pct("transparency", "Transparency", 45.0),
        "Matte Generation",
    ),
    g(pct("highlight", "Highlight", 10.0), "Matte Generation"),
    g(pct("shadow", "Shadow", 50.0), "Matte Generation"),
    g(pct("tolerance", "Tolerance", 50.0), "Matte Generation"),
    g(pct("pedestal", "Pedestal", 10.0), "Matte Generation"),
    g(pct("choke", "Choke", 0.0), "Matte Cleanup"),
    g(pct("soften", "Soften", 0.0), "Matte Cleanup"),
    g(pct("contrast", "Contrast", 0.0), "Matte Cleanup"),
    g(pct("desaturate", "Desaturate", 25.0), "Spill Suppression"),
    g(pct("range", "Range", 50.0), "Spill Suppression"),
    g(pct("spill", "Spill", 50.0), "Spill Suppression"),
    g(pct("luma", "Luma", 50.0), "Spill Suppression"),
];

static COLOR_KEY: [ParamSpec; 4] = [
    color("key_color", "Key Color", 0.0, 0.0, 1.0),
    f("tolerance", "Color Tolerance", 0.0, 0.0, 255.0, Unit::None),
    f("edge_thin", "Edge Thin", 0.0, -5.0, 5.0, Unit::None),
    f("edge_feather", "Edge Feather", 0.0, 0.0, 10.0, Unit::None),
];

static LUMA_KEY: [ParamSpec; 2] = [
    pct("threshold", "Threshold", 0.0),
    pct("cutoff", "Cutoff", 0.0),
];

pub const TRACK_MATTE_TRACKS: &[&str] = &[
    "None", "Video 1", "Video 2", "Video 3", "Video 4", "Video 5", "Video 6", "Video 7", "Video 8",
    "Video 9",
];

static TRACK_MATTE: [ParamSpec; 3] = [
    choice("matte", "Matte", 0, TRACK_MATTE_TRACKS),
    choice(
        "composite",
        "Composite Using",
        0,
        &["Matte Alpha", "Matte Luma"],
    ),
    boolean("reverse", "Reverse", false),
];

static NOISE: [ParamSpec; 3] = [
    pct("amount", "Amount of Noise", 0.0),
    boolean("color", "Use Color Noise", true),
    boolean("clip", "Clipping", true),
];

static DROP_SHADOW: [ParamSpec; 6] = [
    color("color", "Shadow Color", 0.0, 0.0, 0.0),
    pct("opacity", "Opacity", 50.0),
    angle("direction", "Direction", 135.0),
    fs(
        "distance",
        "Distance",
        5.0,
        0.0,
        4000.0,
        0.0,
        120.0,
        Unit::Pixels,
    ),
    fs(
        "softness",
        "Softness",
        0.0,
        0.0,
        1000.0,
        0.0,
        250.0,
        Unit::Pixels,
    ),
    boolean("shadow_only", "Shadow Only", false),
];

static BASIC_3D: [ParamSpec; 4] = [
    angle("swivel", "Swivel", 0.0),
    angle("tilt", "Tilt", 0.0),
    fs(
        "distance",
        "Distance to Image",
        0.0,
        -1000.0,
        1000.0,
        -100.0,
        100.0,
        Unit::None,
    ),
    boolean("specular", "Specular Highlight", false),
];

static MOSAIC: [ParamSpec; 3] = [
    int("horizontal", "Horizontal Blocks", 10, 1, 4000),
    int("vertical", "Vertical Blocks", 10, 1, 4000),
    boolean("sharp", "Sharp Colors", false),
];

static POSTERIZE: [ParamSpec; 1] = [int("level", "Level", 7, 2, 255)];

static FIND_EDGES: [ParamSpec; 2] = [
    boolean("invert", "Invert", false),
    pct("blend", "Blend With Original", 0.0),
];

static EMBOSS: [ParamSpec; 4] = [
    angle("direction", "Direction", 45.0),
    f("relief", "Relief", 1.75, 0.0, 10.0, Unit::None),
    f("contrast", "Contrast", 100.0, 0.0, 500.0, Unit::None),
    pct("blend", "Blend With Original", 0.0),
];

static SOLARIZE: [ParamSpec; 1] = [int("threshold", "Threshold", 128, 0, 254)];

static REPLICATE: [ParamSpec; 1] = [int("count", "Count", 2, 2, 16)];

static THRESHOLD: [ParamSpec; 1] = [f("level", "Level", 128.0, 0.0, 255.0, Unit::None)];

static POSTERIZE_TIME: [ParamSpec; 1] = [f("rate", "Frame Rate", 12.0, 0.1, 99.0, Unit::None)];

static CROP: [ParamSpec; 6] = [
    pct("left", "Left", 0.0),
    pct("top", "Top", 0.0),
    pct("right", "Right", 0.0),
    pct("bottom", "Bottom", 0.0),
    boolean("zoom", "Zoom", false),
    fs(
        "feather",
        "Edge Feather",
        0.0,
        0.0,
        1000.0,
        0.0,
        100.0,
        Unit::Pixels,
    ),
];

static EDGE_FEATHER: [ParamSpec; 1] = [f("amount", "Amount", 10.0, 0.0, 100.0, Unit::None)];

static LINEAR_WIPE: [ParamSpec; 3] = [
    pct("completion", "Transition Completion", 0.0),
    angle("angle", "Wipe Angle", 90.0),
    fs(
        "feather",
        "Feather",
        0.0,
        0.0,
        1000.0,
        0.0,
        200.0,
        Unit::Pixels,
    ),
];

static RADIAL_WIPE: [ParamSpec; 5] = [
    pct("completion", "Transition Completion", 0.0),
    angle("start", "Start Angle", 0.0),
    point("center", "Wipe Center", 0.5, 0.5, PointSpace::Layer),
    choice(
        "wipe",
        "Wipe",
        0,
        &["Clockwise", "Counterclockwise", "Both"],
    ),
    fs(
        "feather",
        "Feather",
        0.0,
        0.0,
        1000.0,
        0.0,
        200.0,
        Unit::Pixels,
    ),
];

static TIMECODE: [ParamSpec; 6] = [
    point("position", "Position", 0.5, 0.9, PointSpace::Sequence),
    f("size", "Size", 5.0, 1.0, 50.0, Unit::Percent),
    pct("opacity", "Opacity", 50.0),
    choice(
        "source",
        "Timecode Source",
        0,
        &["Media", "Clip", "Sequence"],
    ),
    choice("format", "Time Display", 0, &["Timecode", "Frames"]),
    fs(
        "offset",
        "Offset",
        0.0,
        -100000.0,
        100000.0,
        -1000.0,
        1000.0,
        Unit::None,
    ),
];

// ------------------------------------------------------------------------------- transitions

static TR_NONE: [ParamSpec; 1] = [boolean("reverse", "Reverse", false)];

static TR_DIRECTIONAL: [ParamSpec; 2] = [
    choice("direction", "Direction", 0, DIRECTIONS),
    boolean("reverse", "Reverse", false),
];

static TR_DIRECTIONAL8: [ParamSpec; 4] = [
    choice("direction", "Direction", 0, DIRECTIONS8),
    fs(
        "border",
        "Border Width",
        0.0,
        0.0,
        100.0,
        0.0,
        30.0,
        Unit::Pixels,
    ),
    color("border_color", "Border Color", 0.0, 0.0, 0.0),
    boolean("reverse", "Reverse", false),
];

static TR_WIPE: [ParamSpec; 5] = [
    choice("direction", "Direction", 0, DIRECTIONS8),
    fs(
        "border",
        "Border Width",
        0.0,
        0.0,
        100.0,
        0.0,
        30.0,
        Unit::Pixels,
    ),
    color("border_color", "Border Color", 0.0, 0.0, 0.0),
    fs(
        "feather",
        "Feather",
        0.0,
        0.0,
        500.0,
        0.0,
        100.0,
        Unit::Pixels,
    ),
    boolean("reverse", "Reverse", false),
];

static TR_IRIS: [ParamSpec; 5] = [
    point("center", "Center", 0.5, 0.5, PointSpace::Sequence),
    fs(
        "border",
        "Border Width",
        0.0,
        0.0,
        100.0,
        0.0,
        30.0,
        Unit::Pixels,
    ),
    color("border_color", "Border Color", 0.0, 0.0, 0.0),
    fs(
        "feather",
        "Feather",
        0.0,
        0.0,
        500.0,
        0.0,
        100.0,
        Unit::Pixels,
    ),
    boolean("reverse", "Reverse", false),
];

static TR_SPLIT: [ParamSpec; 2] = [
    choice("orientation", "Orientation", 0, &["Vertical", "Horizontal"]),
    boolean("reverse", "Reverse", false),
];

static TR_BLINDS: [ParamSpec; 3] = [
    int("count", "Bands", 8, 2, 100),
    choice("direction", "Direction", 0, &["Horizontal", "Vertical"]),
    boolean("reverse", "Reverse", false),
];

static TR_CHECKER: [ParamSpec; 3] = [
    int("columns", "Columns", 8, 2, 100),
    int("rows", "Rows", 6, 2, 100),
    boolean("reverse", "Reverse", false),
];

static TR_BLOCKS: [ParamSpec; 4] = [
    int("columns", "Columns", 16, 2, 200),
    int("rows", "Rows", 9, 2, 200),
    int("seed", "Random Seed", 1, 0, 9999),
    boolean("reverse", "Reverse", false),
];

static TR_CLOCK: [ParamSpec; 3] = [
    point("center", "Center", 0.5, 0.5, PointSpace::Sequence),
    angle("start", "Start Angle", 0.0),
    boolean("reverse", "Reverse", false),
];

static TR_WHIP: [ParamSpec; 2] = [
    choice("direction", "Direction", 0, DIRECTIONS),
    boolean("reverse", "Reverse", false),
];

static TR_ZOOM: [ParamSpec; 2] = [
    point("center", "Center", 0.5, 0.5, PointSpace::Sequence),
    boolean("reverse", "Reverse", false),
];

static TR_FLIP: [ParamSpec; 3] = [
    choice("axis", "Axis", 0, &["Horizontal", "Vertical"]),
    color("fill", "Fill Color", 0.0, 0.0, 0.0),
    boolean("reverse", "Reverse", false),
];

// ------------------------------------------------------------------------------ audio effects

static AMPLIFY: [ParamSpec; 2] = [
    db("left", "Left Gain", 0.0, -96.0, 30.0),
    db("right", "Right Gain", 0.0, -96.0, 30.0),
];

static BASS: [ParamSpec; 1] = [db("boost", "Boost", 0.0, -24.0, 24.0)];
static TREBLE: [ParamSpec; 1] = [db("boost", "Boost", 0.0, -24.0, 24.0)];
static HIGHPASS: [ParamSpec; 1] = [hz("cutoff", "Cutoff", 80.0)];
static LOWPASS: [ParamSpec; 1] = [hz("cutoff", "Cutoff", 8000.0)];
static BANDPASS: [ParamSpec; 2] = [
    hz("center", "Center", 1000.0),
    f("q", "Q", 1.0, 0.1, 100.0, Unit::None),
];
static NOTCH: [ParamSpec; 2] = [
    hz("center", "Center", 60.0),
    f("q", "Q", 10.0, 0.1, 100.0, Unit::None),
];
static SIMPLE_EQ: [ParamSpec; 3] = [
    hz("center", "Center", 1000.0),
    f("q", "Q", 1.0, 0.1, 100.0, Unit::None),
    db("boost", "Boost", 0.0, -24.0, 24.0),
];

static PARAMETRIC_EQ: [ParamSpec; 16] = [
    g(hz("low_freq", "Low Shelf Frequency", 100.0), "Low Shelf"),
    g(
        db("low_gain", "Low Shelf Gain", 0.0, -30.0, 30.0),
        "Low Shelf",
    ),
    g(hz("b1_freq", "Band 1 Frequency", 250.0), "Band 1"),
    g(db("b1_gain", "Band 1 Gain", 0.0, -30.0, 30.0), "Band 1"),
    g(f("b1_q", "Band 1 Q", 1.0, 0.1, 100.0, Unit::None), "Band 1"),
    g(hz("b2_freq", "Band 2 Frequency", 1000.0), "Band 2"),
    g(db("b2_gain", "Band 2 Gain", 0.0, -30.0, 30.0), "Band 2"),
    g(f("b2_q", "Band 2 Q", 1.0, 0.1, 100.0, Unit::None), "Band 2"),
    g(hz("b3_freq", "Band 3 Frequency", 4000.0), "Band 3"),
    g(db("b3_gain", "Band 3 Gain", 0.0, -30.0, 30.0), "Band 3"),
    g(f("b3_q", "Band 3 Q", 1.0, 0.1, 100.0, Unit::None), "Band 3"),
    g(
        hz("high_freq", "High Shelf Frequency", 8000.0),
        "High Shelf",
    ),
    g(
        db("high_gain", "High Shelf Gain", 0.0, -30.0, 30.0),
        "High Shelf",
    ),
    g(hz("hp_freq", "High Pass", 20.0), "Pass Filters"),
    g(hz("lp_freq", "Low Pass", 20000.0), "Pass Filters"),
    db("output", "Output Gain", 0.0, -30.0, 30.0),
];

static DELAY: [ParamSpec; 3] = [
    f("time", "Delay", 0.5, 0.0, 2.0, Unit::Seconds),
    pct("feedback", "Feedback", 0.0),
    pct("mix", "Mix", 50.0),
];

static HARD_LIMITER: [ParamSpec; 4] = [
    db("ceiling", "Maximum Amplitude", -0.1, -40.0, 0.0),
    db("input", "Input Boost", 0.0, 0.0, 40.0),
    f("lookahead", "Look-Ahead Time", 7.0, 1.0, 50.0, Unit::Ms),
    f("release", "Release Time", 100.0, 10.0, 1000.0, Unit::Ms),
];

static COMPRESSOR: [ParamSpec; 6] = [
    db("threshold", "Threshold", -20.0, -60.0, 0.0),
    f("ratio", "Ratio", 4.0, 1.0, 30.0, Unit::Ratio),
    f("attack", "Attack", 10.0, 0.1, 500.0, Unit::Ms),
    f("release", "Release", 100.0, 10.0, 5000.0, Unit::Ms),
    db("output", "Output Gain", 0.0, -30.0, 30.0),
    boolean("auto_gain", "Auto Gain", false),
];

static REVERB: [ParamSpec; 5] = [
    pct("room", "Room Size", 50.0),
    pct("damping", "High Frequency Damping", 50.0),
    pct("width", "Width", 100.0),
    pct("dry", "Dry", 80.0),
    pct("wet", "Wet", 25.0),
];

static CHORUS: [ParamSpec; 5] = [
    choice("mode", "Mode", 0, &["Chorus", "Flanger"]),
    f("rate", "Speed", 0.8, 0.05, 10.0, Unit::Hz),
    pct("depth", "Depth", 50.0),
    pct("feedback", "Feedback", 20.0),
    pct("mix", "Mix", 50.0),
];

static STEREO_EXPANDER: [ParamSpec; 1] = [f("width", "Width", 100.0, 0.0, 300.0, Unit::Percent)];

static NO_PARAMS: [ParamSpec; 0] = [];

// --------------------------------------------------------------------------------- the table

macro_rules! def {
    ($id:expr, $name:expr, $kind:ident, $cat:expr, [$($mn:expr),*], $params:expr) => {
        EffectDef {
            id: $id,
            name: $name,
            kind: EffectKind::$kind,
            category: $cat,
            match_names: &[$($mn),*],
            params: &$params,
            accelerated: matches!(EffectKind::$kind, EffectKind::VideoEffect | EffectKind::VideoTransition | EffectKind::VideoFixed),
        }
    };
}

pub static CATALOG: &[EffectDef] = &[
    // fixed
    def!(
        MOTION,
        "Motion",
        VideoFixed,
        "",
        ["AE.ADBE Motion"],
        MOTION_PARAMS
    ),
    def!(
        OPACITY,
        "Opacity",
        VideoFixed,
        "",
        ["AE.ADBE Opacity"],
        OPACITY_PARAMS
    ),
    def!(VOLUME, "Volume", AudioFixed, "", [], VOLUME_PARAMS),
    def!(
        CHANNEL_VOLUME,
        "Channel Volume",
        AudioFixed,
        "",
        [],
        CHANNEL_VOLUME_PARAMS
    ),
    def!(PANNER, "Panner", AudioFixed, "", [], PANNER_PARAMS),
    // graphics
    def!(TEXT, "Text", Graphic, "", ["AE.ADBE Text"], TEXT_PARAMS),
    def!(SHAPE, "Shape", Graphic, "", ["AE.ADBE Shape"], SHAPE_PARAMS),
    // video effects
    def!(
        "op.video.brightness_contrast",
        "Brightness & Contrast",
        VideoEffect,
        CAT_COLOR,
        [],
        BRIGHTNESS_CONTRAST
    ),
    def!(
        "op.video.procamp",
        "ProcAmp",
        VideoEffect,
        CAT_ADJUST,
        [],
        PROCAMP
    ),
    def!(
        "op.video.levels",
        "Levels",
        VideoEffect,
        CAT_ADJUST,
        [],
        LEVELS
    ),
    def!(
        "op.video.channel_mixer",
        "Channel Mixer",
        VideoEffect,
        CAT_ADJUST,
        [],
        CHANNEL_MIXER
    ),
    def!(
        "op.video.extract",
        "Extract",
        VideoEffect,
        CAT_ADJUST,
        [],
        EXTRACT
    ),
    def!(
        "op.video.gaussian_blur",
        "Gaussian Blur",
        VideoEffect,
        CAT_BLUR,
        [],
        GAUSSIAN_BLUR
    ),
    def!(
        "op.video.directional_blur",
        "Directional Blur",
        VideoEffect,
        CAT_BLUR,
        [],
        DIRECTIONAL_BLUR
    ),
    def!(
        "op.video.sharpen",
        "Sharpen",
        VideoEffect,
        CAT_BLUR,
        [],
        SHARPEN
    ),
    def!(
        "op.video.unsharp_mask",
        "Unsharp Mask",
        VideoEffect,
        CAT_BLUR,
        [],
        UNSHARP_MASK
    ),
    def!(
        "op.video.invert",
        "Invert",
        VideoEffect,
        CAT_CHANNEL,
        [],
        INVERT
    ),
    def!(
        "op.video.lumetri",
        "Lumetri Color",
        VideoEffect,
        CAT_COLOR,
        ["AE.ADBE Lumetri"],
        LUMETRI
    ),
    def!("op.video.tint", "Tint", VideoEffect, CAT_COLOR, [], TINT),
    def!(
        "op.video.black_white",
        "Black & White",
        VideoEffect,
        CAT_IMAGE,
        [],
        NO_PARAMS
    ),
    def!(
        "op.video.color_balance",
        "Color Balance",
        VideoEffect,
        CAT_COLOR,
        [],
        COLOR_BALANCE
    ),
    def!(
        "op.video.leave_color",
        "Leave Color",
        VideoEffect,
        CAT_COLOR,
        [],
        LEAVE_COLOR
    ),
    def!(
        "op.video.transform",
        "Transform",
        VideoEffect,
        CAT_DISTORT,
        [],
        TRANSFORM
    ),
    def!(
        "op.video.mirror",
        "Mirror",
        VideoEffect,
        CAT_DISTORT,
        [],
        MIRROR
    ),
    def!(
        "op.video.corner_pin",
        "Corner Pin",
        VideoEffect,
        CAT_DISTORT,
        [],
        CORNER_PIN
    ),
    def!(
        "op.video.offset",
        "Offset",
        VideoEffect,
        CAT_DISTORT,
        [],
        OFFSET
    ),
    def!(
        "op.video.spherize",
        "Spherize",
        VideoEffect,
        CAT_DISTORT,
        [],
        SPHERIZE
    ),
    def!(
        "op.video.twirl",
        "Twirl",
        VideoEffect,
        CAT_DISTORT,
        ["AE.ADBE Twirl"],
        TWIRL
    ),
    def!(
        "op.video.wave_warp",
        "Wave Warp",
        VideoEffect,
        CAT_DISTORT,
        [],
        WAVE_WARP
    ),
    def!("op.video.ramp", "Ramp", VideoEffect, CAT_GENERATE, [], RAMP),
    def!(
        "op.video.gamma",
        "Gamma Correction",
        VideoEffect,
        CAT_IMAGE,
        [],
        GAMMA
    ),
    def!(
        "op.video.color_replace",
        "Color Replace",
        VideoEffect,
        CAT_IMAGE,
        [],
        COLOR_REPLACE
    ),
    def!(
        "op.video.ultra_key",
        "Ultra Key",
        VideoEffect,
        CAT_KEYING,
        [],
        ULTRA_KEY
    ),
    def!(
        "op.video.color_key",
        "Color Key",
        VideoEffect,
        CAT_KEYING,
        [],
        COLOR_KEY
    ),
    def!(
        "op.video.luma_key",
        "Luma Key",
        VideoEffect,
        CAT_KEYING,
        [],
        LUMA_KEY
    ),
    def!(
        "op.video.track_matte",
        "Track Matte Key",
        VideoEffect,
        CAT_KEYING,
        [],
        TRACK_MATTE
    ),
    def!("op.video.noise", "Noise", VideoEffect, CAT_NOISE, [], NOISE),
    def!(
        "op.video.drop_shadow",
        "Drop Shadow",
        VideoEffect,
        CAT_PERSPECTIVE,
        [],
        DROP_SHADOW
    ),
    def!(
        "op.video.basic_3d",
        "Basic 3D",
        VideoEffect,
        CAT_PERSPECTIVE,
        [],
        BASIC_3D
    ),
    def!(
        "op.video.mosaic",
        "Mosaic",
        VideoEffect,
        CAT_STYLIZE,
        ["AE.ADBE Mosaic"],
        MOSAIC
    ),
    def!(
        "op.video.posterize",
        "Posterize",
        VideoEffect,
        CAT_STYLIZE,
        [],
        POSTERIZE
    ),
    def!(
        "op.video.find_edges",
        "Find Edges",
        VideoEffect,
        CAT_STYLIZE,
        [],
        FIND_EDGES
    ),
    def!(
        "op.video.emboss",
        "Emboss",
        VideoEffect,
        CAT_STYLIZE,
        [],
        EMBOSS
    ),
    def!(
        "op.video.solarize",
        "Solarize",
        VideoEffect,
        CAT_STYLIZE,
        ["PR.ADBE Solarize"],
        SOLARIZE
    ),
    def!(
        "op.video.replicate",
        "Replicate",
        VideoEffect,
        CAT_STYLIZE,
        [],
        REPLICATE
    ),
    def!(
        "op.video.threshold",
        "Threshold",
        VideoEffect,
        CAT_STYLIZE,
        [],
        THRESHOLD
    ),
    def!(
        "op.video.posterize_time",
        "Posterize Time",
        VideoEffect,
        CAT_TIME,
        [],
        POSTERIZE_TIME
    ),
    def!(
        "op.video.crop",
        "Crop",
        VideoEffect,
        CAT_TRANSFORM,
        [],
        CROP
    ),
    def!(
        "op.video.horizontal_flip",
        "Horizontal Flip",
        VideoEffect,
        CAT_TRANSFORM,
        [],
        NO_PARAMS
    ),
    def!(
        "op.video.vertical_flip",
        "Vertical Flip",
        VideoEffect,
        CAT_TRANSFORM,
        [],
        NO_PARAMS
    ),
    def!(
        "op.video.edge_feather",
        "Edge Feather",
        VideoEffect,
        CAT_TRANSFORM,
        [],
        EDGE_FEATHER
    ),
    def!(
        "op.video.linear_wipe",
        "Linear Wipe",
        VideoEffect,
        CAT_TRANSITION,
        [],
        LINEAR_WIPE
    ),
    def!(
        "op.video.radial_wipe",
        "Radial Wipe",
        VideoEffect,
        CAT_TRANSITION,
        [],
        RADIAL_WIPE
    ),
    def!(
        "op.video.timecode",
        "Timecode",
        VideoEffect,
        CAT_VIDEO,
        [],
        TIMECODE
    ),
    // video transitions
    def!(
        CROSS_DISSOLVE,
        "Cross Dissolve",
        VideoTransition,
        CAT_TR_DISSOLVE,
        [],
        TR_NONE
    ),
    def!(
        "op.tr.dip_to_black",
        "Dip to Black",
        VideoTransition,
        CAT_TR_DISSOLVE,
        [],
        TR_NONE
    ),
    def!(
        "op.tr.dip_to_white",
        "Dip to White",
        VideoTransition,
        CAT_TR_DISSOLVE,
        [],
        TR_NONE
    ),
    def!(
        "op.tr.film_dissolve",
        "Film Dissolve",
        VideoTransition,
        CAT_TR_DISSOLVE,
        [],
        TR_NONE
    ),
    def!(
        "op.tr.additive_dissolve",
        "Additive Dissolve",
        VideoTransition,
        CAT_TR_DISSOLVE,
        [],
        TR_NONE
    ),
    def!(
        "op.tr.non_additive_dissolve",
        "Non-Additive Dissolve",
        VideoTransition,
        CAT_TR_DISSOLVE,
        [],
        TR_NONE
    ),
    def!(
        "op.tr.iris_round",
        "Iris Round",
        VideoTransition,
        CAT_TR_IRIS,
        [],
        TR_IRIS
    ),
    def!(
        "op.tr.iris_box",
        "Iris Box",
        VideoTransition,
        CAT_TR_IRIS,
        [],
        TR_IRIS
    ),
    def!(
        "op.tr.iris_cross",
        "Iris Cross",
        VideoTransition,
        CAT_TR_IRIS,
        [],
        TR_IRIS
    ),
    def!(
        "op.tr.iris_diamond",
        "Iris Diamond",
        VideoTransition,
        CAT_TR_IRIS,
        [],
        TR_IRIS
    ),
    def!(
        "op.tr.push",
        "Push",
        VideoTransition,
        CAT_TR_SLIDE,
        [],
        TR_DIRECTIONAL
    ),
    def!(
        "op.tr.slide",
        "Slide",
        VideoTransition,
        CAT_TR_SLIDE,
        [],
        TR_DIRECTIONAL8
    ),
    def!(
        "op.tr.split",
        "Split",
        VideoTransition,
        CAT_TR_SLIDE,
        [],
        TR_SPLIT
    ),
    def!(
        "op.tr.whip",
        "Whip",
        VideoTransition,
        CAT_TR_SLIDE,
        [],
        TR_WHIP
    ),
    def!(
        "op.tr.center_split",
        "Center Split",
        VideoTransition,
        CAT_TR_SLIDE,
        [],
        TR_NONE
    ),
    def!(
        "op.tr.wipe",
        "Wipe",
        VideoTransition,
        CAT_TR_WIPE,
        [],
        TR_WIPE
    ),
    def!(
        "op.tr.barn_doors",
        "Barn Doors",
        VideoTransition,
        CAT_TR_WIPE,
        [],
        TR_SPLIT
    ),
    def!(
        "op.tr.clock_wipe",
        "Clock Wipe",
        VideoTransition,
        CAT_TR_WIPE,
        [],
        TR_CLOCK
    ),
    def!(
        "op.tr.radial_wipe",
        "Radial Wipe",
        VideoTransition,
        CAT_TR_WIPE,
        [],
        TR_CLOCK
    ),
    def!(
        "op.tr.inset",
        "Inset",
        VideoTransition,
        CAT_TR_WIPE,
        [],
        TR_DIRECTIONAL8
    ),
    def!(
        "op.tr.venetian_blinds",
        "Venetian Blinds",
        VideoTransition,
        CAT_TR_WIPE,
        [],
        TR_BLINDS
    ),
    def!(
        "op.tr.checker_wipe",
        "Checker Wipe",
        VideoTransition,
        CAT_TR_WIPE,
        [],
        TR_CHECKER
    ),
    def!(
        "op.tr.random_blocks",
        "Random Blocks",
        VideoTransition,
        CAT_TR_WIPE,
        [],
        TR_BLOCKS
    ),
    def!(
        "op.tr.cross_zoom",
        "Cross Zoom",
        VideoTransition,
        CAT_TR_ZOOM,
        [],
        TR_ZOOM
    ),
    def!(
        "op.tr.flip_over",
        "Flip Over",
        VideoTransition,
        CAT_TR_3D,
        [],
        TR_FLIP
    ),
    // audio effects
    def!(
        "op.audio.amplify",
        "Amplify",
        AudioEffect,
        CAT_AUDIO_AMPLITUDE,
        [],
        AMPLIFY
    ),
    def!(
        "op.audio.hard_limiter",
        "Hard Limiter",
        AudioEffect,
        CAT_AUDIO_AMPLITUDE,
        [],
        HARD_LIMITER
    ),
    def!(
        "op.audio.compressor",
        "Single-band Compressor",
        AudioEffect,
        CAT_AUDIO_AMPLITUDE,
        [],
        COMPRESSOR
    ),
    def!(
        "op.audio.delay",
        "Delay",
        AudioEffect,
        CAT_AUDIO_DELAY,
        [],
        DELAY
    ),
    def!(
        "op.audio.bass",
        "Bass",
        AudioEffect,
        CAT_AUDIO_FILTER,
        [],
        BASS
    ),
    def!(
        "op.audio.treble",
        "Treble",
        AudioEffect,
        CAT_AUDIO_FILTER,
        [],
        TREBLE
    ),
    def!(
        "op.audio.highpass",
        "Highpass",
        AudioEffect,
        CAT_AUDIO_FILTER,
        [],
        HIGHPASS
    ),
    def!(
        "op.audio.lowpass",
        "Lowpass",
        AudioEffect,
        CAT_AUDIO_FILTER,
        [],
        LOWPASS
    ),
    def!(
        "op.audio.bandpass",
        "Bandpass",
        AudioEffect,
        CAT_AUDIO_FILTER,
        [],
        BANDPASS
    ),
    def!(
        "op.audio.notch",
        "Notch Filter",
        AudioEffect,
        CAT_AUDIO_FILTER,
        [],
        NOTCH
    ),
    def!(
        "op.audio.simple_eq",
        "Simple Parametric EQ",
        AudioEffect,
        CAT_AUDIO_FILTER,
        [],
        SIMPLE_EQ
    ),
    def!(
        "op.audio.parametric_eq",
        "Parametric Equalizer",
        AudioEffect,
        CAT_AUDIO_FILTER,
        [],
        PARAMETRIC_EQ
    ),
    def!(
        "op.audio.chorus",
        "Chorus/Flanger",
        AudioEffect,
        CAT_AUDIO_MODULATION,
        [],
        CHORUS
    ),
    def!(
        "op.audio.reverb",
        "Studio Reverb",
        AudioEffect,
        CAT_AUDIO_REVERB,
        [],
        REVERB
    ),
    def!(
        "op.audio.invert",
        "Invert",
        AudioEffect,
        CAT_AUDIO_SPECIAL,
        [],
        NO_PARAMS
    ),
    def!(
        "op.audio.swap_channels",
        "Swap Channels",
        AudioEffect,
        CAT_AUDIO_STEREO,
        [],
        NO_PARAMS
    ),
    def!(
        "op.audio.fill_left",
        "Fill Left with Right",
        AudioEffect,
        CAT_AUDIO_STEREO,
        [],
        NO_PARAMS
    ),
    def!(
        "op.audio.fill_right",
        "Fill Right with Left",
        AudioEffect,
        CAT_AUDIO_STEREO,
        [],
        NO_PARAMS
    ),
    def!(
        "op.audio.stereo_expander",
        "Stereo Expander",
        AudioEffect,
        CAT_AUDIO_STEREO,
        [],
        STEREO_EXPANDER
    ),
    // audio transitions
    def!(
        CONSTANT_POWER,
        "Constant Power",
        AudioTransition,
        CAT_CROSSFADE,
        [],
        NO_PARAMS
    ),
    def!(
        "op.atr.constant_gain",
        "Constant Gain",
        AudioTransition,
        CAT_CROSSFADE,
        [],
        NO_PARAMS
    ),
    def!(
        "op.atr.exponential_fade",
        "Exponential Fade",
        AudioTransition,
        CAT_CROSSFADE,
        [],
        NO_PARAMS
    ),
];

/// Looks up a definition by our ID.
pub fn find(id: &str) -> Option<&'static EffectDef> {
    CATALOG.iter().find(|d| d.id == id)
}

/// Looks up a definition by an interchange identity (for importers).
pub fn find_match_name(name: &str) -> Option<&'static EffectDef> {
    CATALOG.iter().find(|d| d.match_names.contains(&name))
}

pub fn by_kind(kind: EffectKind) -> impl Iterator<Item = &'static EffectDef> {
    CATALOG.iter().filter(move |d| d.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_and_param_keys_are_unique() {
        let mut ids = HashSet::new();
        for d in CATALOG {
            assert!(ids.insert(d.id), "duplicate {}", d.id);
            let mut keys = HashSet::new();
            for p in d.params {
                assert!(keys.insert(p.key), "{}: duplicate param {}", d.id, p.key);
                // defaults satisfy the limits
                assert_eq!(
                    p.clamp(p.default_value()),
                    p.default_value(),
                    "{}.{}",
                    d.id,
                    p.key
                );
            }
        }
        assert_eq!(BLEND_MODES.len(), 27);
    }

    #[test]
    fn match_names_resolve() {
        assert_eq!(find_match_name("AE.ADBE Motion").unwrap().id, MOTION);
        assert_eq!(
            find(CROSS_DISSOLVE).unwrap().kind,
            EffectKind::VideoTransition
        );
    }
}
