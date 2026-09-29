//! Final Cut Pro 7 XML (`xmeml`) interchange: the XML exchange format that many editors,
//! including Premiere Pro, read and write.

use std::collections::HashMap;
use std::sync::Arc;

use op_core::catalog::{self, EffectKind};
use op_core::*;

use crate::ProjectError;
use crate::otio::{fit_assets, path_to_url, placeholder_asset, url_to_path};
use crate::xml::{self, Doc, Limits, Writer};

fn rate_block(w: &mut Writer, rate: Rate) {
    w.open("rate", &[]);
    w.leaf("timebase", &rate.timecode_base().to_string());
    w.leaf("ntsc", if rate.den == 1001 { "TRUE" } else { "FALSE" });
    w.close();
}

fn fr(d: Dur, rate: Rate) -> i64 {
    rate.dur_to_frames_round(d)
}

fn pathurl(path: &str) -> String {
    path_to_url(path).replacen("file://", "file://localhost", 1)
}

struct Exporter<'a> {
    p: &'a Project,
    files: HashMap<AssetId, String>,
    sequences: HashMap<SequenceId, String>,
    clip_ids: HashMap<ClipId, String>,
    next: usize,
}

impl<'a> Exporter<'a> {
    fn id(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}-{}", self.next)
    }

    fn sequence(&mut self, w: &mut Writer, sid: SequenceId, depth: usize) {
        let seq = self.p.sequence(sid).unwrap();
        let rate = seq.rate();
        if let Some(id) = self.sequences.get(&sid) {
            w.empty("sequence", &[("id", id)]);
            return;
        }
        let id = self.id("sequence");
        self.sequences.insert(sid, id.clone());
        // clip ids first so links can reference clips on any track
        for (_, c) in seq.clips() {
            let cid = self.id("clipitem");
            self.clip_ids.insert(c.id, cid);
        }
        w.open("sequence", &[("id", &id)]);
        w.leaf("name", &seq.name);
        w.leaf("duration", &fr(seq.duration(), rate).to_string());
        rate_block(w, rate);
        w.open("timecode", &[]);
        rate_block(w, rate);
        let fmt = TimecodeFormat::new(rate, seq.settings.drop_frame);
        w.leaf("string", &fmt.format(seq.settings.start_timecode));
        w.leaf("frame", &fr(seq.settings.start_timecode, rate).to_string());
        w.leaf("displayformat", if fmt.drop_frame { "DF" } else { "NDF" });
        w.close();
        w.open("media", &[]);
        w.open("video", &[]);
        w.open("format", &[]);
        w.open("samplecharacteristics", &[]);
        rate_block(w, rate);
        w.leaf("width", &seq.settings.width.to_string());
        w.leaf("height", &seq.settings.height.to_string());
        w.leaf(
            "pixelaspectratio",
            if seq.settings.pixel_aspect.0 == seq.settings.pixel_aspect.1 {
                "square"
            } else {
                "NTSC-601"
            },
        );
        w.leaf("fielddominance", "none");
        w.close();
        w.close();
        for t in &seq.video {
            self.track(w, seq, t, depth);
        }
        w.close();
        w.open("audio", &[]);
        w.leaf(
            "numOutputChannels",
            &seq.settings.master.channels().to_string(),
        );
        w.open("format", &[]);
        w.open("samplecharacteristics", &[]);
        w.leaf("depth", "16");
        w.leaf("samplerate", &seq.settings.audio_rate.to_string());
        w.close();
        w.close();
        for t in &seq.audio {
            self.track(w, seq, t, depth);
        }
        w.close();
        w.close();
        for m in &seq.markers {
            w.open("marker", &[]);
            w.leaf("name", &m.name);
            w.leaf("comment", &m.comment);
            w.leaf("in", &fr(m.start.since_zero(), rate).to_string());
            w.leaf(
                "out",
                &if m.duration.0 > 0 {
                    fr(m.end().since_zero(), rate)
                } else {
                    -1
                }
                .to_string(),
            );
            w.close();
        }
        w.close();
    }

    fn track(&mut self, w: &mut Writer, seq: &Sequence, t: &Track, depth: usize) {
        let rate = seq.rate();
        w.open("track", &[]);
        for c in &t.clips {
            if let Some(tr) = t.transitions.iter().find(|tr| tr.to == Some(c.id)) {
                self.transition(w, seq, t, tr);
            }
            self.clipitem(w, seq, t, c, depth);
            if let Some(tr) = t
                .transitions
                .iter()
                .find(|tr| tr.from == Some(c.id) && tr.to.is_none())
            {
                self.transition(w, seq, t, tr);
            }
        }
        w.leaf(
            "enabled",
            if t.enabled && !t.muted {
                "TRUE"
            } else {
                "FALSE"
            },
        );
        w.leaf("locked", if t.locked { "TRUE" } else { "FALSE" });
        w.close();
        let _ = rate;
    }

    fn transition(&mut self, w: &mut Writer, seq: &Sequence, t: &Track, tr: &Transition) {
        let rate = seq.rate();
        let r = tr.range();
        w.open("transitionitem", &[]);
        w.leaf("start", &fr(r.start.since_zero(), rate).to_string());
        w.leaf("end", &fr(r.end.since_zero(), rate).to_string());
        let alignment = match (tr.from, tr.to, tr.alignment) {
            (None, Some(_), _) => "start-black",
            (Some(_), None, _) => "end-black",
            (_, _, Alignment::StartAtCut) => "start",
            (_, _, Alignment::EndAtCut) => "end",
            _ => "center",
        };
        w.leaf("alignment", alignment);
        rate_block(w, rate);
        let name = catalog::find(&tr.effect)
            .map(|d| d.name)
            .unwrap_or("Cross Dissolve");
        w.open("effect", &[]);
        w.leaf("name", name);
        w.leaf("effectid", name);
        w.leaf(
            "effectcategory",
            catalog::find(&tr.effect)
                .map(|d| d.category)
                .unwrap_or("Dissolve"),
        );
        w.leaf("effecttype", "transition");
        w.leaf(
            "mediatype",
            if t.kind == TrackKind::Video {
                "video"
            } else {
                "audio"
            },
        );
        w.close();
        w.close();
    }

    fn clipitem(&mut self, w: &mut Writer, seq: &Sequence, t: &Track, c: &Clip, depth: usize) {
        let rate = seq.rate();
        let id = self.clip_ids.get(&c.id).cloned().unwrap_or_default();
        w.open("clipitem", &[("id", &id)]);
        w.leaf("name", &c.name);
        w.leaf("enabled", if c.enabled { "TRUE" } else { "FALSE" });
        let avail = self
            .p
            .available(&c.source)
            .map(|r| r.duration())
            .unwrap_or(c.source_duration());
        w.leaf("duration", &fr(avail, rate).to_string());
        rate_block(w, rate);
        w.leaf("start", &fr(c.start.since_zero(), rate).to_string());
        w.leaf("end", &fr(c.end().since_zero(), rate).to_string());
        let src = c.source_range();
        w.leaf("in", &fr(src.start.since_zero(), rate).to_string());
        w.leaf("out", &fr(src.end.since_zero(), rate).to_string());
        match &c.source {
            ClipSource::Asset { asset, .. } => {
                if let Some(a) = self.p.asset(*asset) {
                    self.file(w, a, rate);
                }
            }
            ClipSource::Sequence { sequence, .. } if depth < 8 => {
                self.sequence(w, *sequence, depth + 1)
            }
            _ => {}
        }
        if c.kind == TrackKind::Audio {
            w.open("sourcetrack", &[]);
            w.leaf("mediatype", "audio");
            w.leaf("trackindex", "1");
            w.close();
        }
        if !c.speed.is_normal() || c.reverse {
            w.open("filter", &[]);
            w.open("effect", &[]);
            w.leaf("name", "Time Remap");
            w.leaf("effectid", "timeremap");
            w.leaf("effectcategory", "motion");
            w.leaf("effecttype", "motion");
            w.leaf("mediatype", "video");
            for (pid, name, value) in [
                ("variablespeed", "variablespeed", "0".to_string()),
                ("speed", "speed", format!("{:.4}", c.speed.percent())),
                (
                    "reverse",
                    "reverse",
                    if c.reverse { "TRUE" } else { "FALSE" }.to_string(),
                ),
            ] {
                w.open("parameter", &[]);
                w.leaf("parameterid", pid);
                w.leaf("name", name);
                w.leaf("value", &value);
                w.close();
            }
            w.close();
            w.close();
        }
        if c.kind == TrackKind::Video
            && let Some(op) = c.component(catalog::OPACITY)
            && !op
                .param("opacity")
                .is_some_and(|p| !p.animated && p.value.as_f64() == 100.0)
        {
            w.open("filter", &[]);
            w.open("effect", &[]);
            w.leaf("name", "Opacity");
            w.leaf("effectid", "opacity");
            w.leaf("effectcategory", "motion");
            w.leaf("effecttype", "motion");
            w.leaf("mediatype", "video");
            w.open("parameter", &[]);
            w.leaf("parameterid", "opacity");
            w.leaf("name", "opacity");
            w.leaf("valuemin", "0");
            w.leaf("valuemax", "100");
            w.leaf(
                "value",
                &format!("{:.2}", op.f64_at("opacity", c.source_in)),
            );
            w.close();
            w.close();
            w.close();
        }
        if let Some(link) = c.link {
            for (r, other) in seq.clips().filter(|(_, o)| o.link == Some(link)) {
                let Some(oid) = self.clip_ids.get(&other.id).cloned() else {
                    continue;
                };
                let index = seq
                    .track(r)
                    .and_then(|tr| tr.index_of(other.id))
                    .unwrap_or(0)
                    + 1;
                w.open("link", &[]);
                w.leaf("linkclipref", &oid);
                w.leaf(
                    "mediatype",
                    if other.is_video() { "video" } else { "audio" },
                );
                w.leaf("trackindex", &(r.index + 1).to_string());
                w.leaf("clipindex", &index.to_string());
                w.close();
            }
        }
        w.close();
        let _ = t;
    }

    fn file(&mut self, w: &mut Writer, a: &MediaAsset, seq_rate: Rate) {
        if let Some(id) = self.files.get(&a.id) {
            w.empty("file", &[("id", id)]);
            return;
        }
        let id = self.id("file");
        self.files.insert(a.id, id.clone());
        let rate = a.frame_rate().unwrap_or(seq_rate);
        w.open("file", &[("id", &id)]);
        w.leaf("name", a.file_name());
        w.leaf("pathurl", &pathurl(&a.path));
        rate_block(w, rate);
        w.leaf(
            "duration",
            &fr(
                a.available().map(|r| r.duration()).unwrap_or(a.duration),
                rate,
            )
            .to_string(),
        );
        w.open("media", &[]);
        if let Some(v) = &a.video {
            w.open("video", &[]);
            w.open("samplecharacteristics", &[]);
            rate_block(w, rate);
            w.leaf("width", &v.width.to_string());
            w.leaf("height", &v.height.to_string());
            w.close();
            w.close();
        }
        if let Some(s) = a.audio.first() {
            w.open("audio", &[]);
            w.open("samplecharacteristics", &[]);
            w.leaf("depth", "16");
            w.leaf("samplerate", &s.sample_rate.to_string());
            w.close();
            w.leaf("channelcount", &s.layout.channels().to_string());
            w.close();
        }
        w.close();
        w.close();
    }
}

