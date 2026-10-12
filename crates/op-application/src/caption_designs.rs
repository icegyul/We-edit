//! Caption designs: ready-made looks for Korean captions (variety-show yellow, news box, speech
//! caption, shorts word highlight, neon, handwriting ...). A design is a font plus the values of the
//! Caption component; it applies to captions that exist and to captions about to be made.
//!
//! The fonts are open-licence (SIL OFL 1.1) families of the Google Fonts project. They are not
//! shipped with the program: the first use of a design downloads its one font file from GitHub
//! (pinned to a commit) into the program's fonts folder.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use op_core::catalog::CAPTION_STYLES;
use op_core::*;
use parking_lot::Mutex;

use crate::editor::Editor;

/// google/fonts commit the font files are fetched from (files never change under a commit).
const FONTS_COMMIT: &str = "66928797f271fed5ee428544e7229685e2d90127";

pub struct Design {
    /// Name shown in the menus.
    pub name: &'static str,
    pub note: &'static str,
    /// Family name the font file registers under.
    pub font: &'static str,
    /// Path of the font file under `ofl/` in the google/fonts repository.
    pub file: &'static str,
    /// Name in `CAPTION_STYLES` (the animation).
    pub style: &'static str,
    /// Font size as a fraction of the picture height.
    pub size: f64,
    pub fill: [f32; 3],
    /// Color of the word being spoken (or of the marker, for the highlighter styles).
    pub highlight: [f32; 3],
    /// Outline color and its width as a fraction of the font size.
    pub stroke: Option<([f32; 3], f64)>,
    /// Box color and opacity in percent.
    pub boxed: Option<([f32; 3], f64)>,
    pub shadow: bool,
}

const fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 255) as f32 / 255.0,
        ((hex >> 8) & 255) as f32 / 255.0,
        (hex & 255) as f32 / 255.0,
    ]
}

