//! Pixel tests on the available GPU (a software adapter in CI).

use std::sync::Arc;

use op_core::catalog::{self, EffectKind};
use op_core::*;
use op_render::*;
use op_timeline::*;

fn gpu() -> Option<Arc<Gpu>> {
    match Gpu::headless() {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("skipping GPU tests: {e}");
            None
        }
    }
}

struct Scene {
    p: Project,
    seq: SequenceId,
}

impl Scene {
    fn new() -> Scene {
        let mut p = Project::new("gpu");
        let settings = SequenceSettings {
            width: 64,
            height: 36,
            rate: Rate::FPS_25,
            ..SequenceSettings::default()
        };
        let (seq, _) = p.add_sequence(p.root, "S", settings);
        Scene { p, seq }
    }

    fn matte(&mut self, color: Rgba) -> ItemId {
        let root = self.p.root;
        self.p.add_item(
            root,
            "Matte",
            ItemKind::Synthetic {
                generator: Generator::ColorMatte { color },
                duration: Dur::from_seconds(60.0),
            },
        )
    }

    fn put(&mut self, item: ItemId, track: usize, from: f64, to: f64) -> ClipId {
        let mut spec = SourceClip::from_item(&self.p, item).unwrap();
        spec.range = SrcRange::new(SrcTime::ZERO, SrcTime::from_seconds(to - from));
        let seq = self.seq;
        let (next, ids) = self
            .p
            .transact(|p| {
                overwrite(
                    p,
                    seq,
                    &spec,
                    SeqTime::from_seconds(from),
                    &Patch {
                        video: Some(track),
                        audio: vec![],
                    },
                    EditOptions::default(),
                )
            })
            .unwrap();
        self.p = next;
        ids[0]
    }

    fn set(&mut self, clip: ClipId, effect: &str, key: &str, v: Value) {
        let c = self
            .p
            .sequence_mut(self.seq)
            .unwrap()
            .clip_mut(clip)
            .unwrap();
        let comp = match c.component_mut(effect) {
            Some(x) => x,
            None => {
                let mut ids = IdGen::default();
                ids.observe(10_000 + c.components.len() as u64);
                c.components
                    .push(Component::new(catalog::find(effect).unwrap(), &mut ids));
                c.components.last_mut().unwrap()
            }
        };
        comp.param_mut(key).unwrap().value = v;
    }

    fn render(&self, r: &mut Renderer, t: f64) -> Vec<u8> {
        let frame = r.render(&Request {
            project: &self.p,
            sequence: self.seq,
            time: SeqTime::from_seconds(t),
            scale: 1.0,
            source: &NoFrames,
        });
        let px = r.rgba8(&frame);
        r.recycle(frame);
        px
    }
}

fn at(px: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * 64 + x) * 4;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

fn near(a: [u8; 4], b: [u8; 4], tol: i32) -> bool {
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| (*x as i32 - *y as i32).abs() <= tol)
}