/// Exports a sequence (and the sequences nested in it).
pub fn export(p: &Project, sid: SequenceId) -> Result<String, ProjectError> {
    if p.sequence(sid).is_none() {
        return Err(ProjectError::Format("no such sequence".into()));
    }
    let mut w = Writer::new();
    w.raw("<!DOCTYPE xmeml>\n");
    w.open("xmeml", &[("version", "4")]);
    let mut ex = Exporter {
        p,
        files: HashMap::new(),
        sequences: HashMap::new(),
        clip_ids: HashMap::new(),
        next: 0,
    };
    ex.sequence(&mut w, sid, 0);
    Ok(w.finish())
}

// ------------------------------------------------------------------------------------ import

fn rate_of(doc: &Doc, n: usize) -> Option<Rate> {
    let r = doc.child(n, "rate")?;
    let base: u32 = doc.text_at(r, "timebase")?.parse().ok()?;
    let ntsc = doc
        .text_at(r, "ntsc")
        .is_some_and(|t| t.eq_ignore_ascii_case("true"));
    Some(if ntsc {
        Rate::new(base * 1000, 1001)
    } else {
        Rate::fps(base)
    })
}

fn num(doc: &Doc, n: usize, path: &str) -> Option<i64> {
    doc.text_at(n, path)
        .and_then(|t| t.trim().parse::<f64>().ok())
        .map(|v| v.round() as i64)
}

