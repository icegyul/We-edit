//! The Project panel: bins, media, sequences and synthetic items in list or icon view.

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use op_application::{Editor, Focus, Monitor};
use op_core::*;

use crate::app::{Drag, State};
use crate::dialogs::Dialog;
use crate::i18n::{t, tf};
use crate::icons::{self, Icon};
use crate::theme;
use crate::timeline::label_name;

pub struct ProjectView {
    pub icons: bool,
    pub search: String,
    pub focus_search: bool,
    rename: Option<(ItemId, String)>,
    icon_size: f32,
    anchor: Option<ItemId>,
}

impl Default for ProjectView {
    fn default() -> Self {
        ProjectView {
            icons: false,
            search: String::new(),
            focus_search: false,
            rename: None,
            icon_size: 120.0,
            anchor: None,
        }
    }
}

impl ProjectView {
    pub fn start_rename(&mut self, ed: &Editor, id: ItemId) {
        if let Some(it) = ed.project.item(id) {
            self.rename = Some((id, it.name.clone()));
        }
    }
}

fn item_icon(p: &Project, it: &ProjectItem) -> Icon {
    match &it.kind {
        ItemKind::Bin { .. } => Icon::Folder,
        ItemKind::Sequence { .. } => Icon::Sequence,
        ItemKind::Media { asset, .. } => match p.asset(*asset) {
            Some(a) if a.is_still() => Icon::Image,
            Some(a) if !a.has_video() => Icon::Music,
            _ => Icon::Film,
        },
        ItemKind::Synthetic { .. } => Icon::Film,
    }
}

fn item_rate(p: &Project, it: &ProjectItem) -> Option<Rate> {
    match &it.kind {
        ItemKind::Media { asset, .. } => p
            .asset(*asset)
            .and_then(|a| if a.is_still() { None } else { a.frame_rate() }),
        ItemKind::Sequence { sequence } => p.sequence(*sequence).map(|s| s.rate()),
        _ => None,
    }
}

fn item_duration(p: &Project, it: &ProjectItem) -> Option<Dur> {
    match &it.kind {
        ItemKind::Media { asset, subclip } => subclip.map(|r| r.duration()).or_else(|| {
            p.asset(*asset).and_then(|a| {
                if a.is_still() {
                    None
                } else {
                    a.available().map(|r| r.duration())
                }
            })
        }),
        ItemKind::Sequence { sequence } => p.sequence(*sequence).map(|s| s.duration()),
        ItemKind::Synthetic { duration, .. } => Some(*duration),
        ItemKind::Bin { .. } => None,
    }
}