#[test]
fn compositing_effects_and_transitions() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    // every shader compiles and validates
    r.warm_up();

    // a red matte fills the frame
    let mut s = Scene::new();
    let red = s.matte(Rgba::new(1.0, 0.0, 0.0, 1.0));
    let blue = s.matte(Rgba::new(0.0, 0.0, 1.0, 1.0));
    let a = s.put(red, 0, 0.0, 10.0);
    let px = s.render(&mut r, 1.0);
    assert!(
        near(at(&px, 10, 10), [255, 0, 0, 255], 1),
        "{:?}",
        at(&px, 10, 10)
    );

    // opacity 50 %: straight red with half alpha
    s.set(a, catalog::OPACITY, "opacity", Value::Float(50.0));
    let px = s.render(&mut r, 1.0);
    assert!(
        near(at(&px, 32, 18), [255, 0, 0, 128], 2),
        "{:?}",
        at(&px, 32, 18)
    );
    s.set(a, catalog::OPACITY, "opacity", Value::Float(100.0));

    // V2 blue at 50 % scale: center blue, corner red
    let b = s.put(blue, 1, 0.0, 10.0);
    s.set(b, catalog::MOTION, "scale", Value::Float(50.0));
    let px = s.render(&mut r, 1.0);
    assert!(
        near(at(&px, 32, 18), [0, 0, 255, 255], 2),
        "{:?}",
        at(&px, 32, 18)
    );
    assert!(
        near(at(&px, 2, 2), [255, 0, 0, 255], 2),
        "{:?}",
        at(&px, 2, 2)
    );

    // moving V2 to the right half via Position
    s.set(b, catalog::MOTION, "position", Value::Point([0.75, 0.5]));
    let px = s.render(&mut r, 1.0);
    assert!(near(at(&px, 48, 18), [0, 0, 255, 255], 2));
    assert!(near(at(&px, 20, 18), [255, 0, 0, 255], 2));

    // multiply: blue over red -> black
    s.set(b, catalog::MOTION, "scale", Value::Float(100.0));
    s.set(b, catalog::MOTION, "position", Value::Point([0.5, 0.5]));
    s.set(b, catalog::OPACITY, "blend_mode", Value::Choice(3));
    let px = s.render(&mut r, 1.0);
    assert!(
        near(at(&px, 32, 18), [0, 0, 0, 255], 2),
        "{:?}",
        at(&px, 32, 18)
    );
    // screen: blue over red -> magenta
    s.set(b, catalog::OPACITY, "blend_mode", Value::Choice(8));
    let px = s.render(&mut r, 1.0);
    assert!(
        near(at(&px, 32, 18), [255, 0, 255, 255], 2),
        "{:?}",
        at(&px, 32, 18)
    );

    // cross dissolve half way between red and blue on one track
    let mut s = Scene::new();
    let red = s.matte(Rgba::new(1.0, 0.0, 0.0, 1.0));
    let blue = s.matte(Rgba::new(0.0, 0.0, 1.0, 1.0));
    s.put(red, 0, 0.0, 5.0);
    s.put(blue, 0, 5.0, 10.0);
    let seq = s.seq;
    let (next, _) =
        s.p.transact(|p| {
            add_transition(
                p,
                seq,
                TrackRef::video(0),
                SeqTime::from_seconds(5.0),
                catalog::CROSS_DISSOLVE,
                Dur::from_seconds(1.0),
                Some(Alignment::CenterAtCut),
                EditOptions::default(),
            )
        })
        .unwrap();
    s.p = next;
    let tr_range = s.p.sequence(seq).unwrap().video[0].transitions[0].range();
    let mid = (tr_range.start.seconds() + tr_range.end.seconds()) / 2.0;
    let px = s.render(&mut r, mid);
    assert!(
        near(at(&px, 32, 18), [128, 0, 128, 255], 3),
        "{:?}",
        at(&px, 32, 18)
    );

    // every video transition renders
    for def in catalog::by_kind(EffectKind::VideoTransition) {
        s.p.sequence_mut(seq)
            .unwrap()
            .track_mut(TrackRef::video(0))
            .unwrap()
            .transitions[0]
            .effect = def.id.to_string();
        let px = s.render(&mut r, mid);
        assert_eq!(px.len(), 64 * 36 * 4, "{}", def.id);
    }

    // every video effect renders over the red matte
    let mut s = Scene::new();
    let red = s.matte(Rgba::new(0.9, 0.2, 0.1, 1.0));
    let c = s.put(red, 0, 0.0, 10.0);
    for def in catalog::by_kind(EffectKind::VideoEffect) {
        let comp = {
            let mut ids = s.p.ids.clone();
            let comp = Component::new(def, &mut ids);
            s.p.ids = ids;
            comp
        };
        s.p.sequence_mut(s.seq)
            .unwrap()
            .clip_mut(c)
            .unwrap()
            .components
            .push(comp);
        let px = s.render(&mut r, 1.0);
        assert_eq!(px.len(), 64 * 36 * 4, "{}", def.id);
        s.p.sequence_mut(s.seq)
            .unwrap()
            .clip_mut(c)
            .unwrap()
            .components
            .pop();
    }

    // looks: bars cover the top rows, a vignette darkens the corners, glow brightens
    let base = s.render(&mut r, 1.0);
    s.set(c, "op.video.letterbox", "aspect", Value::Choice(0));
    let px = s.render(&mut r, 1.0);
    assert!(
        near(at(&px, 32, 1), [0, 0, 0, 255], 2),
        "{:?}",
        at(&px, 32, 1)
    );
    assert!(near(at(&px, 32, 18), at(&base, 32, 18), 2));
    s.p.sequence_mut(s.seq)
        .unwrap()
        .clip_mut(c)
        .unwrap()
        .components
        .retain(|x| x.effect != "op.video.letterbox");
    s.set(c, "op.video.vignette", "amount", Value::Float(100.0));
    let px = s.render(&mut r, 1.0);
    assert!(
        at(&px, 0, 0)[0] < at(&base, 0, 0)[0] / 2,
        "{:?}",
        at(&px, 0, 0)
    );
    s.p.sequence_mut(s.seq)
        .unwrap()
        .clip_mut(c)
        .unwrap()
        .components
        .retain(|x| x.effect != "op.video.vignette");
    for glow in ["op.video.glow", "op.video.radiant_glow"] {
        s.set(c, glow, "threshold", Value::Float(0.0));
        let px = s.render(&mut r, 1.0);
        assert!(
            at(&px, 32, 18)[1] > at(&base, 32, 18)[1],
            "{glow}: {:?}",
            at(&px, 32, 18)
        );
        s.p.sequence_mut(s.seq)
            .unwrap()
            .clip_mut(c)
            .unwrap()
            .components
            .retain(|x| x.effect != glow);
    }
}