struct Importer<'a> {
    doc: &'a Doc,
    p: Project,
    files: HashMap<String, (AssetId, ItemId)>,
    sequences: HashMap<String, SequenceId>,
    /// clipitem id -> our clip, for links
    clip_ids: HashMap<String, ClipId>,
    links: Vec<(ClipId, Vec<String>)>,
}

impl<'a> Importer<'a> {
    fn file(&mut self, n: usize, seq_rate: Rate) -> Option<(AssetId, ItemId)> {
        let doc = self.doc;
        let id = doc.attr(n, "id").unwrap_or("").to_string();
        if let Some(x) = self.files.get(&id) {
            return Some(*x);
        }
        let url = doc.text_at(n, "pathurl")?;
        let path = url_to_path(url);
        let rate = rate_of(doc, n).unwrap_or(seq_rate);
        let mut a = placeholder_asset(&path, rate);
        let dur = num(doc, n, "duration").unwrap_or(0).max(0);
        if let Some(v) = doc.path(n, "media/video") {
            let sc = doc.child(v, "samplecharacteristics");
            a.video = Some(VideoStream {
                index: 0,
                codec: String::new(),
                width: sc.and_then(|s| num(doc, s, "width")).unwrap_or(1920) as u32,
                height: sc.and_then(|s| num(doc, s, "height")).unwrap_or(1080) as u32,
                pixel_aspect: (1, 1),
                rate,
                frames: dur.max(1),
                pixel_format: String::new(),
                bit_depth: 8,
                alpha: AlphaMode::None,
                color: ColorInfo::default(),
                field_order: FieldOrder::Progressive,
                start: Dur::ZERO,
                rotation: 0,
                timecode: None,
            });
        }
        if let Some(au) = doc.path(n, "media/audio") {
            let sr = doc
                .path(au, "samplecharacteristics")
                .and_then(|s| num(doc, s, "samplerate"))
                .unwrap_or(48000) as u32;
            let ch = num(doc, au, "channelcount").unwrap_or(2).clamp(1, 64) as u16;
            a.audio.push(AudioStream {
                index: 0,
                codec: String::new(),
                sample_rate: sr,
                layout: ChannelLayout::from_count(ch),
                samples: Rate::fps(sr).dur_to_frames_round(rate.frames_to_dur(dur)),
                start: Dur::ZERO,
            });
        }
        a.kind = if a.video.is_some() {
            MediaKind::Video
        } else {
            MediaKind::Audio
        };
        a.duration = rate.frames_to_dur(dur);
        let root = self.p.root;
        let (asset, item) = self.p.add_asset(root, a);
        if !id.is_empty() {
            self.files.insert(id, (asset, item));
        }
        Some((asset, item))
    }