fn video_info(p: &Project, it: &ProjectItem) -> String {
    match &it.kind {
        ItemKind::Media { asset, .. } => p
            .asset(*asset)
            .and_then(|a| a.video.as_ref().map(|v| (v, a)))
            .map(|(v, a)| {
                let (w, h) = v.display_size();
                let par = a.interpretation.pixel_aspect.unwrap_or(v.pixel_aspect);
                format!("{w} x {h} ({:.2})", par.0 as f64 / par.1.max(1) as f64)
            })
            .unwrap_or_default(),
        ItemKind::Sequence { sequence } => p
            .sequence(*sequence)
            .map(|s| format!("{} x {}", s.settings.width, s.settings.height))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn audio_info(p: &Project, it: &ProjectItem) -> String {
    match &it.kind {
        ItemKind::Media { asset, .. } => p
            .asset(*asset)
            .and_then(|a| a.audio.first())
            .map(|s| {
                let layout = match s.layout.channels() {
                    1 => t("Mono").to_string(),
                    2 => t("Stereo").to_string(),
                    n => tf("{} channels", &[&n]),
                };
                format!("{} Hz - {layout}", s.sample_rate)
            })
            .unwrap_or_default(),
        ItemKind::Sequence { sequence } => p
            .sequence(*sequence)
            .map(|s| format!("{} Hz", s.settings.audio_rate))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn is_offline(p: &Project, it: &ProjectItem) -> bool {
    match &it.kind {
        ItemKind::Media { asset, .. } => p
            .asset(*asset)
            .is_some_and(|a| !std::path::Path::new(&a.path).exists()),
        _ => false,
    }
}

/// Items shown: the current bin, or every match of the search.
fn listed(s: &State) -> Vec<ItemId> {
    let p = &s.ed.project;
    let q = s.proj.search.trim().to_lowercase();
    if q.is_empty() {
        let mut v = p.children(s.ed.bin).to_vec();
        // bins first, then by name
        v.sort_by_key(|i| {
            p.item(*i)
                .map(|x| (!x.is_bin(), x.name.to_lowercase()))
                .unwrap_or((true, String::new()))
        });
        v
    } else {
        p.walk()
            .into_iter()
            .map(|(_, id)| id)
            .filter(|id| {
                *id != p.root
                    && p.item(*id)
                        .is_some_and(|x| x.name.to_lowercase().contains(&q))
            })
            .collect()
    }
}

fn click(s: &mut State, id: ItemId, list: &[ItemId], m: egui::Modifiers) {
    if m.command {
        if let Some(pos) = s.ed.items.iter().position(|x| *x == id) {
            s.ed.items.remove(pos);
        } else {
            s.ed.items.push(id);
        }
        s.proj.anchor = Some(id);
    } else if m.shift
        && let Some(a) = s.proj.anchor
        && let (Some(i0), Some(i1)) = (
            list.iter().position(|x| *x == a),
            list.iter().position(|x| *x == id),
        )
    {
        let (lo, hi) = (i0.min(i1), i0.max(i1));
        s.ed.items = list[lo..=hi].to_vec();
    } else {
        s.ed.items = vec![id];
        s.proj.anchor = Some(id);
    }
}

fn open_item(s: &mut State, id: ItemId) {
    let Some(it) = s.ed.project.item(id) else {
        return;
    };
    match it.kind {
        ItemKind::Bin { .. } => {
            s.ed.bin = id;
            s.ed.items.clear();
            s.proj.search.clear();
        }
        ItemKind::Sequence { sequence } => s.ed.open_sequence(sequence),
        _ => {
            s.ed.load_source(id);
            s.focus = Focus::Source;
            s.command("uif.window.Source Monitors");
        }
    }
}

fn move_into_bin(s: &mut State, items: Vec<ItemId>, bin: ItemId) {
    s.ed.edit("Move", |p| {
        let mut any = false;
        for i in &items {
            if *i != bin && !p.is_ancestor(*i, bin) {
                any |= p.move_item(*i, bin);
            }
        }
        if any { Ok(()) } else { Err(EditError::Nothing) }
    });
}

pub fn show(s: &mut State, ui: &mut Ui) {
    let full = ui.max_rect();
    let top_h = 30.0;
    let bottom_h = 28.0;
    let top = Rect::from_min_size(full.min, vec2(full.width(), top_h));
    let bottom = Rect::from_min_max(pos2(full.min.x, full.max.y - bottom_h), full.max);
    let content = Rect::from_min_max(pos2(full.min.x, top.max.y), pos2(full.max.x, bottom.min.y));

    ui.scope_builder(
        egui::UiBuilder::new().max_rect(top.shrink2(vec2(6.0, 4.0))),
        |ui| {
            ui.horizontal(|ui| {
                // breadcrumb
                let mut chain = Vec::new();
                let mut cur = Some(s.ed.bin);
                while let Some(c) = cur {
                    chain.push(c);
                    cur = s.ed.project.item(c).and_then(|i| i.parent);
                }
                chain.reverse();
                for (i, id) in chain.iter().enumerate() {
                    let name = if *id == s.ed.project.root {
                        s.ed.project.name.clone()
                    } else {
                        s.ed.project
                            .item(*id)
                            .map(|x| x.name.clone())
                            .unwrap_or_default()
                    };
                    if i > 0 {
                        ui.label(RichText::new("\u{203A}").color(theme::TEXT_DIM));
                    }
                    let r = ui.add(
                        egui::Label::new(RichText::new(name).color(if i + 1 == chain.len() {
                            theme::TEXT_BRIGHT
                        } else {
                            theme::ACCENT
                        }))
                        .sense(Sense::click()),
                    );
                    if r.clicked() {
                        s.ed.bin = *id;
                        s.proj.search.clear();
                    }
                    // dropping items on a breadcrumb moves them there
                    if r.hovered()
                        && ui.input(|i| i.pointer.any_released())
                        && let Some(Drag::Items(items)) = s.drag.clone()
                    {
                        move_into_bin(s, items, *id);
                        s.drag = None;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = s.ed.project.children(s.ed.bin).len();
                    ui.label(
                        RichText::new(tf("{} items", &[&n]))
                            .color(theme::TEXT_DIM)
                            .size(11.0),
                    );
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut s.proj.search)
                            .hint_text(t("Search"))
                            .desired_width(150.0),
                    );
                    if std::mem::take(&mut s.proj.focus_search) {
                        r.request_focus();
                    }
                });
            });
        },
    );

    let list = listed(s);
    let resp_bg = ui.interact(content, ui.id().with("proj-bg"), Sense::click());
    if resp_bg.clicked() {
        s.ed.items.clear();
    }
    ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
        if s.proj.icons {
            icon_view(s, ui, &list)
        } else {
            list_view(s, ui, &list)
        }
    });
    resp_bg.context_menu(|ui| new_menu(s, ui));
    if list.is_empty() {
        ui.painter().text(
            content.center(),
            Align2::CENTER_CENTER,
            if cfg!(target_os = "macos") {
                t("Import media to start (Cmd+I) or drop files here")
            } else {
                t("Import media to start (Ctrl+I) or drop files here")
            },
            FontId::proportional(12.5),
            theme::TEXT_DIM,
        );
    }

    ui.scope_builder(
        egui::UiBuilder::new().max_rect(bottom.shrink2(vec2(6.0, 3.0))),
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if icons::button(ui, Icon::List, 20.0, !s.proj.icons, t("List View")).clicked() {
                    s.proj.icons = false;
                }
                if icons::button(ui, Icon::Grid, 20.0, s.proj.icons, t("Icon View")).clicked() {
                    s.proj.icons = true;
                }
                if s.proj.icons {
                    ui.add(
                        egui::Slider::new(&mut s.proj.icon_size, 70.0..=260.0).show_value(false),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icons::button(ui, Icon::Trash, 20.0, false, t("Clear")).clicked() {
                        s.focus = Focus::Project;
                        s.command("cmd.edit.clear");
                    }
                    let r = icons::button(ui, Icon::Plus, 20.0, false, t("New Item"));
                    egui::Popup::menu(&r).show(|ui| new_menu(s, ui));
                    if icons::button(ui, Icon::Folder, 20.0, false, t("New Bin")).clicked() {
                        s.command("cmd.file.new.bin");
                    }
                    if icons::button(ui, Icon::Search, 20.0, false, t("Import...")).clicked() {
                        s.command("cmd.file.import");
                    }
                });
            });
        },
    );
}