pub const DESIGNS: &[Design] = &[
    Design {
        name: "예능 노랑",
        note: "굵은 노란 글씨에 두꺼운 검정 테두리 (예능 자막)",
        font: "Black Han Sans",
        file: "blackhansans/BlackHanSans-Regular.ttf",
        style: "Pop",
        size: 0.075,
        fill: rgb(0xFFE600),
        highlight: rgb(0xFFFFFF),
        stroke: Some((rgb(0x000000), 0.14)),
        boxed: None,
        shadow: true,
    },
    Design {
        name: "예능 빨강 강조",
        note: "흰 글씨, 말하는 단어만 빨갛게 (예능 자막)",
        font: "Do Hyeon",
        file: "dohyeon/DoHyeon-Regular.ttf",
        style: "Karaoke",
        size: 0.08,
        fill: rgb(0xFFFFFF),
        highlight: rgb(0xFF2D2D),
        stroke: Some((rgb(0x000000), 0.13)),
        boxed: None,
        shadow: true,
    },
    Design {
        name: "말자막 (둥근체)",
        note: "둥글둥글한 글씨에 분홍 테두리",
        font: "Jua",
        file: "jua/Jua-Regular.ttf",
        style: "Classic",
        size: 0.07,
        fill: rgb(0xFFFFFF),
        highlight: rgb(0xFFD84D),
        stroke: Some((rgb(0xFF4F8B), 0.16)),
        boxed: None,
        shadow: true,
    },
    Design {
        name: "뉴스 자막",
        note: "남색 바탕 위 흰 글씨 (뉴스·정보 영상)",
        font: "NanumGothic",
        file: "nanumgothic/NanumGothic-ExtraBold.ttf",
        style: "Boxed",
        size: 0.055,
        fill: rgb(0xFFFFFF),
        highlight: rgb(0xFFD84D),
        stroke: None,
        boxed: Some((rgb(0x0A1A3F), 85.0)),
        shadow: false,
    },
    Design {
        name: "다큐 하단 자막",
        note: "테두리 없는 깔끔한 흰 글씨 (다큐·인터뷰)",
        font: "IBM Plex Sans KR",
        file: "ibmplexsanskr/IBMPlexSansKR-Bold.ttf",
        style: "Fade In",
        size: 0.05,
        fill: rgb(0xFFFFFF),
        highlight: rgb(0xFFD84D),
        stroke: None,
        boxed: None,
        shadow: true,
    },
    Design {
        name: "쇼츠 단어 강조",
        note: "큰 흰 글씨, 말하는 단어가 노랗게 켜짐 (쇼츠·릴스)",
        font: "Gothic A1",
        file: "gothica1/GothicA1-Black.ttf",
        style: "Karaoke",
        size: 0.08,
        fill: rgb(0xFFFFFF),
        highlight: rgb(0xFFD60A),
        stroke: Some((rgb(0x000000), 0.12)),
        boxed: None,
        shadow: true,
    },
    Design {
        name: "먹방 팝",
        note: "통통 튀는 주황 글씨 (먹방·브이로그)",
        font: "Bagel Fat One",
        file: "bagelfatone/BagelFatOne-Regular.ttf",
        style: "Bounce In",
        size: 0.10,
        fill: rgb(0xFF8A1F),
        highlight: rgb(0xFFE066),
        stroke: Some((rgb(0x3A1500), 0.14)),
        boxed: None,
        shadow: true,
    },
    Design {
        name: "네온",
        note: "하늘색 네온 불빛 (밤·음악 영상)",
        font: "Gugi",
        file: "gugi/Gugi-Regular.ttf",
        style: "Neon",
        size: 0.07,
        fill: rgb(0x7DF9FF),
        highlight: rgb(0xFF4DFF),
        stroke: None,
        boxed: None,
        shadow: false,
    },
    Design {
        name: "손글씨 감성",
        note: "손으로 쓴 듯한 글씨 (일상·감성 영상)",
        font: "Gaegu",
        file: "gaegu/Gaegu-Bold.ttf",
        style: "Fade In",
        size: 0.095,
        fill: rgb(0xFFFFFF),
        highlight: rgb(0xFFE6A8),
        stroke: Some((rgb(0x2B2B2B), 0.10)),
        boxed: None,
        shadow: false,
    },
    Design {
        name: "형광펜",
        note: "검정 글씨에 노란 형광펜이 칠해짐 (강의·설명)",
        font: "Sunflower",
        file: "sunflower/Sunflower-Bold.ttf",
        style: "Highlighter",
        size: 0.06,
        fill: rgb(0x111111),
        highlight: rgb(0xFFEB3B),
        stroke: None,
        boxed: None,
        shadow: false,
    },
    Design {
        name: "귀여운 큰 글씨",
        note: "깡충깡충 뛰는 큰 글씨 (키즈·귀여운 영상)",
        font: "Dongle",
        file: "dongle/Dongle-Bold.ttf",
        style: "Hop",
        size: 0.115,
        fill: rgb(0xFFFFFF),
        highlight: rgb(0xFFD1E3),
        stroke: Some((rgb(0xFF5C8A), 0.10)),
        boxed: None,
        shadow: true,
    },
    Design {
        name: "영화 명조",
        note: "타자기처럼 한 글자씩, 은은한 크림색 명조 (영화·단편)",
        font: "NanumMyeongjo",
        file: "nanummyeongjo/NanumMyeongjo-ExtraBold.ttf",
        style: "Typewriter",
        size: 0.058,
        fill: rgb(0xFFF6DC),
        highlight: rgb(0xFFFFFF),
        stroke: None,
        boxed: None,
        shadow: true,
    },
];

/// The Caption parameters that give a caption this design on a picture `height` pixels tall.
pub fn values(d: &Design, height: u32) -> Vec<(String, Value)> {
    let size = (f64::from(height) * d.size).round().max(12.0);
    let color = |c: [f32; 3]| Value::Color(Rgba::new(c[0], c[1], c[2], 1.0));
    let style = CAPTION_STYLES.iter().position(|s| *s == d.style).unwrap_or(0) as u32;
    let mut v = vec![
        ("style".to_string(), Value::Choice(style)),
        ("font".to_string(), Value::Text(d.font.to_string())),
        // bold: the closest weight of the family, so a family with several faces gets its heavy one
        ("font_style".to_string(), Value::Choice(1)),
        ("font_size".to_string(), Value::Float(size)),
        ("uppercase".to_string(), Value::Bool(false)),
        ("fill".to_string(), color(d.fill)),
        ("highlight".to_string(), color(d.highlight)),
        ("stroke".to_string(), Value::Bool(d.stroke.is_some())),
        ("shadow".to_string(), Value::Bool(d.shadow)),
    ];
    if let Some((c, ratio)) = d.stroke {
        v.push(("stroke_color".to_string(), color(c)));
        v.push(("stroke_width".to_string(), Value::Float((size * ratio).round())));
    }
    match d.boxed {
        Some((c, opacity)) => {
            v.push(("background_color".to_string(), color(c)));
            v.push(("background_opacity".to_string(), Value::Float(opacity)));
        }
        None => v.push(("background_opacity".to_string(), Value::Float(0.0))),
    }
    if d.shadow {
        v.push(("shadow_color".to_string(), color([0.0, 0.0, 0.0])));
        v.push(("shadow_opacity".to_string(), Value::Float(70.0)));
        v.push(("shadow_distance".to_string(), Value::Float((size * 0.06).round())));
        v.push(("shadow_blur".to_string(), Value::Float((size * 0.1).round())));
    }
    v
}