    fn sequence(&mut self, n: usize, depth: usize) -> Option<SequenceId> {
        let doc = self.doc;
        let id = doc.attr(n, "id").unwrap_or("").to_string();
        if let Some(s) = self.sequences.get(&id) {
            return Some(*s);
        }
        // a reference without content resolves when the definition appears
        doc.child(n, "media")?;
        if depth > 8 {
            return None;
        }
        let rate = rate_of(doc, n).unwrap_or(Rate::FPS_25);
        let mut settings = SequenceSettings {
            rate,
            video_tracks: 1,
            audio_tracks: 1,
            ..SequenceSettings::default()
        };
        if let Some(sc) = doc.path(n, "media/video/format/samplecharacteristics") {
            settings.width = num(doc, sc, "width").unwrap_or(1920).clamp(16, 16384) as u32;
            settings.height = num(doc, sc, "height").unwrap_or(1080).clamp(16, 16384) as u32;
        }
        if let Some(sr) = doc
            .path(n, "media/audio/format/samplecharacteristics")
            .and_then(|s| num(doc, s, "samplerate"))
        {
            settings.audio_rate = sr.clamp(8000, 192000) as u32;
        }
        let name = doc.text_at(n, "name").unwrap_or("Sequence").to_string();
        let root = self.p.root;
        let (sid, _) = self.p.add_sequence(root, name, settings);
        if !id.is_empty() {
            self.sequences.insert(id, sid);
        }
        let mut video = Vec::new();
        let mut audio = Vec::new();
        if let Some(v) = doc.path(n, "media/video") {
            for t in doc.children_named(v, "track").collect::<Vec<_>>() {
                video.push(Arc::new(self.track(t, TrackKind::Video, rate, sid, depth)));
            }
        }
        if let Some(a) = doc.path(n, "media/audio") {
            for t in doc.children_named(a, "track").collect::<Vec<_>>() {
                audio.push(Arc::new(self.track(t, TrackKind::Audio, rate, sid, depth)));
            }
        }
        let mut markers = Vec::new();
        for m in doc.children_named(n, "marker").collect::<Vec<_>>() {
            let Some(inn) = num(doc, m, "in").filter(|v| *v >= 0) else {
                continue;
            };
            let out = num(doc, m, "out").unwrap_or(-1);
            let mut mk = Marker::new(self.p.ids.marker(), rate.frame_start(inn));
            if out > inn {
                mk.duration = rate.frames_to_dur(out - inn);
            }
            mk.name = doc.text_at(m, "name").unwrap_or("").into();
            mk.comment = doc.text_at(m, "comment").unwrap_or("").into();
            markers.push(mk);
        }
        let s = self.p.sequence_mut(sid).unwrap();
        if !video.is_empty() {
            s.video = video;
        }
        if !audio.is_empty() {
            s.audio = audio;
        }
        s.markers = markers;
        Some(sid)
    }