fn new_menu(s: &mut State, ui: &mut Ui) {
    let entry = |s: &mut State, ui: &mut Ui, label: &'static str, cmd: &str| {
        if ui.button(t(label)).clicked() {
            s.command(cmd);
            ui.close();
        }
    };
    entry(s, ui, "Sequence...", "cmd.file.new.sequence");
    entry(s, ui, "Bin", "cmd.file.new.bin");
    ui.separator();
    entry(s, ui, "Color Matte...", "op.new.colormatte");
    entry(s, ui, "Black Video", "op.new.blackvideo");
    entry(s, ui, "Transparent Video", "op.new.transparentvideo");
    entry(s, ui, "Bars and Tone", "op.new.barsandtone");
    entry(s, ui, "Adjustment Layer", "op.new.adjustmentlayer");
    ui.separator();
    entry(s, ui, "Import...", "cmd.file.import");
}

fn item_menu(s: &mut State, ui: &mut Ui, id: ItemId) {
    let Some(it) = s.ed.project.item(id).cloned() else {
        return;
    };
    if ui.button(t("Rename")).clicked() {
        s.proj.rename = Some((id, it.name.clone()));
        ui.close();
    }
    if !it.is_bin() {
        if ui.button(t("Open in Source Monitor")).clicked() {
            s.ed.load_source(id);
            ui.close();
        }
        if matches!(it.kind, ItemKind::Media { .. } | ItemKind::Synthetic { .. })
            && ui.button(t("New Sequence From Clip")).clicked()
        {
            s.ed.sequence_from_item(id);
            ui.close();
        }
    }
    if let ItemKind::Sequence { sequence } = it.kind
        && ui.button(t("Open in Timeline")).clicked()
    {
        s.ed.open_sequence(sequence);
        ui.close();
    }
    if let ItemKind::Media { asset, .. } = it.kind
        && let Some(a) = s.ed.project.asset(asset).cloned()
    {
        if a.has_video() && !a.is_still() && ui.button(t("Interpret Footage...")).clicked() {
            let fps = a
                .interpretation
                .frame_rate
                .or(a.frame_rate())
                .map(|r| r.as_f64())
                .unwrap_or(25.0);
            s.dialogs.push(Dialog::Interpret {
                asset,
                fps,
                alpha_ignore: a.interpretation.ignore_alpha,
            });
            ui.close();
        }
        if a.has_video() && !a.is_still() {
            // the menu acts on the selection; a right-click outside it selects this item
            if !s.ed.items.contains(&id) {
                s.ed.items = vec![id];
            }
            s.focus = Focus::Project;
            crate::app::proxy_menu(ui, s);
        }
        if !std::path::Path::new(&a.path).exists() && ui.button(t("Link Media...")).clicked() {
            ui.close();
            s.command("op.file.linkmedia");
        }
        if ui.button(t("Relink Media...")).clicked() {
            ui.close();
            if let Some(path) = rfd::FileDialog::new()
                .set_title(t("Relink Media..."))
                .pick_file()
            {
                match op_media::probe(&path) {
                    Ok(mut probed) => {
                        probed.id = asset;
                        probed.interpretation = a.interpretation.clone();
                        let probed = std::sync::Arc::new(probed);
                        s.ed.media.forget(asset);
                        s.ed.edit("Relink Media", |p| {
                            p.assets.insert(asset, probed);
                            Ok(())
                        });
                    }
                    Err(e) => s.ed.error(e.to_string()),
                }
            }
        }
    }
    ui.menu_button(t("Label"), |ui| {
        for l in Label::ALL {
            let [r, g, b] = l.rgb();
            if ui
                .button(
                    RichText::new(format!("\u{25A0} {}", t(label_name(l))))
                        .color(Color32::from_rgb(r, g, b)),
                )
                .clicked()
            {
                let items = if s.ed.items.contains(&id) {
                    s.ed.items.clone()
                } else {
                    vec![id]
                };
                s.ed.edit("Label", |p| {
                    for i in &items {
                        if let Some(x) = p.item_mut(*i) {
                            x.label = l;
                        }
                    }
                    Ok(())
                });
                ui.close();
            }
        }
    });
    ui.separator();
    if ui.button(t("Clear")).clicked() {
        if !s.ed.items.contains(&id) {
            s.ed.items = vec![id];
        }
        s.focus = Focus::Project;
        s.command("cmd.edit.clear");
        ui.close();
    }
}

