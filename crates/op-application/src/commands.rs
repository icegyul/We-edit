//! Command dispatch. Command identifiers are the ones used by the default shortcut map
//! (docs/evidence/generated/premiere_shortcuts_default.md), so menus, shortcuts and imported
//! `.kys` maps all address the same commands.

use op_core::catalog::{self, EffectKind};
use op_core::*;
use op_timeline::*;

use crate::editor::Editor;
use crate::session::*;

/// Commands the editor handles itself (the UI adds dialogs and window commands).
pub const EDITOR_COMMANDS: &[&str] = &[
    "cmd.edit.undo",
    "cmd.edit.redo",
    "cmd.edit.cut",
    "cmd.edit.copy",
    "cmd.edit.paste",
    "cmd.edit.pasteinsert",
    "cmd.edit.clear",
    "cmd.edit.rippledelete",
    "cmd.edit.selectall",
    "cmd.edit.deselectall",
    "cmd.edit.duplicate",
    "cmd.clip.enable",
    "cmd.clip.linkaudioandvideo",
    "cmd.clip.group",
    "cmd.clip.ungroup",
    "cmd.clip.insert",
    "cmd.clip.overlay",
    "cmd.clip.makesubclip",
    "cmd.common.setin",
    "cmd.common.setout",
    "cmd.clear.in",
    "cmd.clear.out",
    "cmd.clear.inandout",
    "cmd.goto.in",
    "cmd.goto.out",
    "cmd.marker.setsequenceinoutmarkeraroundtargetclip",
    "cmd.marker.setsequenceinoutmarkeraroundselection.out",
    "cmd.sequence.lift",
    "cmd.sequence.extract",
    "cmd.sequence.razorateditline",
    "cmd.sequence.razorateditline.all",
    "cmd.sequence.applydefaultvideotransition",
    "cmd.sequence.applydefaultaudiotransition",
    "cmd.sequence.applydefaulttransitions",
    "cmd.sequence.snap",
    "cmd.sequence.extendselectededittoplayhead",
    "cmd.sequence.rippletrimpreviousedittoplayhead",
    "cmd.sequence.rippletrimnextedittoplayhead",
    "cmd.sequence.matchframe",
    "cmd.sequence.reversematchframe",
    "cmd.sequence.increaseclipvolume",
    "cmd.sequence.decreaseclipvolume",
    "cmd.sequence.increaseclipvolumemany",
    "cmd.sequence.decreaseclipvolumemany",
    "cmd.sequence.findnextsequencegap",
    "cmd.sequence.findprevioussequencegap",
    "cmd.sequence.makesubsequence",
    "cmd.set.marker",
    "cmd.marker.gotomarker.next",
    "cmd.marker.gotomarker.previous",
    "cmd.marker.clearmarker.current",
    "cmd.marker.clearmarker.all",
    "cmd.transport.toggleplay",
    "cmd.transport.shuttle.left",
    "cmd.transport.shuttle.right",
    "cmd.transport.shuttle.stop",
    "cmd.transport.shuttle.slow.left",
    "cmd.transport.shuttle.slow.right",
    "cmd.transport.step.back",
    "cmd.transport.step.forward",
    "cmd.transport.step.back.five",
    "cmd.transport.step.forward.five",
    "cmd.transport.sequence.start",
    "cmd.transport.sequence.end",
    "cmd.transport.selectedclip.start",
    "cmd.transport.selectedclip.end",
    "cmd.transport.playintoout",
    "cmd.transport.play.ctitoout",
    "cmd.transport.playedit",
    "cmd.tlnav.next.edit",
    "cmd.tlnav.prev.edit",
    "cmd.tlnav.next.edit.any.track",
    "cmd.tlnav.prev.edit.any.track",
    "cmd.tlnav.select.clip.at.playhead",
    "cmd.tlnav.select.next.clip",
    "cmd.tlnav.select.previous.clip",
    "cmd.tlnav.toggle.all.target.video",
    "cmd.tlnav.toggle.all.target.audio",
    "cmd.tlnav.toggle.all.source.video",
    "cmd.tlnav.toggle.all.source.audio",
    "cmd.tlnav.trim.in.to.cti",
    "cmd.tlnav.trim.out.to.cti",
    "cmd.timeline.ripple.delete",
    "cmd.timeline.paste.to.same.track",
    "cmd.timeline.pasteinsert.to.same.track",
    "cmd.timeline.nudge.left.one",
    "cmd.timeline.nudge.right.one",
    "cmd.timeline.nudge.left.several",
    "cmd.timeline.nudge.right.several",
    "cmd.timeline.nudge.up",
    "cmd.timeline.nudge.down",
    "cmd.timeline.slip.left.one",
    "cmd.timeline.slip.right.one",
    "cmd.timeline.slip.left.several",
    "cmd.timeline.slip.right.several",
    "cmd.timeline.slide.left.one",
    "cmd.timeline.slide.right.one",
    "cmd.timeline.slide.left.several",
    "cmd.timeline.slide.right.several",
    "cmd.graphics.add.text",
    "cmd.graphics.add.shape.rectangle",
    "cmd.graphics.add.shape.ellipse",
    "cmd.file.new.bin",
];

const SEVERAL: i64 = 5;