/// Whether the font of a design is available (system, program fonts folder, or downloaded).
pub fn font_ready(d: &Design) -> bool {
    op_render::text::Fonts::global().families().iter().any(|f| f == d.font)
}

/// A design's font being downloaded; when it arrives the design goes onto `targets`.
pub struct FontJob {
    pub design: usize,
    targets: Vec<ClipId>,
    result: Arc<Mutex<Option<Result<PathBuf, String>>>>,
}

impl Editor {
    /// Chooses a design: gets its font if the program lacks it, and gives the design to `targets`
    /// (captions that exist) once the font is there. With no targets only the font is fetched.
    pub fn use_caption_design(&mut self, design: usize, targets: Vec<ClipId>) {
        let Some(d) = DESIGNS.get(design) else { return };
        if font_ready(d) {
            self.apply_caption_design(design, &targets);
            return;
        }
        if self.design_font.is_some() {
            self.error("다른 글꼴을 내려받는 중입니다. 끝난 뒤 다시 눌러 주세요");
            return;
        }
        let file = d.file.rsplit('/').next().unwrap_or(d.file);
        let dest = self.dirs.data.join("Fonts").join(file);
        let url = format!("https://raw.githubusercontent.com/google/fonts/{FONTS_COMMIT}/ofl/{}", d.file);
        let result = Arc::new(Mutex::new(None));
        let r = result.clone();
        let spawned = std::thread::Builder::new().name("font-download".into()).spawn(move || {
            let res = crate::download::fetch_file(&url, &dest, &AtomicBool::new(false), &mut |_| {})
                .map(|_| dest);
            *r.lock() = Some(res);
        });
        if spawned.is_err() {
            self.error("글꼴 내려받기를 시작할 수 없습니다");
            return;
        }
        self.info(format!("글꼴 '{}'을(를) 내려받는 중입니다 (한 번만)", d.font));
        self.design_font = Some(FontJob { design, targets, result });
    }

    fn apply_caption_design(&mut self, design: usize, targets: &[ClipId]) {
        if targets.is_empty() {
            return;
        }
        let Some(d) = DESIGNS.get(design) else { return };
        let Some(height) = self.active_seq().map(|s| s.settings.height) else { return };
        self.caption_edit(|e| {
            e.set_caption_values(targets, values(d, height), None);
            e.seal();
        });
        self.info(format!("자막 디자인 '{}'을(를) 적용했습니다", d.name));
    }

    pub(crate) fn poll_design_font(&mut self) {
        let Some(res) = self.design_font.as_ref().and_then(|j| j.result.lock().take()) else {
            return;
        };
        let Some(job) = self.design_font.take() else { return };
        let Some(d) = DESIGNS.get(job.design) else { return };
        match res {
            Ok(path) => {
                op_render::text::Fonts::global().add_file(&path);
                if font_ready(d) {
                    self.info(format!("글꼴 '{}'을(를) 설치했습니다", d.font));
                    self.apply_caption_design(job.design, &job.targets);
                } else {
                    let _ = std::fs::remove_file(&path);
                    self.error(format!("글꼴 '{}' 파일을 읽을 수 없습니다", d.font));
                }
            }
            Err(e) => self.error(format!("글꼴을 내려받지 못했습니다: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_design_sets_parameters_the_caption_has() {
        let def = op_core::catalog::find(op_core::catalog::CAPTION).expect("caption effect");
        for d in DESIGNS {
            assert!(CAPTION_STYLES.contains(&d.style), "{}: style {}", d.name, d.style);
            assert!(d.file.ends_with(".ttf") && d.file.contains('/'), "{}", d.name);
            let v = values(d, 1080);
            for (k, _) in &v {
                assert!(def.params.iter().any(|p| p.key == k), "{}: no parameter {k}", d.name);
            }
            assert!(v.iter().any(|(k, val)| k == "font_size" && matches!(val, Value::Float(s) if *s >= 12.0)));
        }
    }
}