    fn track(
        &mut self,
        t: usize,
        kind: TrackKind,
        rate: Rate,
        sid: SequenceId,
        depth: usize,
    ) -> Track {
        let doc = self.doc;
        let mut track = Track::new(self.p.ids.track(), kind);
        if kind == TrackKind::Audio {
            track.components = default_components(EffectKind::AudioFixed, &mut self.p.ids);
        }
        track.locked = doc
            .text_at(t, "locked")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"));
        let enabled = doc
            .text_at(t, "enabled")
            .is_none_or(|v| !v.eq_ignore_ascii_case("false"));
        match kind {
            TrackKind::Video => track.enabled = enabled,
            TrackKind::Audio => track.muted = !enabled,
        }
        let items: Vec<usize> = doc
            .children(t)
            .filter(|c| matches!(doc.name(*c), "clipitem" | "transitionitem"))
            .collect();
        // transition cut points resolve clip edges written as -1
        let mut cuts: Vec<(i64, i64, i64, String)> = Vec::new();
        for &i in &items {
            if doc.name(i) != "transitionitem" {
                continue;
            }
            let (Some(s), Some(e)) = (num(doc, i, "start"), num(doc, i, "end")) else {
                continue;
            };
            let cut = match doc.text_at(i, "alignment").unwrap_or("center") {
                "start" | "start-black" => s,
                "end" | "end-black" => e,
                _ => (s + e) / 2,
            };
            let name = doc.text_at(i, "effect/name").unwrap_or("").to_string();
            cuts.push((s, e, cut, name));
        }
        let mut seen: Vec<(String, i64, i64, i64)> = Vec::new();
        for &i in &items {
            if doc.name(i) != "clipitem" {
                continue;
            }
            let mut start = num(doc, i, "start").unwrap_or(-1);
            let mut end = num(doc, i, "end").unwrap_or(-1);
            if start < 0 {
                start = cuts
                    .iter()
                    .filter(|c| c.1 >= 0)
                    .map(|c| c.2)
                    .filter(|c| *c <= end || end < 0)
                    .max()
                    .unwrap_or(0);
            }
            if end < 0 {
                end = cuts
                    .iter()
                    .map(|c| c.2)
                    .filter(|c| *c > start)
                    .min()
                    .unwrap_or(start);
            }
            if end <= start {
                continue;
            }
            let inn = num(doc, i, "in").unwrap_or(0).max(0);
            let out = num(doc, i, "out").unwrap_or(inn + end - start);
            let clip_rate = rate_of(doc, i).unwrap_or(rate);
            let source = if let Some(f) = doc.child(i, "file") {
                let key = doc.attr(f, "id").unwrap_or("").to_string();
                let dup = (key.clone(), start, end, inn);
                if kind == TrackKind::Audio && !key.is_empty() && seen.contains(&dup) {
                    continue;
                }
                seen.push(dup);
                // a file reference may precede its definition only in malformed files
                let Some((asset, item)) = self.file(f, clip_rate) else {
                    continue;
                };
                ensure_kind(&mut self.p, asset, kind);
                ClipSource::Asset {
                    asset,
                    item: Some(item),
                    stream: 0,
                }
            } else if let Some(s) = doc.child(i, "sequence") {
                let Some(nested) = self.sequence(s, depth + 1) else {
                    continue;
                };
                if nested == sid || self.p.nests(nested, sid) {
                    continue;
                }
                ClipSource::Sequence {
                    sequence: nested,
                    item: self.p.sequence_item(nested),
                }
            } else {
                continue;
            };
            let mut speed = Speed::NORMAL;
            let mut reverse = false;
            let mut opacity = None;
            for f in doc.children_named(i, "filter").collect::<Vec<_>>() {
                let Some(e) = doc.child(f, "effect") else {
                    continue;
                };
                let eid = doc.text_at(e, "effectid").unwrap_or("");
                for prm in doc.children_named(e, "parameter").collect::<Vec<_>>() {
                    let pid = doc.text_at(prm, "parameterid").unwrap_or("");
                    let value = doc.text_at(prm, "value").unwrap_or("");
                    match (eid, pid) {
                        ("timeremap", "speed") => {
                            if let Ok(v) = value.parse::<f64>()
                                && v.abs() > 0.001
                            {
                                speed = Speed::from_percent(v.abs());
                                reverse |= v < 0.0;
                            }
                        }
                        ("timeremap", "reverse") => reverse |= value.eq_ignore_ascii_case("true"),
                        ("opacity", "opacity") => opacity = value.parse::<f64>().ok(),
                        _ => {}
                    }
                }
            }
            let fixed = if kind == TrackKind::Video {
                EffectKind::VideoFixed
            } else {
                EffectKind::AudioFixed
            };
            let id = self.p.ids.clip();
            let mut components = default_components(fixed, &mut self.p.ids);
            if let (Some(o), Some(c)) = (
                opacity,
                components.iter_mut().find(|c| c.effect == catalog::OPACITY),
            ) && let Some(p) = c.param_mut("opacity")
            {
                p.value = Value::Float(o.clamp(0.0, 100.0));
            }
            let duration = rate.frames_to_dur(end - start);
            let source_len = clip_rate.frames_to_dur((out - inn).max(1));
            if speed.is_normal() && source_len != duration && out > inn {
                speed = Speed::from_durations(source_len, duration);
            }
            let clip = Clip {
                id,
                name: doc.text_at(i, "name").unwrap_or("").to_string(),
                kind,
                source,
                start: rate.frame_start(start),
                duration,
                source_in: clip_rate.frame_start(inn),
                speed,
                reverse,
                hold: None,
                enabled: doc
                    .text_at(i, "enabled")
                    .is_none_or(|v| !v.eq_ignore_ascii_case("false")),
                link: None,
                group: None,
                label: Label::None,
                components,
                gain_db: 0.0,
                scale_to_frame: false,
                channels: (kind == TrackKind::Audio).then_some(ChannelLayout::Stereo),
            };
            if let Some(cid) = doc.attr(i, "id") {
                self.clip_ids.insert(cid.to_string(), id);
            }
            let links: Vec<String> = doc
                .children_named(i, "link")
                .filter_map(|l| doc.text_at(l, "linkclipref"))
                .map(str::to_string)
                .collect();
            if !links.is_empty() {
                self.links.push((id, links));
            }
            if track
                .clips
                .iter()
                .any(|c| c.range().overlaps(&clip.range()))
            {
                continue;
            }
            track.clips.push(clip);
        }
        track.sort();
        // transitions between adjacent clips or at one clip edge
        for (s, e, cut, name) in cuts {
            let cut_t: SeqTime = rate.frame_start(cut);
            let from = track.clips.iter().find(|c| c.end() == cut_t).map(|c| c.id);
            let to = track.clips.iter().find(|c| c.start == cut_t).map(|c| c.id);
            if from.is_none() && to.is_none() {
                continue;
            }
            let effect = catalog::CATALOG
                .iter()
                .find(|d| {
                    d.kind.is_transition()
                        && d.kind.is_video() == (kind == TrackKind::Video)
                        && d.name.eq_ignore_ascii_case(&name)
                })
                .map(|d| d.id)
                .unwrap_or(if kind == TrackKind::Video {
                    catalog::CROSS_DISSOLVE
                } else {
                    catalog::CONSTANT_POWER
                });
            track.transitions.push(Transition {
                id: self.p.ids.transition(),
                effect: effect.into(),
                cut: cut_t,
                duration: rate.frames_to_dur(e - s),
                alignment: Alignment::Custom(rate.frames_to_dur(cut - s)),
                from,
                to,
                params: vec![],
            });
        }
        track
    }
}