fn row_interaction(s: &mut State, ui: &mut Ui, resp: &egui::Response, id: ItemId, list: &[ItemId]) {
    let m = ui.input(|i| i.modifiers);
    if resp.clicked() {
        click(s, id, list, m);
        s.focus = Focus::Project;
    }
    if resp.double_clicked() {
        open_item(s, id);
    }
    if resp.drag_started() {
        if !s.ed.items.contains(&id) {
            s.ed.items = vec![id];
        }
        s.drag = Some(Drag::Items(s.ed.items.clone()));
    }
    // dropping on a bin moves the dragged items into it
    let is_bin = s.ed.project.item(id).is_some_and(|x| x.is_bin());
    if is_bin
        && resp.contains_pointer()
        && let Some(Drag::Items(items)) = s.drag.clone()
        && !items.contains(&id)
    {
        ui.painter().rect_stroke(
            resp.rect,
            2.0,
            Stroke::new(1.5, theme::ACCENT),
            StrokeKind::Inside,
        );
        if ui.input(|i| i.pointer.any_released()) {
            move_into_bin(s, items, id);
            s.drag = None;
        }
    }
    resp.context_menu(|ui| item_menu(s, ui, id));
}

fn name_cell(s: &mut State, ui: &mut Ui, id: ItemId, r: Rect, name: &str, color: Color32) {
    if s.proj.rename.as_ref().is_some_and(|(i, _)| *i == id) {
        let mut text = s.proj.rename.as_ref().unwrap().1.clone();
        let resp = ui.put(
            r,
            egui::TextEdit::singleline(&mut text).font(FontId::proportional(12.0)),
        );
        resp.request_focus();
        if resp.lost_focus() {
            s.proj.rename = None;
            let cancelled = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if !cancelled && !text.trim().is_empty() {
                let new = text.trim().to_string();
                s.ed.edit("Rename", |p| {
                    let seq = match p.item(id).map(|i| i.kind.clone()) {
                        Some(ItemKind::Sequence { sequence }) => Some(sequence),
                        _ => None,
                    };
                    p.item_mut(id).ok_or(EditError::Nothing)?.name = new.clone();
                    if let Some(sq) = seq.and_then(|sq| p.sequence_mut(sq)) {
                        sq.name = new;
                    }
                    Ok(())
                });
            }
        } else {
            s.proj.rename = Some((id, text));
        }
    } else {
        ui.painter().with_clip_rect(r).text(
            r.left_center(),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(12.0),
            color,
        );
    }
}