impl Editor {
    /// Runs a command. Returns false when it is not an editor command (the UI handles it).
    pub fn execute(&mut self, cmd: &str, focus: Focus) -> bool {
        if let Some(tool) = Tool::from_command(cmd) {
            self.tool = tool;
            return true;
        }
        let monitor = focus.monitor();
        match cmd {
            "cmd.edit.undo" => self.undo(),
            "cmd.edit.redo" => self.redo(),
            "cmd.edit.copy" => self.copy(),
            "cmd.edit.cut" => {
                self.copy();
                self.delete_selection(false);
            }
            "cmd.edit.paste" => self.paste(false),
            "cmd.timeline.paste.to.same.track" => self.paste(false),
            "cmd.edit.pasteinsert" | "cmd.timeline.pasteinsert.to.same.track" => self.paste(true),
            "cmd.edit.clear" => {
                if focus == Focus::Project {
                    self.delete_items();
                } else {
                    self.delete_selection(false);
                }
            }
            "cmd.edit.rippledelete" | "cmd.timeline.ripple.delete" => self.delete_selection(true),
            "cmd.edit.selectall" => {
                if focus == Focus::Project {
                    self.items = self.project.children(self.bin).to_vec();
                } else if let Some(seq) = self.active_seq() {
                    self.selection = Selection::only(seq.clips().map(|(_, c)| c.id).collect());
                }
            }
            "cmd.edit.deselectall" => {
                self.selection.clear();
                if focus == Focus::Project {
                    self.items.clear();
                }
            }
            "cmd.edit.duplicate" => self.duplicate_items(),
            "cmd.clip.enable" => self.with_selection("Enable", |p, sid, clips, opts| {
                toggle_enabled(p, sid, clips, opts).map(|_| ())
            }),
            "cmd.clip.linkaudioandvideo" => {
                let linked = self.active_seq().is_some_and(|s| {
                    self.selection
                        .clips
                        .iter()
                        .any(|c| s.clip(*c).is_some_and(|c| c.link.is_some()))
                });
                if linked {
                    self.with_selection("Unlink", |p, sid, clips, _| unlink(p, sid, clips));
                } else {
                    self.with_selection("Link", |p, sid, clips, _| link(p, sid, clips));
                }
            }
            "cmd.clip.group" => {
                self.with_selection("Group", |p, sid, clips, _| group(p, sid, clips))
            }
            "cmd.clip.ungroup" => {
                self.with_selection("Ungroup", |p, sid, clips, _| ungroup(p, sid, clips))
            }
            "cmd.clip.insert" => self.source_edit(true),
            "cmd.clip.overlay" => self.source_edit(false),
            "cmd.clip.makesubclip" => self.make_subclip(),
            "cmd.common.setin" => self.set_mark(monitor, true, Some(self.monitor_time(monitor))),
            "cmd.common.setout" => {
                // Out marks the end of the current frame
                let t = self.monitor_time(monitor) + self.monitor_rate(monitor).frame_duration();
                self.set_mark(monitor, false, Some(t));
            }
            "cmd.clear.in" => self.set_mark(monitor, true, None),
            "cmd.clear.out" => self.set_mark(monitor, false, None),
            "cmd.clear.inandout" => {
                self.set_mark(monitor, true, None);
                self.set_mark(monitor, false, None);
            }
            "cmd.goto.in" => {
                if let Some(t) = self.mark(monitor, true) {
                    self.stop();
                    self.set_monitor_time(monitor, t);
                }
            }
            "cmd.goto.out" => {
                if let Some(t) = self.mark(monitor, false) {
                    self.stop();
                    let f = self.monitor_rate(monitor).frame_duration();
                    self.set_monitor_time(monitor, t - f);
                }
            }
            "cmd.marker.setsequenceinoutmarkeraroundtargetclip" => self.mark_clip(),
            "cmd.marker.setsequenceinoutmarkeraroundselection.out" => self.mark_selection(),
            "cmd.sequence.lift" => self.lift_extract(false),
            "cmd.sequence.extract" => self.lift_extract(true),
            "cmd.sequence.razorateditline" => self.add_edit(false),
            "cmd.sequence.razorateditline.all" => self.add_edit(true),
            "cmd.sequence.applydefaultvideotransition" => {
                self.default_transition(Some(TrackKind::Video))
            }
            "cmd.sequence.applydefaultaudiotransition" => {
                self.default_transition(Some(TrackKind::Audio))
            }
            "cmd.sequence.applydefaulttransitions" => self.default_transition(None),
            "cmd.sequence.snap" => {
                self.prefs.snapping = !self.prefs.snapping;
                let on = self.prefs.snapping;
                self.info(if on { "Snapping on" } else { "Snapping off" });
            }
            "cmd.sequence.extendselectededittoplayhead" => {
                let t = self.playhead();
                let clips = self.selection.clips.clone();
                self.seq_edit("Extend Edit", |p, sid, opts| {
                    extend_to(p, sid, &clips, t, opts)
                });
            }
            "cmd.sequence.rippletrimpreviousedittoplayhead"
            | "cmd.sequence.rippletrimnextedittoplayhead" => {
                let previous = cmd.contains("previous");
                let t = self.playhead();
                let targets = self.targets();
                let ok = self.seq_edit("Ripple Trim", |p, sid, opts| {
                    ripple_trim_to_playhead(p, sid, &targets, t, previous, opts)
                });
                if ok.is_some() && previous {
                    // the playhead lands on the edit that was pulled to it
                    if let Some(e) = self
                        .active_seq()
                        .and_then(|s| prev_edit(s, &targets, t + Dur(1)))
                    {
                        self.set_playhead(e);
                    }
                }
            }
            "cmd.tlnav.trim.in.to.cti" | "cmd.tlnav.trim.out.to.cti" => {
                let head = cmd.ends_with("in.to.cti");
                let t = self.playhead();
                let targets = self.targets();
                self.seq_edit("Trim", |p, sid, opts| {
                    trim_to_playhead(p, sid, &targets, t, head, opts)
                });
            }
            "cmd.sequence.matchframe" => self.match_frame(),
            "cmd.sequence.reversematchframe" => self.reverse_match_frame(),
            "cmd.sequence.increaseclipvolume" => self.nudge_volume(1.0),
            "cmd.sequence.decreaseclipvolume" => self.nudge_volume(-1.0),
            "cmd.sequence.increaseclipvolumemany" => self.nudge_volume(6.0),
            "cmd.sequence.decreaseclipvolumemany" => self.nudge_volume(-6.0),
            "cmd.sequence.findnextsequencegap" | "cmd.sequence.findprevioussequencegap" => {
                let t = self.playhead();
                let targets = self.targets();
                if let Some(seq) = self.active_seq() {
                    let found = if cmd.contains("next") {
                        next_gap(seq, &targets, t)
                    } else {
                        prev_gap(seq, &targets, t)
                    };
                    if let Some(g) = found {
                        self.set_playhead(g);
                    }
                }
            }
            "cmd.sequence.makesubsequence" => self.make_subsequence(),
            "cmd.set.marker" => self.add_marker(monitor),
            "cmd.marker.gotomarker.next" | "cmd.marker.gotomarker.previous" => {
                let next = cmd.ends_with("next");
                let t = self.playhead();
                if let Some(seq) = self.active_seq() {
                    let m = if next {
                        seq.markers.iter().map(|m| m.start).filter(|s| *s > t).min()
                    } else {
                        seq.markers.iter().map(|m| m.start).filter(|s| *s < t).max()
                    };
                    if let Some(m) = m {
                        self.set_playhead(m);
                    }
                }
            }
            "cmd.marker.clearmarker.current" => {
                let t = self.playhead();
                self.seq_edit("Clear Marker", |p, sid, _| {
                    let s = p.sequence_mut(sid).unwrap();
                    let n = s.markers.len();
                    s.markers.retain(|m| m.start != t);
                    if s.markers.len() == n {
                        Err(EditError::Nothing)
                    } else {
                        Ok(())
                    }
                });
            }
            "cmd.marker.clearmarker.all" => {
                self.seq_edit("Clear Markers", |p, sid, _| {
                    let s = p.sequence_mut(sid).unwrap();
                    if s.markers.is_empty() {
                        return Err(EditError::Nothing);
                    }
                    s.markers.clear();
                    Ok(())
                });
            }
            "cmd.transport.toggleplay" => self.toggle_play(monitor),
            "cmd.transport.shuttle.left" => self.shuttle(monitor, false),
            "cmd.transport.shuttle.right" => self.shuttle(monitor, true),
            "cmd.transport.shuttle.stop" => self.stop(),
            "cmd.transport.shuttle.slow.left" => self.play(monitor, -0.25),
            "cmd.transport.shuttle.slow.right" => self.play(monitor, 0.25),
            "cmd.transport.step.back" => self.step(monitor, -1),
            "cmd.transport.step.forward" => self.step(monitor, 1),
            "cmd.transport.step.back.five" => self.step(monitor, -SEVERAL),
            "cmd.transport.step.forward.five" => self.step(monitor, SEVERAL),
            "cmd.transport.sequence.start" => {
                self.stop();
                self.set_monitor_time(monitor, SeqTime::ZERO);
            }
            "cmd.transport.sequence.end" => {
                self.stop();
                let end = match monitor {
                    Monitor::Program => {
                        SeqTime::ZERO + self.active_seq().map(|s| s.duration()).unwrap_or(Dur::ZERO)
                    }
                    Monitor::Source => SeqTime::ZERO + self.source_duration(),
                };
                self.set_monitor_time(monitor, end);
            }
            "cmd.transport.selectedclip.start" | "cmd.transport.selectedclip.end" => {
                let start = cmd.ends_with("start");
                if let Some(c) = self
                    .selection
                    .clips
                    .first()
                    .and_then(|c| self.active_seq().and_then(|s| s.clip(*c)))
                {
                    let t = if start {
                        c.start
                    } else {
                        c.end() - self.active_seq().unwrap().settings.frame_duration()
                    };
                    self.stop();
                    self.set_playhead(t);
                }
            }
            "cmd.transport.playintoout" => {
                let (a, b) = (self.mark(monitor, true), self.mark(monitor, false));
                if let (Some(a), Some(b)) = (a, b) {
                    self.stop();
                    self.set_monitor_time(monitor, a);
                    self.play(monitor, 1.0);
                    self.transport.stop_at = Some(b);
                }
            }
            "cmd.transport.play.ctitoout" => {
                if let Some(b) = self.mark(monitor, false) {
                    self.play(monitor, 1.0);
                    self.transport.stop_at = Some(b);
                }
            }
            "cmd.transport.playedit" => {
                // play around the playhead: two seconds before to two seconds after
                let t = self.monitor_time(monitor);
                self.stop();
                self.set_monitor_time(monitor, (t - Dur::from_seconds(2.0)).max(SeqTime::ZERO));
                self.play(monitor, 1.0);
                self.transport.stop_at = Some(t + Dur::from_seconds(2.0));
            }
            "cmd.tlnav.next.edit"
            | "cmd.tlnav.prev.edit"
            | "cmd.tlnav.next.edit.any.track"
            | "cmd.tlnav.prev.edit.any.track" => {
                let any = cmd.contains("any.track");
                let next = cmd.contains("next");
                let t = self.playhead();
                let tracks = if any {
                    self.active_seq()
                        .map(|s| s.all_tracks().map(|(r, _)| r).collect())
                        .unwrap_or_default()
                } else {
                    self.targets()
                };
                if let Some(seq) = self.active_seq() {
                    let found = if next {
                        next_edit(seq, &tracks, t)
                    } else {
                        prev_edit(seq, &tracks, t)
                    };
                    if let Some(e) = found {
                        self.stop();
                        self.set_playhead(e);
                    }
                }
            }
            "cmd.tlnav.select.clip.at.playhead" => {
                let t = self.playhead();
                let targets = self.targets();
                if let Some(seq) = self.active_seq() {
                    let clips = clips_at(seq, &targets, t);
                    if let Some(first) = clips.first() {
                        let with = if self.prefs.linked_selection {
                            seq.linked(*first)
                        } else {
                            vec![*first]
                        };
                        self.selection = Selection::only(with);
                    }
                }
            }
            "cmd.tlnav.select.next.clip" | "cmd.tlnav.select.previous.clip" => {
                self.select_adjacent(cmd.ends_with("next.clip"))
            }
            "cmd.tlnav.toggle.all.target.video" | "cmd.tlnav.toggle.all.target.audio" => {
                let kind = if cmd.ends_with("video") {
                    TrackKind::Video
                } else {
                    TrackKind::Audio
                };
                if let Some(sid) = self.active {
                    let n = self
                        .project
                        .sequence(sid)
                        .map(|s| s.tracks(kind).len())
                        .unwrap_or(0);
                    let v = self.view(sid);
                    let list = if kind == TrackKind::Video {
                        &mut v.video_targets
                    } else {
                        &mut v.audio_targets
                    };
                    let all = list.iter().take(n).all(|t| *t);
                    list.iter_mut().for_each(|t| *t = !all);
                }
            }
            "cmd.tlnav.toggle.all.source.video" | "cmd.tlnav.toggle.all.source.audio" => {
                if let Some(sid) = self.active {
                    let v = self.view(sid);
                    if cmd.ends_with("video") {
                        v.video_patch = if v.video_patch.is_some() {
                            None
                        } else {
                            Some(0)
                        };
                    } else if v.audio_patch.iter().any(|p| p.is_some()) {
                        v.audio_patch.iter_mut().for_each(|p| *p = None);
                    } else {
                        v.audio_patch = vec![Some(0), Some(1)];
                    }
                }
            }
            "cmd.timeline.nudge.left.one" => self.nudge(-1, 0),
            "cmd.timeline.nudge.right.one" => self.nudge(1, 0),
            "cmd.timeline.nudge.left.several" => self.nudge(-SEVERAL, 0),
            "cmd.timeline.nudge.right.several" => self.nudge(SEVERAL, 0),
            "cmd.timeline.nudge.up" => self.nudge(0, 1),
            "cmd.timeline.nudge.down" => self.nudge(0, -1),
            "cmd.timeline.slip.left.one" => self.slip_selection(-1),
            "cmd.timeline.slip.right.one" => self.slip_selection(1),
            "cmd.timeline.slip.left.several" => self.slip_selection(-SEVERAL),
            "cmd.timeline.slip.right.several" => self.slip_selection(SEVERAL),
            "cmd.timeline.slide.left.one" => self.slide_selection(-1),
            "cmd.timeline.slide.right.one" => self.slide_selection(1),
            "cmd.timeline.slide.left.several" => self.slide_selection(-SEVERAL),
            "cmd.timeline.slide.right.several" => self.slide_selection(SEVERAL),
            "cmd.graphics.add.text" => {
                self.add_graphic(catalog::TEXT, None);
            }
            "cmd.graphics.add.shape.rectangle" => {
                self.add_graphic(catalog::SHAPE, Some(0));
            }
            "cmd.graphics.add.shape.ellipse" => {
                self.add_graphic(catalog::SHAPE, Some(1));
            }
            "op.captions.new" => {
                self.add_graphic(catalog::CAPTION, None);
            }
            "op.captions.apply_style" => {
                let n = self.apply_caption_style_to_all();
                if n > 0 {
                    self.info(format!("Style applied to {n} captions"));
                }
            }
            "cmd.file.new.bin" => {
                let bin = self.bin;
                if let Some(id) = self.edit("New Bin", |p| Ok(p.add_bin(bin, "Bin"))) {
                    self.items = vec![id];
                }
            }
            _ => return false,
        }
        true
    }