fn ensure_kind(p: &mut Project, asset: AssetId, kind: TrackKind) {
    let a = Arc::make_mut(p.assets.get_mut(&asset).unwrap());
    let rate = a.interpretation.frame_rate.unwrap_or(Rate::FPS_25);
    match kind {
        TrackKind::Video if a.video.is_none() => {
            a.video = Some(VideoStream {
                index: 0,
                codec: String::new(),
                width: 1920,
                height: 1080,
                pixel_aspect: (1, 1),
                rate,
                frames: rate.dur_to_frames_ceil(a.duration).max(1),
                pixel_format: String::new(),
                bit_depth: 8,
                alpha: AlphaMode::None,
                color: ColorInfo::default(),
                field_order: FieldOrder::Progressive,
                start: Dur::ZERO,
                rotation: 0,
                timecode: None,
            });
            a.kind = MediaKind::Video;
        }
        TrackKind::Audio if a.audio.is_empty() => {
            a.audio.push(AudioStream {
                index: 0,
                codec: String::new(),
                sample_rate: 48000,
                layout: ChannelLayout::Stereo,
                samples: Rate::HZ_48000.dur_to_frames_ceil(a.duration),
                start: Dur::ZERO,
            });
        }
        _ => {}
    }
}

/// Imports every top-level sequence of an `xmeml` document. Returns the project and the first
/// sequence.
pub fn import(text: &str, name: &str) -> Result<(Project, SequenceId), ProjectError> {
    let doc =
        xml::parse(text, Limits::default()).map_err(|e| ProjectError::Format(e.to_string()))?;
    if doc.name(0) != "xmeml" {
        return Err(ProjectError::Format("not a Final Cut Pro XML file".into()));
    }
    let mut im = Importer {
        doc: &doc,
        p: Project::new(name),
        files: HashMap::new(),
        sequences: HashMap::new(),
        clip_ids: HashMap::new(),
        links: Vec::new(),
    };
    // pre-register file definitions anywhere in the document so references can resolve
    let file_defs: Vec<usize> = (0..doc.nodes.len())
        .filter(|i| doc.name(*i) == "file" && doc.child(*i, "pathurl").is_some())
        .collect();
    for f in file_defs {
        im.file(f, Rate::FPS_25);
    }
    let mut stack: Vec<usize> = vec![0];
    let mut top = Vec::new();
    while let Some(n) = stack.pop() {
        for c in doc.children(n) {
            match doc.name(c) {
                "sequence" => top.push(c),
                "project" | "children" | "bin" => stack.push(c),
                _ => {}
            }
        }
    }
    let mut first = None;
    for s in top {
        if let Some(sid) = im.sequence(s, 0) {
            first.get_or_insert(sid);
        }
    }
    let first = first.ok_or_else(|| ProjectError::Format("the file has no sequence".into()))?;
    // links: every clipitem that lists another as linked shares a link
    let mut p = im.p;
    for (clip, refs) in im.links {
        let others: Vec<ClipId> = refs
            .iter()
            .filter_map(|r| im.clip_ids.get(r).copied())
            .filter(|c| *c != clip)
            .collect();
        if others.is_empty() {
            continue;
        }
        let sid = p
            .sequences
            .iter()
            .find(|(_, s)| s.clip(clip).is_some())
            .map(|(id, _)| *id);
        let Some(sid) = sid else { continue };
        let existing = p.sequence(sid).unwrap().clip(clip).and_then(|c| c.link);
        let link = existing.unwrap_or_else(|| p.ids.link());
        let s = p.sequence_mut(sid).unwrap();
        for id in std::iter::once(clip).chain(others) {
            if let Some(c) = s.clip_mut(id)
                && c.link.is_none()
            {
                c.link = Some(link);
            }
        }
    }
    fit_assets(&mut p);
    p.normalize();
    p.validate().map_err(ProjectError::Invalid)?;
    Ok((p, first))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_import_round_trip() {
        let (p, sid) = crate::otio::tests_support::sample_project();
        let text = export(&p, sid).unwrap();
        assert!(text.contains("<xmeml version=\"4\">"));
        let (q, qsid) = import(&text, "back").unwrap();
        let a = p.sequence(sid).unwrap();
        let b = q.sequence(qsid).unwrap();
        let layout = |s: &Sequence, r: TrackRef| -> Vec<(i64, i64, i64)> {
            s.track(r)
                .unwrap()
                .clips
                .iter()
                .map(|c| (c.start.ticks(), c.duration.ticks(), c.source_in.ticks()))
                .collect()
        };
        assert_eq!(layout(a, TrackRef::video(0)), layout(b, TrackRef::video(0)));
        assert_eq!(layout(a, TrackRef::audio(0)), layout(b, TrackRef::audio(0)));
        assert_eq!(b.video[0].transitions.len(), 1);
        assert_eq!(
            b.video[0].transitions[0].range(),
            a.video[0].transitions[0].range()
        );
        assert_eq!(b.markers.len(), 1);
        assert!(b.video[0].clips[0].link.is_some());
        assert_eq!(b.video[0].clips[0].link, b.audio[0].clips[0].link);
        assert_eq!(q.assets.len(), 1);
    }
}