/// Info columns (label, start, end) that fit next to a name column of at least 150 px.
fn columns(width: f32) -> Vec<(&'static str, f32, f32)> {
    let all = [
        (t("Frame Rate"), 78.0f32),
        (t("Duration"), 88.0),
        (t("Video Info"), 120.0),
        (t("Audio Info"), 130.0),
    ];
    let mut n = all.len();
    while n > 0 && all[..n].iter().map(|c| c.1).sum::<f32>() + 150.0 > width {
        n -= 1;
    }
    let mut x = width - all[..n].iter().map(|c| c.1).sum::<f32>();
    let mut out = Vec::new();
    for (label, w) in &all[..n] {
        out.push((*label, x, x + w));
        x += w;
    }
    out
}

fn list_view(s: &mut State, ui: &mut Ui, list: &[ItemId]) {
    let row_h = 22.0;
    let full = ui.max_rect();
    let cols = columns(full.width());
    let header = Rect::from_min_size(full.min, vec2(full.width(), 20.0));
    ui.painter().rect_filled(header, 0.0, theme::PANEL_DARK);
    ui.painter().text(
        pos2(header.min.x + 30.0, header.center().y),
        Align2::LEFT_CENTER,
        t("Name"),
        FontId::proportional(11.0),
        theme::TEXT_DIM,
    );
    for (label, x0, x1) in &cols {
        let cell = Rect::from_min_max(
            pos2(header.min.x + x0, header.min.y),
            pos2(header.min.x + x1 - 4.0, header.max.y),
        );
        ui.painter().with_clip_rect(cell).text(
            cell.left_center(),
            Align2::LEFT_CENTER,
            *label,
            FontId::proportional(11.0),
            theme::TEXT_DIM,
        );
    }
    let body = Rect::from_min_max(pos2(full.min.x, header.max.y), full.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(
            ui,
            row_h,
            list.len(),
            |ui, range| {
                for i in range {
                    let id = list[i];
                    let Some(it) = s.ed.project.item(id).cloned() else {
                        continue;
                    };
                    let (r, resp) = ui.allocate_exact_size(
                        vec2(ui.available_width(), row_h),
                        Sense::click_and_drag(),
                    );
                    let selected = s.ed.items.contains(&id);
                    let fill = if selected {
                        theme::ACCENT_DIM
                    } else if resp.hovered() {
                        theme::RAISED
                    } else if i % 2 == 0 {
                        theme::PANEL
                    } else {
                        Color32::from_rgb(0x27, 0x27, 0x2c)
                    };
                    ui.painter().rect_filled(r, 0.0, fill);
                    let p = &s.ed.project;
                    let [lr, lg, lb] = if it.label == Label::None {
                        [0, 0, 0]
                    } else {
                        it.label.rgb()
                    };
                    if it.label != Label::None {
                        ui.painter().rect_filled(
                            Rect::from_min_size(r.min + vec2(3.0, 5.0), vec2(4.0, row_h - 10.0)),
                            1.0,
                            Color32::from_rgb(lr, lg, lb),
                        );
                    }
                    icons::draw(
                        ui.painter(),
                        Rect::from_min_size(r.min + vec2(10.0, 3.0), vec2(16.0, 16.0)),
                        item_icon(p, &it),
                        theme::TEXT_DIM,
                    );
                    let offline = is_offline(p, &it);
                    let rate = item_rate(p, &it)
                        .map(|x| format!("{:.3} fps", x.as_f64()))
                        .unwrap_or_default();
                    let dur = item_duration(p, &it)
                        .map(|d| {
                            let fmt = TimecodeFormat::new(
                                item_rate(p, &it).unwrap_or(Rate::FPS_25),
                                false,
                            );
                            fmt.format(d)
                        })
                        .unwrap_or_default();
                    let vinfo = video_info(p, &it);
                    let ainfo = audio_info(p, &it);
                    let name_color = if offline {
                        theme::ERROR
                    } else {
                        theme::TEXT_BRIGHT
                    };
                    let name = if offline {
                        format!("{} ({})", it.name, t("Offline"))
                    } else {
                        it.name.clone()
                    };
                    let pr = ui.painter().clone();
                    let texts = [rate, dur, vinfo, ainfo];
                    for (i, (_, x0, x1)) in cols.iter().enumerate() {
                        let c = Rect::from_min_max(
                            pos2(r.min.x + x0, r.min.y),
                            pos2(r.min.x + x1 - 4.0, r.max.y),
                        );
                        pr.with_clip_rect(c).text(
                            c.left_center(),
                            Align2::LEFT_CENTER,
                            &texts[i],
                            FontId::proportional(11.5),
                            theme::TEXT,
                        );
                    }
                    let name_end = cols.first().map(|c| c.1 - 6.0).unwrap_or(r.width());
                    name_cell(
                        s,
                        ui,
                        id,
                        Rect::from_min_max(
                            pos2(r.min.x + 30.0, r.min.y + 1.0),
                            pos2(r.min.x + name_end, r.max.y - 1.0),
                        ),
                        &name,
                        name_color,
                    );
                    row_interaction(s, ui, &resp, id, list);
                }
            },
        );
    });
}