    /// Runs an edit on the active sequence.
    pub fn seq_edit<R>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Project, SequenceId, EditOptions) -> EditResult<R>,
    ) -> Option<R> {
        let sid = self.active?;
        let opts = self.opts();
        self.edit(label, |p| f(p, sid, opts))
    }

    fn with_selection(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Project, SequenceId, &[ClipId], EditOptions) -> EditResult,
    ) {
        if self.selection.clips.is_empty() {
            return;
        }
        let clips = self.selection.clips.clone();
        self.seq_edit(label, |p, sid, opts| f(p, sid, &clips, opts));
    }

    // ------------------------------------------------------------------------------ editing

    pub fn delete_selection(&mut self, ripple: bool) {
        if let Some((track, t)) = self.selection.gap {
            if ripple || self.selection.clips.is_empty() {
                self.seq_edit("Ripple Delete", |p, sid, opts| {
                    delete_gap(p, sid, track, t, opts).map(|_| ())
                });
                self.selection.gap = None;
            }
            return;
        }
        let clips = self.selection.clips.clone();
        let transitions = self.selection.transitions.clone();
        if clips.is_empty() && transitions.is_empty() {
            return;
        }
        if ripple && !clips.is_empty() {
            let full = self.seq_edit("Ripple Delete", |p, sid, opts| {
                ripple_delete(p, sid, &clips, opts)
            });
            if full == Some(false) {
                self.info(
                    "The gap could not close completely because of material on a sync-locked track",
                );
            }
        } else {
            self.seq_edit("Clear", |p, sid, opts| {
                delete(p, sid, &clips, &transitions, opts)
            });
        }
        self.selection.clear();
    }

    pub fn copy(&mut self) {
        if let Some(seq) = self.active_seq() {
            let ids = if self.prefs.linked_selection {
                let mut v = Vec::new();
                for c in &self.selection.clips {
                    for l in seq.linked(*c) {
                        if !v.contains(&l) {
                            v.push(l);
                        }
                    }
                }
                v
            } else {
                self.selection.clips.clone()
            };
            if !ids.is_empty() {
                self.clipboard = Clipboard::copy(seq, &ids);
            }
        }
    }

    pub fn paste(&mut self, insert: bool) {
        if self.clipboard.is_empty() {
            return;
        }
        let Some(sid) = self.active else { return };
        let t = self.playhead();
        let seq = self.project.sequence(sid).unwrap().clone();
        let v = self.view(sid).clone();
        let base_v = (0..seq.video.len())
            .find(|i| v.is_targeted(TrackRef::video(*i)))
            .unwrap_or(0);
        let base_a = (0..seq.audio.len())
            .find(|i| v.is_targeted(TrackRef::audio(*i)))
            .unwrap_or(0);
        let cb = self.clipboard.clone();
        let span = cb.span().map(|s| s.duration()).unwrap_or(Dur::ZERO);
        let label = if insert { "Paste Insert" } else { "Paste" };
        if let Some(ids) = self.seq_edit(label, |p, sid, opts| {
            paste(p, sid, &cb, t, base_v, base_a, insert, opts)
        }) {
            self.selection = Selection::only(ids);
            self.set_playhead(t + span);
        }
    }

    pub fn paste_attributes(&mut self, what: Attributes) {
        let Some(from) = self.clipboard.clips.first().map(|(_, c)| c.clone()) else {
            return;
        };
        let to = self.selection.clips.clone();
        self.seq_edit("Paste Attributes", |p, sid, opts| {
            paste_attributes(p, sid, &from, &to, what, opts)
        });
    }

    /// Insert (`,`) or overwrite (`.`) from the Source Monitor, with three-point rules: the
    /// sequence In (or the playhead) is the destination; sequence In and Out together set the
    /// duration.
    pub fn source_edit(&mut self, insert: bool) {
        let Some(item) = self.source.item.or_else(|| self.items.first().copied()) else {
            self.error("Open a clip in the Source Monitor first");
            return;
        };
        if self.active.is_none() {
            self.sequence_from_item(item);
            return;
        }
        let sid = self.active.unwrap();
        let Ok(mut spec) = SourceClip::from_item(&self.project, item) else {
            return;
        };
        let seq = self.project.sequence(sid).unwrap();
        let at = seq.mark_in.unwrap_or(self.playhead());
        if let (Some(i), Some(o)) = (seq.mark_in, seq.mark_out)
            && o > i
        {
            let len = o - i;
            let avail = self.project.available(
                spec.video
                    .as_ref()
                    .or(spec.audio.first().map(|a| &a.0))
                    .unwrap_or(&ClipSource::Graphic),
            );
            let mut end = spec.range.start + len;
            if let Some(a) = avail {
                end = end.min(a.end);
            }
            spec.range = SrcRange::new(spec.range.start, end);
        }
        let patch = self.patch(sid);
        let label = if insert { "Insert" } else { "Overwrite" };
        let len = spec.duration();
        let r = self.seq_edit(label, |p, sid, opts| {
            if insert {
                insert_edit(p, sid, &spec, at, &patch, opts)
            } else {
                overwrite(p, sid, &spec, at, &patch, opts)
            }
        });
        if r.is_some() {
            self.set_playhead(at + len);
        }
    }

    fn make_subclip(&mut self) {
        let Some(item) = self.source.item else { return };
        let Some(it) = self.project.item(item).cloned() else {
            return;
        };
        let ItemKind::Media { asset, subclip } = it.kind else {
            return;
        };
        let full = subclip.or(self.project.asset(asset).and_then(|a| a.available()));
        let Some(full) = full else { return };
        let start = it.mark_in.unwrap_or(full.start);
        let end = it.mark_out.unwrap_or(full.end);
        if end <= start {
            return;
        }
        let bin = it.parent.unwrap_or(self.project.root);
        let name = format!("{}.Subclip", it.name);
        if let Some(id) = self.edit("Make Subclip", |p| {
            Ok(p.add_item(
                bin,
                name,
                ItemKind::Media {
                    asset,
                    subclip: Some(SrcRange::new(start, end)),
                },
            ))
        }) {
            self.items = vec![id];
        }
    }

    pub fn mark(&self, monitor: Monitor, is_in: bool) -> Option<SeqTime> {
        match monitor {
            Monitor::Program => self
                .active_seq()
                .and_then(|s| if is_in { s.mark_in } else { s.mark_out }),
            Monitor::Source => self
                .source
                .item
                .and_then(|i| self.project.item(i))
                .and_then(|i| if is_in { i.mark_in } else { i.mark_out })
                .map(|t| t.cast()),
        }
    }

    pub fn set_mark(&mut self, monitor: Monitor, is_in: bool, t: Option<SeqTime>) {
        let label = match (is_in, t.is_some()) {
            (true, true) => "Mark In",
            (false, true) => "Mark Out",
            (true, false) => "Clear In",
            (false, false) => "Clear Out",
        };
        match monitor {
            Monitor::Program => {
                self.seq_edit(label, |p, sid, _| {
                    let s = p.sequence_mut(sid).unwrap();
                    let slot = if is_in {
                        &mut s.mark_in
                    } else {
                        &mut s.mark_out
                    };
                    if *slot == t {
                        return Err(EditError::Nothing);
                    }
                    *slot = t;
                    // an In after the Out (or the reverse) clears the other mark
                    if let (Some(i), Some(o)) = (s.mark_in, s.mark_out)
                        && o <= i
                    {
                        if is_in {
                            s.mark_out = None
                        } else {
                            s.mark_in = None
                        }
                    }
                    Ok(())
                });
            }
            Monitor::Source => {
                let Some(item) = self.source.item else { return };
                self.edit(label, |p| {
                    let it = p.item_mut(item).ok_or(EditError::Nothing)?;
                    let v = t.map(|t| t.cast::<Src>());
                    let slot = if is_in {
                        &mut it.mark_in
                    } else {
                        &mut it.mark_out
                    };
                    if *slot == v {
                        return Err(EditError::Nothing);
                    }
                    *slot = v;
                    if let (Some(i), Some(o)) = (it.mark_in, it.mark_out)
                        && o <= i
                    {
                        if is_in {
                            it.mark_out = None
                        } else {
                            it.mark_in = None
                        }
                    }
                    Ok(())
                });
            }
        }
    }

    /// X: In/Out around the clip under the playhead on the topmost targeted track.
    fn mark_clip(&mut self) {
        let t = self.playhead();
        let targets = self.targets();
        let Some(seq) = self.active_seq() else { return };
        let Some(c) = clips_at(seq, &targets, t)
            .first()
            .and_then(|c| seq.clip(*c))
            .cloned()
        else {
            return;
        };
        self.seq_edit("Mark Clip", |p, sid, _| {
            let s = p.sequence_mut(sid).unwrap();
            s.mark_in = Some(c.start);
            s.mark_out = Some(c.end());
            Ok(())
        });
    }

    /// /: In/Out around the selection.
    fn mark_selection(&mut self) {
        let Some(seq) = self.active_seq() else { return };
        let clips: Vec<Clip> = self
            .selection
            .clips
            .iter()
            .filter_map(|c| seq.clip(*c))
            .cloned()
            .collect();
        let (Some(a), Some(b)) = (
            clips.iter().map(|c| c.start).min(),
            clips.iter().map(|c| c.end()).max(),
        ) else {
            return;
        };
        self.seq_edit("Mark Selection", |p, sid, _| {
            let s = p.sequence_mut(sid).unwrap();
            s.mark_in = Some(a);
            s.mark_out = Some(b);
            Ok(())
        });
    }

    fn lift_extract(&mut self, extract_mode: bool) {
        let Some(seq) = self.active_seq() else { return };
        let (Some(a), Some(b)) = (seq.mark_in, seq.mark_out) else {
            self.error("Set In and Out points first");
            return;
        };
        let targets = self.targets();
        let range = SeqRange::new(a, b);
        let ok = if extract_mode {
            self.seq_edit("Extract", |p, sid, opts| {
                extract(p, sid, range, &targets, opts)
            })
        } else {
            self.seq_edit("Lift", |p, sid, opts| lift(p, sid, range, &targets, opts))
        };
        if ok.is_some() {
            self.seq_edit("Clear In/Out", |p, sid, _| {
                let s = p.sequence_mut(sid).unwrap();
                s.mark_in = None;
                s.mark_out = None;
                Ok(())
            });
            self.history.seal();
            self.set_playhead(a);
        }
    }

    fn add_edit(&mut self, all: bool) {
        let t = self.playhead();
        let Some(seq) = self.active_seq() else { return };
        // selected clips under the playhead take precedence over targeted tracks
        let selected: Vec<ClipId> = self
            .selection
            .clips
            .iter()
            .copied()
            .filter(|c| seq.clip(*c).is_some_and(|c| c.range().contains(t)))
            .collect();
        if !all && !selected.is_empty() {
            self.seq_edit("Add Edit", |p, sid, opts| razor(p, sid, &selected, t, opts));
            return;
        }
        let tracks: Vec<TrackRef> = if all {
            seq.all_tracks().map(|(r, _)| r).collect()
        } else {
            self.targets()
        };
        self.seq_edit("Add Edit", |p, sid, opts| {
            add_edit(p, sid, &tracks, t, opts)
        });
    }

    fn default_transition(&mut self, kind: Option<TrackKind>) {
        let t = self.playhead();
        match kind {
            Some(k) => {
                let targets: Vec<TrackRef> =
                    self.targets().into_iter().filter(|r| r.kind == k).collect();
                if targets.is_empty() {
                    self.error("Target a track first");
                    return;
                }
                self.seq_edit("Apply Default Transition", |p, sid, opts| {
                    apply_default_at_playhead(p, sid, &targets, t, opts)
                });
            }
            None => {
                let clips = self.selection.clips.clone();
                self.seq_edit("Apply Default Transitions", |p, sid, opts| {
                    apply_default_to_clips(p, sid, &clips, opts)
                });
            }
        }
    }

    /// F: opens the source of the clip under the playhead at the same frame.
    fn match_frame(&mut self) {
        let t = self.playhead();
        let targets = self.targets();
        let Some(seq) = self.active_seq() else { return };
        let clip = self
            .selection
            .clips
            .iter()
            .filter_map(|c| seq.clip(*c))
            .find(|c| c.range().contains(t))
            .cloned()
            .or_else(|| {
                clips_at(seq, &targets, t)
                    .first()
                    .and_then(|c| seq.clip(*c))
                    .cloned()
            });
        let Some(c) = clip else { return };
        let Some(item) = c.source.item() else { return };
        self.load_source(item);
        let s = c.to_source(t);
        self.set_monitor_time(Monitor::Source, s.cast());
    }

    /// Shift+R: finds the Source Monitor frame in the active sequence.
    fn reverse_match_frame(&mut self) {
        let Some(item) = self.source.item else { return };
        let s: SrcTime = self.source.playhead.cast();
        let Some(seq) = self.active_seq() else { return };
        let hit = seq
            .clips()
            .map(|(_, c)| c)
            .find(|c| c.source.item() == Some(item) && c.source_range().contains(s))
            .map(|c| c.to_sequence(s));
        if let Some(t) = hit {
            self.set_playhead(t);
        } else {
            self.info("The source frame is not used in this sequence");
        }
    }

    fn nudge_volume(&mut self, db: f64) {
        let clips = self.selection.clips.clone();
        if clips.is_empty() {
            return;
        }
        self.seq_edit("Audio Gain", |p, sid, _| {
            let s = p.sequence_mut(sid).unwrap();
            let mut any = false;
            for id in &clips {
                if let Some(c) = s.clip_mut(*id)
                    && !c.is_video()
                {
                    c.gain_db = (c.gain_db + db).clamp(-96.0, 96.0);
                    any = true;
                }
            }
            if any { Ok(()) } else { Err(EditError::Nothing) }
        });
    }

    pub fn set_gain(&mut self, db: f64) {
        let clips = self.selection.clips.clone();
        self.seq_edit("Audio Gain", |p, sid, _| {
            let s = p.sequence_mut(sid).unwrap();
            for id in &clips {
                if let Some(c) = s.clip_mut(*id)
                    && !c.is_video()
                {
                    c.gain_db = db.clamp(-96.0, 96.0);
                }
            }
            Ok(())
        });
    }

    fn make_subsequence(&mut self) {
        let Some(seq) = self.active_seq().cloned() else {
            return;
        };
        let clips = self.selection.clips.clone();
        if clips.is_empty() {
            return;
        }
        let cb = Clipboard::copy(&seq, &clips);
        let name = format!("{} Sub", seq.name);
        let bin = self.bin;
        let settings = SequenceSettings {
            video_tracks: seq.video.len(),
            audio_tracks: seq.audio.len(),
            ..seq.settings.clone()
        };
        let opts = self.opts();
        let base_v = cb
            .clips
            .iter()
            .filter(|(r, _)| r.kind == TrackKind::Video)
            .map(|(r, _)| r.index)
            .min()
            .unwrap_or(0);
        let base_a = cb
            .clips
            .iter()
            .filter(|(r, _)| r.kind == TrackKind::Audio)
            .map(|(r, _)| r.index)
            .min()
            .unwrap_or(0);
        let new = self.edit("Make Subsequence", |p| {
            let (sid, _) = p.add_sequence(bin, name, settings);
            paste(p, sid, &cb, SeqTime::ZERO, base_v, base_a, false, opts)?;
            Ok(sid)
        });
        if let Some(sid) = new {
            self.open_sequence(sid);
        }
    }

    fn add_marker(&mut self, monitor: Monitor) {
        match monitor {
            Monitor::Source => {
                let Some(item) = self.source.item else { return };
                let t: SrcTime = self.source.playhead.cast();
                self.edit("Add Marker", |p| {
                    let id = p.ids.marker();
                    let it = p.item_mut(item).ok_or(EditError::Nothing)?;
                    if it.markers.iter().any(|m| m.start == t) {
                        return Err(EditError::Nothing);
                    }
                    it.markers.push(Marker::new(id, t));
                    it.markers.sort_by_key(|m| m.start);
                    Ok(())
                });
            }
            Monitor::Program => {
                let t = self.playhead();
                self.seq_edit("Add Marker", |p, sid, _| add_marker(p, sid, t).map(|_| ()));
            }
        }
    }

    fn select_adjacent(&mut self, next: bool) {
        let t = self.playhead();
        let targets = self.targets();
        let Some(seq) = self.active_seq() else { return };
        let mut candidates: Vec<&Clip> = targets
            .iter()
            .filter_map(|r| seq.track(*r))
            .flat_map(|tr| tr.clips.iter())
            .collect();
        candidates.sort_by_key(|c| c.start);
        let pick = if next {
            candidates.iter().find(|c| c.start > t)
        } else {
            candidates.iter().rev().find(|c| c.start < t)
        };
        if let Some(c) = pick {
            let (id, start) = (c.id, c.start);
            let with = if self.prefs.linked_selection {
                seq.linked(id)
            } else {
                vec![id]
            };
            self.selection = Selection::only(with);
            self.set_playhead(start);
        }
    }

    fn nudge(&mut self, frames: i64, tracks: i32) {
        let Some(seq) = self.active_seq() else { return };
        if self.selection.clips.is_empty() {
            return;
        }
        let spec = MoveSpec {
            clips: self.selection.clips.clone(),
            delta: seq.rate().frames_to_dur(frames),
            track_delta: tracks,
            insert: false,
            duplicate: false,
        };
        if let Some(ids) = self.seq_edit("Nudge", |p, sid, opts| move_clips(p, sid, &spec, opts)) {
            self.selection = Selection::only(ids);
        }
    }

    fn slip_selection(&mut self, frames: i64) {
        let Some(seq) = self.active_seq() else { return };
        let Some(&c) = self.selection.clips.first() else {
            return;
        };
        let d = seq.rate().frames_to_dur(frames);
        self.seq_edit("Slip", |p, sid, opts| slip(p, sid, c, d, opts).map(|_| ()));
    }

    fn slide_selection(&mut self, frames: i64) {
        let Some(seq) = self.active_seq() else { return };
        let Some(&c) = self.selection.clips.first() else {
            return;
        };
        let d = seq.rate().frames_to_dur(frames);
        self.seq_edit("Slide", |p, sid, opts| {
            slide(p, sid, c, d, opts).map(|_| ())
        });
    }

    /// Adds a graphics clip (text or shape) at the playhead on the lowest free targeted video
    /// track above existing material, five seconds long.
    /// Places a project item at the playhead on the first free video track above the material
    /// there, like a new graphic (used for pasted images). Selects the new clip.
    pub fn place_on_top(&mut self, item: ItemId) -> Option<ClipId> {
        let sid = self.active?;
        let spec = SourceClip::from_item(&self.project, item).ok()?;
        spec.video.as_ref()?;
        let t = self.playhead();
        let seq = self.project.sequence(sid)?.clone();
        let range = SeqRange::with_duration(t, spec.duration());
        let occupied_top = seq
            .video
            .iter()
            .enumerate()
            .filter(|(_, tr)| tr.clips_in(range).next().is_some())
            .map(|(i, _)| i + 1)
            .max()
            .unwrap_or(0);
        let index = (occupied_top..seq.video.len() + 1)
            .find(|i| {
                seq.video
                    .get(*i)
                    .is_none_or(|tr| tr.clips_in(range).next().is_none() && !tr.locked)
            })
            .unwrap_or(seq.video.len());
        let patch = Patch {
            video: Some(index),
            audio: vec![],
        };
        let ids = self.seq_edit("Paste Image", |p, sid, opts| {
            while p.sequence(sid).unwrap().video.len() <= index {
                let tid = p.ids.track();
                p.sequence_mut(sid)
                    .unwrap()
                    .video
                    .push(std::sync::Arc::new(Track::new(tid, TrackKind::Video)));
            }
            overwrite(p, sid, &spec, t, &patch, opts)
        })?;
        self.selection = Selection::only(ids.clone());
        ids.first().copied()
    }

    pub fn add_graphic(&mut self, effect: &str, shape: Option<u32>) -> Option<ClipId> {
        let sid = self.active?;
        let t = self.playhead();
        let seq = self.project.sequence(sid)?.clone();
        let len = Dur::from_seconds(5.0).round_frames(seq.rate());
        let range = SeqRange::with_duration(t, len);
        // the first video track with room, starting above the highest occupied one at t
        let occupied_top = seq
            .video
            .iter()
            .enumerate()
            .filter(|(_, tr)| tr.clips_in(range).next().is_some())
            .map(|(i, _)| i + 1)
            .max()
            .unwrap_or(0);
        let index = (occupied_top..seq.video.len() + 1)
            .find(|i| {
                seq.video
                    .get(*i)
                    .is_none_or(|tr| tr.clips_in(range).next().is_none() && !tr.locked)
            })
            .unwrap_or(seq.video.len());
        let def = catalog::find(effect)?;
        let name = match effect {
            catalog::TEXT => "Text",
            catalog::CAPTION => "Caption",
            _ => "Shape",
        }
        .to_string();
        let caption_size = (seq.settings.height as f64 * 0.065).round().max(12.0);
        let default_font = op_render::text::Fonts::default_family().to_string();
        let r = self.seq_edit("New Graphic", move |p, sid, _| {
            let mut ids = p.ids.clone();
            let mut comp = Component::new(def, &mut ids);
            if let Some(s) = shape
                && let Some(prm) = comp.param_mut("shape")
            {
                prm.value = Value::Choice(s);
            }
            if let Some(f) = comp.param_mut("font") {
                f.value = Value::Text(default_font);
            }
            if effect == catalog::CAPTION
                && let Some(f) = comp.param_mut("font_size")
            {
                f.value = Value::Float(caption_size);
            }
            let mut components = default_components(EffectKind::VideoFixed, &mut ids);
            components.push(comp);
            let clip = Clip {
                id: ids.clip(),
                name,
                kind: TrackKind::Video,
                source: ClipSource::Graphic,
                start: t,
                duration: len,
                source_in: SrcTime::ZERO,
                speed: Speed::NORMAL,
                reverse: false,
                hold: None,
                enabled: true,
                link: None,
                group: None,
                label: Label::Rose,
                components,
                gain_db: 0.0,
                scale_to_frame: false,
                channels: None,
            };
            let id = clip.id;
            p.ids = ids;
            while p.sequence(sid).unwrap().video.len() <= index {
                let tid = p.ids.track();
                p.sequence_mut(sid)
                    .unwrap()
                    .video
                    .push(std::sync::Arc::new(Track::new(tid, TrackKind::Video)));
            }
            let s = p.sequence_mut(sid).unwrap();
            let tr = s.track_mut(TrackRef::video(index)).unwrap();
            let pos = tr.clips.partition_point(|c| c.start <= t);
            tr.clips.insert(pos, clip);
            Ok(id)
        })?;
        self.selection = Selection::only(vec![r]);
        Some(r)
    }

    // ------------------------------------------------------------------------ project items

    fn delete_items(&mut self) {
        let items = self.items.clone();
        if items.is_empty() {
            return;
        }
        self.edit("Clear", |p| {
            for i in &items {
                p.remove_item(*i);
            }
            Ok(())
        });
        self.items.clear();
    }

    fn duplicate_items(&mut self) {
        let items = self.items.clone();
        let new = self.edit("Duplicate", |p| {
            let mut out = Vec::new();
            for i in &items {
                let Some(it) = p.item(*i).cloned() else {
                    continue;
                };
                if it.is_bin() {
                    continue;
                }
                let parent = it.parent.unwrap_or(p.root);
                let kind = match &it.kind {
                    ItemKind::Sequence { sequence } => {
                        let mut s = p.sequence(*sequence).unwrap().clone();
                        let nid = p.ids.sequence();
                        s.id = nid;
                        s.name = format!("{} Copy", s.name);
                        p.sequences.insert(nid, std::sync::Arc::new(s));
                        ItemKind::Sequence { sequence: nid }
                    }
                    k => k.clone(),
                };
                let id = p.add_item(parent, format!("{} Copy", it.name), kind);
                out.push(id);
            }
            if out.is_empty() {
                Err(EditError::Nothing)
            } else {
                Ok(out)
            }
        });
        if let Some(n) = new {
            self.items = n;
        }
    }

    // ---------------------------------------------------------------------- clip operations

    pub fn nest(&mut self, name: &str) {
        let clips = self.selection.clips.clone();
        if clips.is_empty() {
            return;
        }
        let bin = self.bin;
        let name = name.to_string();
        if let Some((_, new)) = self.seq_edit("Nest", |p, sid, opts| {
            nest(p, sid, &clips, &name, bin, opts)
        }) {
            self.selection = Selection::only(new);
        }
    }

    pub fn set_speed(&mut self, percent: f64, reverse: bool, ripple: bool) {
        let clips = self.selection.clips.clone();
        let speed = Speed::from_percent(percent.clamp(1.0, 100_000.0));
        self.seq_edit("Speed/Duration", |p, sid, opts| {
            set_speed(p, sid, &clips, speed, reverse, ripple, opts)
        });
    }

    pub fn frame_hold(&mut self) {
        let t = self.playhead();
        let Some(seq) = self.active_seq() else { return };
        let Some(c) = self
            .selection
            .clips
            .iter()
            .filter_map(|c| seq.clip(*c))
            .find(|c| c.is_video() && c.range().contains(t))
            .map(|c| c.id)
        else {
            return;
        };
        self.seq_edit("Add Frame Hold", |p, sid, opts| {
            add_frame_hold(p, sid, c, t, opts)
        });
    }

    /// Adds an effect to the selected clips of its kind (or to one clip).
    pub fn apply_effect(&mut self, effect: &str, clips: Option<Vec<ClipId>>) {
        let clips = clips.unwrap_or_else(|| self.selection.clips.clone());
        if let Some(preset) = op_core::presets::find(effect) {
            self.seq_edit(&format!("Add {}", preset.name), |p, sid, _| {
                let mut ids = p.ids.clone();
                let s = p.sequence_mut(sid).unwrap();
                let mut any = false;
                for id in &clips {
                    if let Some(c) = s.clip_mut(*id)
                        && c.is_video()
                    {
                        preset.apply(c, &mut ids);
                        any = true;
                    }
                }
                p.ids = ids;
                if any { Ok(()) } else { Err(EditError::Nothing) }
            });
            return;
        }
        let Some(def) = catalog::find(effect) else {
            return;
        };
        let video = def.kind.is_video();
        self.seq_edit(&format!("Add {}", def.name), |p, sid, _| {
            let mut ids = p.ids.clone();
            let s = p.sequence_mut(sid).unwrap();
            let mut any = false;
            for id in &clips {
                if let Some(c) = s.clip_mut(*id)
                    && c.is_video() == video
                {
                    c.components.push(Component::new(def, &mut ids));
                    any = true;
                }
            }
            p.ids = ids;
            if any { Ok(()) } else { Err(EditError::Nothing) }
        });
    }
}