#[test]
fn graphics_and_delivery() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let mut s = Scene::new();
    // a graphics clip with white text
    let seq = s.seq;
    let mut ids = s.p.ids.clone();
    let mut text = Component::new(catalog::find(catalog::TEXT).unwrap(), &mut ids);
    text.param_mut("text").unwrap().value = Value::Text("OK".into());
    text.param_mut("font_size").unwrap().value = Value::Float(30.0);
    let clip = Clip {
        id: ids.clip(),
        name: "Title".into(),
        kind: TrackKind::Video,
        source: ClipSource::Graphic,
        start: SeqTime::ZERO,
        duration: Dur::from_seconds(5.0),
        source_in: SrcTime::ZERO,
        speed: Speed::NORMAL,
        reverse: false,
        hold: None,
        enabled: true,
        link: None,
        group: None,
        label: Label::None,
        components: {
            let mut v = default_components(EffectKind::VideoFixed, &mut ids);
            v.push(text);
            v
        },
        gain_db: 0.0,
        scale_to_frame: false,
        channels: None,
    };
    s.p.ids = ids;
    s.p.sequence_mut(seq)
        .unwrap()
        .track_mut(TrackRef::video(0))
        .unwrap()
        .clips
        .push(clip);
    let px = s.render(&mut r, 1.0);
    let white = px.chunks(4).filter(|p| p[0] > 200 && p[3] > 200).count();
    assert!(white > 20, "text drew {white} pixels");

    // delivery planes of pure red: BT.709 limited range Y=63 Cb=102 Cr=240
    let mut s = Scene::new();
    let red = s.matte(Rgba::new(1.0, 0.0, 0.0, 1.0));
    s.put(red, 0, 0.0, 10.0);
    let frame = r.render(&Request {
        project: &s.p,
        sequence: s.seq,
        time: SeqTime::from_seconds(1.0),
        scale: 1.0,
        source: &NoFrames,
    });
    let planes = r.delivery_planes(&frame, op_media::VideoInput::Yuv420p);
    assert_eq!(planes.len(), 3);
    assert_eq!(planes[0].len(), 64 * 36);
    assert_eq!(planes[1].len(), 32 * 18);
    assert!(
        (planes[0][100] as i32 - 63).abs() <= 1,
        "Y {}",
        planes[0][100]
    );
    assert!(
        (planes[1][10] as i32 - 102).abs() <= 1,
        "Cb {}",
        planes[1][10]
    );
    assert!(
        (planes[2][10] as i32 - 240).abs() <= 1,
        "Cr {}",
        planes[2][10]
    );
    let p10 = r.delivery_planes(&frame, op_media::VideoInput::Yuv422p10);
    let y10 = u16::from_le_bytes([p10[0][0], p10[0][1]]);
    assert!((y10 as i32 - 250).abs() <= 2, "Y10 {y10}");

    // scopes draw without validation errors
    let target = r.display_target(256, 128);
    for kind in ScopeKind::ALL {
        r.scope(&frame, kind, &target, 1.0);
    }
    r.submit();
    let scope_px = r.read_texture(&target, 4);
    assert_eq!(scope_px.len(), 256 * 128 * 4);
    r.recycle(frame);
}