fn icon_view(s: &mut State, ui: &mut Ui, list: &[ItemId]) {
    let tile_w = s.proj.icon_size;
    let tile_h = tile_w * 9.0 / 16.0 + 34.0;
    let full = ui.max_rect();
    let per_row = ((full.width() - 8.0) / (tile_w + 8.0)).floor().max(1.0) as usize;
    let rows = list.len().div_ceil(per_row);
    egui::ScrollArea::vertical().auto_shrink(false).show_rows(
        ui,
        tile_h + 8.0,
        rows,
        |ui, range| {
            for row in range {
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    for k in 0..per_row {
                        let Some(&id) = list.get(row * per_row + k) else {
                            break;
                        };
                        let Some(it) = s.ed.project.item(id).cloned() else {
                            continue;
                        };
                        let (r, resp) =
                            ui.allocate_exact_size(vec2(tile_w, tile_h), Sense::click_and_drag());
                        let selected = s.ed.items.contains(&id);
                        ui.painter().rect_filled(
                            r,
                            3.0,
                            if selected {
                                theme::ACCENT_DIM
                            } else if resp.hovered() {
                                theme::RAISED
                            } else {
                                theme::PANEL_DARK
                            },
                        );
                        let pic = Rect::from_min_size(
                            r.min + vec2(3.0, 3.0),
                            vec2(tile_w - 6.0, (tile_w - 6.0) * 9.0 / 16.0),
                        );
                        ui.painter().rect_filled(pic, 2.0, Color32::from_gray(18));
                        let mut drawn = false;
                        if let ItemKind::Media { asset, subclip } = &it.kind
                            && let Some(a) = s.ed.project.asset(*asset).cloned()
                            && a.has_video()
                        {
                            let index = subclip
                                .and_then(|sc| a.frame_rate().map(|rt| rt.frame_of(sc.start)))
                                .unwrap_or(0)
                                .max(0);
                            let index = if a.is_still() {
                                0
                            } else {
                                index.max(
                                    a.frame_rate()
                                        .map(|rt| rt.dur_to_frames_floor(Dur::from_seconds(1.0)))
                                        .unwrap_or(0)
                                        .min(a.video.as_ref().map(|v| v.frames / 3).unwrap_or(0)),
                                )
                            };
                            if let Some(tex) = s.thumb(ui.ctx(), *asset, index) {
                                ui.painter().image(
                                    tex,
                                    pic,
                                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                                drawn = true;
                            }
                        }
                        if let ItemKind::Synthetic {
                            generator: Generator::ColorMatte { color },
                            ..
                        } = &it.kind
                        {
                            let [cr, cg, cb, _] = color.to_u8();
                            ui.painter()
                                .rect_filled(pic, 2.0, Color32::from_rgb(cr, cg, cb));
                            drawn = true;
                        }
                        if !drawn {
                            icons::draw(
                                ui.painter(),
                                Rect::from_center_size(pic.center(), vec2(32.0, 32.0)),
                                item_icon(&s.ed.project, &it),
                                theme::TEXT_DIM,
                            );
                        }
                        if let Some(d) = item_duration(&s.ed.project, &it) {
                            let fmt = TimecodeFormat::new(
                                item_rate(&s.ed.project, &it).unwrap_or(Rate::FPS_25),
                                false,
                            );
                            ui.painter().text(
                                pic.right_bottom() - vec2(4.0, 3.0),
                                Align2::RIGHT_BOTTOM,
                                fmt.format(d),
                                FontId::monospace(10.0),
                                Color32::from_white_alpha(220),
                            );
                        }
                        if it.label != Label::None {
                            let [lr, lg, lb] = it.label.rgb();
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    pos2(r.min.x + 4.0, pic.max.y + 8.0),
                                    vec2(8.0, 8.0),
                                ),
                                1.0,
                                Color32::from_rgb(lr, lg, lb),
                            );
                        }
                        let name_r = Rect::from_min_max(
                            pos2(r.min.x + 16.0, pic.max.y + 4.0),
                            pos2(r.max.x - 4.0, r.max.y - 4.0),
                        );
                        let color = if is_offline(&s.ed.project, &it) {
                            theme::ERROR
                        } else {
                            theme::TEXT_BRIGHT
                        };
                        name_cell(s, ui, id, name_r, &it.name, color);
                        row_interaction(s, ui, &resp, id, list);
                    }
                });
                ui.add_space(8.0);
            }
        },
    );
    let _ = Monitor::Source;
}