// avoid a name clash between the editor's insert helper and op_timeline::insert
use op_timeline::insert as insert_edit;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::{Dirs, Preferences};

    /// Tests that create an editor hold `recovery::TEST_GUARD`: an editor resets the shared
    /// recovery state.
    fn editor() -> (Editor, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let e = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
        (e, dir)
    }

    fn matte(e: &mut Editor) -> ItemId {
        let root = e.project.root;
        e.edit("x", |p| {
            Ok(p.add_item(
                root,
                "Red",
                ItemKind::Synthetic {
                    generator: Generator::ColorMatte {
                        color: Rgba::new(1.0, 0.0, 0.0, 1.0),
                    },
                    duration: Dur::from_seconds(10.0),
                },
            ))
        })
        .unwrap()
    }

    #[test]
    fn three_point_edits_and_undo() {
        let _guard = crate::recovery::TEST_GUARD.lock();
        let (mut e, _d) = editor();
        let item = matte(&mut e);
        e.new_sequence("S", SequenceSettings::default());
        e.load_source(item);
        e.execute("cmd.clip.overlay", Focus::Timeline);
        let seq = e.active_seq().unwrap();
        assert_eq!(seq.video[0].clips.len(), 1);
        assert_eq!(e.playhead(), SeqTime::from_seconds(10.0));
        // source In/Out shortens the next edit
        e.set_monitor_time(Monitor::Source, SeqTime::from_seconds(2.0));
        e.execute("cmd.common.setin", Focus::Source);
        e.set_monitor_time(Monitor::Source, SeqTime::from_seconds(3.0));
        e.execute("cmd.common.setout", Focus::Source);
        e.set_playhead(SeqTime::ZERO);
        e.execute("cmd.clip.insert", Focus::Timeline);
        let seq = e.active_seq().unwrap();
        assert_eq!(seq.video[0].clips.len(), 2);
        assert_eq!(
            seq.video[0].clips[0].duration,
            Dur::from_seconds(1.0) + Rate::FPS_25.frame_duration()
        );
        e.execute("cmd.edit.undo", Focus::Timeline);
        assert_eq!(e.active_seq().unwrap().video[0].clips.len(), 1);
        e.execute("cmd.edit.redo", Focus::Timeline);
        assert_eq!(e.active_seq().unwrap().video[0].clips.len(), 2);
    }

    #[test]
    fn presets_and_effects_apply_to_clips() {
        let _guard = crate::recovery::TEST_GUARD.lock();
        let (mut e, _d) = editor();
        let item = matte(&mut e);
        e.new_sequence("S", SequenceSettings::default());
        e.load_source(item);
        e.execute("cmd.clip.overlay", Focus::Timeline);
        let clip = e.active_seq().unwrap().video[0].clips[0].id;
        e.apply_effect("op.preset.fade_in_out", Some(vec![clip]));
        e.apply_effect("op.video.radiant_glow", Some(vec![clip]));
        let seq = e.active_seq().unwrap();
        let c = seq.clip(clip).unwrap();
        assert!(
            c.component(catalog::OPACITY)
                .unwrap()
                .param("opacity")
                .unwrap()
                .animated
        );
        assert!(c.component("op.video.radiant_glow").is_some());
        e.execute("cmd.edit.undo", Focus::Timeline);
        e.execute("cmd.edit.undo", Focus::Timeline);
        let c = e.active_seq().unwrap().clip(clip).unwrap().clone();
        assert!(
            !c.component(catalog::OPACITY)
                .unwrap()
                .param("opacity")
                .unwrap()
                .animated
        );
    }

    #[test]
    fn pasted_images_land_on_top_of_the_timeline() {
        let _guard = crate::recovery::TEST_GUARD.lock();
        let (mut e, d) = editor();
        let item = matte(&mut e);
        e.new_sequence("S", SequenceSettings::default());
        e.load_source(item);
        e.execute("cmd.clip.overlay", Focus::Timeline);
        e.set_playhead(SeqTime::from_seconds(1.0));
        // a 1x1 PNG, as the paste feature writes it
        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99,
            0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let path = d.path().join("Pasted Image.png");
        std::fs::write(&path, png).unwrap();
        e.place_after_import.push(path.clone());
        let root = e.project.root;
        e.import(vec![path], root);
        let start = std::time::Instant::now();
        while (e.importing > 0 || !e.imports.is_empty())
            && start.elapsed() < std::time::Duration::from_secs(20)
        {
            e.tick();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let seq = e.active_seq().unwrap();
        assert!(seq.video.len() >= 2, "a track above V1");
        let placed = seq.video[1].clips.first().expect("the image is on V2");
        assert_eq!(placed.start, SeqTime::from_seconds(1.0));
        assert_eq!(e.selection.clips, vec![placed.id]);
        assert!(e.place_after_import.is_empty());
    }

    #[test]
    fn add_edit_lift_markers_graphics_and_transport() {
        let _guard = crate::recovery::TEST_GUARD.lock();
        let (mut e, _d) = editor();
        let item = matte(&mut e);
        e.new_sequence("S", SequenceSettings::default());
        e.load_source(item);
        e.execute("cmd.clip.overlay", Focus::Timeline);
        e.set_playhead(SeqTime::from_seconds(4.0));
        e.execute("cmd.sequence.razorateditline", Focus::Timeline);
        assert_eq!(e.active_seq().unwrap().video[0].clips.len(), 2);
        e.execute("cmd.set.marker", Focus::Timeline);
        assert_eq!(e.active_seq().unwrap().markers.len(), 1);
        e.execute("cmd.sequence.applydefaultvideotransition", Focus::Timeline);
        assert_eq!(e.active_seq().unwrap().video[0].transitions.len(), 1);
        // graphics go above existing material
        let g = e.add_graphic(catalog::TEXT, None).unwrap();
        let (r, _) = e.active_seq().unwrap().find_clip(g).unwrap();
        assert_eq!(r, TrackRef::video(1));
        // In/Out and lift
        e.set_playhead(SeqTime::from_seconds(1.0));
        e.execute("cmd.common.setin", Focus::Timeline);
        e.set_playhead(SeqTime::from_seconds(2.0));
        e.execute("cmd.common.setout", Focus::Timeline);
        e.execute("cmd.sequence.lift", Focus::Timeline);
        assert!(e.active_seq().unwrap().mark_in.is_none());
        assert_eq!(e.active_seq().unwrap().video[0].clips.len(), 3);
        // edit navigation
        e.set_playhead(SeqTime::ZERO);
        e.execute("cmd.tlnav.next.edit", Focus::Timeline);
        assert_eq!(e.playhead(), SeqTime::from_seconds(1.0));
        // tools
        e.execute("cmd.tools.06razor", Focus::Timeline);
        assert_eq!(e.tool, Tool::Razor);
        // stepping moves by frames
        e.execute("cmd.transport.step.forward", Focus::Timeline);
        assert_eq!(
            e.playhead(),
            SeqTime::from_seconds(1.0) + Rate::FPS_25.frame_duration()
        );
    }

    #[test]
    fn save_and_reopen_keeps_session() {
        let _guard = crate::recovery::TEST_GUARD.lock();
        let (mut e, d) = editor();
        let item = matte(&mut e);
        e.new_sequence("S", SequenceSettings::default());
        e.load_source(item);
        e.execute("cmd.clip.overlay", Focus::Timeline);
        let path = d.path().join("demo.opproj");
        e.save_as(&path).unwrap();
        assert!(!e.history.is_dirty());
        let (mut f, _d2) = editor();
        f.open(&path).unwrap();
        assert_eq!(f.active_seq().unwrap().video[0].clips.len(), 1);
        assert_eq!(f.playhead(), SeqTime::from_seconds(10.0));
    }
}