/// A media clip whose frames are one fixed decoded frame.
struct OneFrame(Arc<op_media::VideoFrame>);

impl FrameSource for OneFrame {
    fn video_frame(&self, _a: &MediaAsset, _t: SrcTime) -> Option<Arc<op_media::VideoFrame>> {
        Some(self.0.clone())
    }
}

/// Hardware decoders deliver NV12 / P010: the GPU conversion matches the reference one.
#[test]
fn semi_planar_frames_convert_like_the_reference() {
    let Some(gpu) = gpu() else { return };
    let mut r = Renderer::new(gpu);
    let (w, h) = (64u32, 36u32);
    for wide in [false, true] {
        // limited-range BT.709 orange-ish: Y=150, Cb=80, Cr=190 (8-bit codes)
        let (yv, cb, cr) = (150u16, 80u16, 190u16);
        let put = |v: u16| -> Vec<u8> {
            if wide {
                ((v as u32) << 8).min(65535).to_le_bytes()[..2].to_vec()
            } else {
                vec![v as u8]
            }
        };
        let bpp = if wide { 2 } else { 1 };
        let luma: Vec<u8> = (0..w * h).flat_map(|_| put(yv)).collect();
        let chroma: Vec<u8> = (0..(w / 2) * (h / 2))
            .flat_map(|_| [put(cb), put(cr)].concat())
            .collect();
        let frame = op_media::VideoFrame {
            width: w,
            height: h,
            layout: op_media::PixelLayout::Nv12 { wide },
            planes: vec![
                op_media::Plane {
                    data: luma,
                    width: w,
                    height: h,
                    bytes_per_row: w * bpp,
                },
                op_media::Plane {
                    data: chroma,
                    width: w / 2,
                    height: h / 2,
                    bytes_per_row: w / 2 * 2 * bpp,
                },
            ],
            color: ColorInfo::default(),
            index: 0,
        };
        let reference = frame.to_rgba8();
        let mut p = Project::new("nv12");
        let asset = MediaAsset {
            id: AssetId(0),
            path: "x.mp4".into(),
            proxy: None,
            kind: MediaKind::Video,
            video: Some(VideoStream {
                index: 0,
                codec: "h264".into(),
                width: w,
                height: h,
                pixel_aspect: (1, 1),
                rate: Rate::FPS_25,
                frames: 250,
                pixel_format: "nv12".into(),
                bit_depth: 8,
                alpha: AlphaMode::None,
                color: ColorInfo::default(),
                field_order: FieldOrder::Progressive,
                start: Dur::ZERO,
                rotation: 0,
                timecode: None,
            }),
            audio: vec![],
            duration: Dur::from_seconds(10.0),
            interpretation: Interpretation::default(),
            file_size: 0,
            modified_unix: 0,
        };
        let (_, item) = p.add_asset(p.root, asset);
        let (seq, _) = p.add_sequence(
            p.root,
            "S",
            SequenceSettings {
                width: w,
                height: h,
                rate: Rate::FPS_25,
                ..SequenceSettings::default()
            },
        );
        let spec = SourceClip::from_item(&p, item).unwrap();
        let (p, _) = p
            .transact(|p| {
                overwrite(
                    p,
                    seq,
                    &spec,
                    SeqTime::ZERO,
                    &Patch {
                        video: Some(0),
                        audio: vec![],
                    },
                    EditOptions::default(),
                )
            })
            .unwrap();
        let src = OneFrame(Arc::new(frame));
        let out = r.render(&Request {
            project: &p,
            sequence: seq,
            time: SeqTime::from_seconds(1.0),
            scale: 1.0,
            source: &src,
        });
        let px = r.rgba8(&out);
        r.recycle(out);
        let i = ((h / 2) * w + w / 2) as usize * 4;
        for k in 0..3 {
            assert!(
                (px[i + k] as i32 - reference[i + k] as i32).abs() <= 3,
                "wide={wide}: {:?} vs {:?}",
                &px[i..i + 4],
                &reference[i..i + 4]
            );
        }
    }
}
