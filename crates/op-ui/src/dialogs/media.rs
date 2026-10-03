//! Link Media, and Paste and Remove Attributes.

use super::*;

/// File > Link Media: locate missing files by hand or search this computer, then relink them.
pub(super) fn link_media_dialog(s: &mut State, ctx: &egui::Context, form: &mut LinkForm) -> bool {
    use op_application::relink;
    // results of a running search
    if let Some(search) = &form.search {
        let st = search.state.lock();
        for (id, p) in &st.found {
            form.found.entry(*id).or_insert_with(|| p.clone());
        }
        if st.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
    let searching = form.search.as_ref().is_some_and(|x| x.state.lock().running);
    let mut locate: Option<usize> = None;
    let mut start_search = false;
    let mut link = false;
    let mut close = false;
    let total = form.missing.len();
    let (_, esc) = modal(ctx, "link-media", t("Link Media"), 620.0, |ui| {
        ui.label(tf(
            "{} media files could not be found where the project expects them. Locate them, or let OpenPremier search this computer.",
            &[&total],
        ));
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (i, m) in form.missing.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(470.0);
                            ui.label(
                                RichText::new(&m.name)
                                    .strong()
                                    .family(crate::fonts::strong())
                                    .color(theme::TEXT_BRIGHT),
                            );
                            match form.found.get(&m.asset) {
                                Some(p) => ui.label(
                                    RichText::new(tf("Found: {}", &[&p.display()]))
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(110, 200, 140)),
                                ),
                                None => ui.label(
                                    RichText::new(tf("Missing: {}", &[&m.path]))
                                        .size(11.0)
                                        .color(theme::ERROR),
                                ),
                            };
                        });
                        if ui.button(t("Locate...")).clicked() {
                            locate = Some(i);
                        }
                    });
                    ui.add_space(3.0);
                }
            });
        ui.add_space(6.0);
        if let Some(search) = &form.search {
            let st = search.state.lock();
            ui.horizontal(|ui| {
                if st.running {
                    ui.spinner();
                    ui.label(
                        RichText::new(tf("Searching... {} folders", &[&st.folders]))
                            .color(theme::TEXT_DIM),
                    );
                } else {
                    ui.label(
                        RichText::new(tf(
                            "Search finished: {} of {} found",
                            &[&form.found.len(), &total],
                        ))
                        .color(theme::TEXT_DIM),
                    );
                }
            });
            if st.running {
                let cur: String = st
                    .current
                    .chars()
                    .rev()
                    .take(80)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                ui.label(RichText::new(cur).size(10.5).color(theme::TEXT_DIM));
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if searching {
                if ui.button(t("Stop Search")).clicked()
                    && let Some(x) = &form.search
                {
                    x.cancel();
                }
            } else if ui
                .add_enabled(form.found.len() < total, egui::Button::new(t("Search Automatically")))
                .on_hover_text(t(
                    "Looks in the project folder, your media folders and every drive, by file name; when several files share a name, the one whose duration and size match wins.",
                ))
                .clicked()
            {
                start_search = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let n = form.found.len();
                if ui
                    .add_enabled(
                        n > 0,
                        egui::Button::new(
                            RichText::new(tf("Link {} Files", &[&n])).color(egui::Color32::WHITE),
                        )
                        .fill(theme::ACCENT_DIM),
                    )
                    .clicked()
                {
                    link = true;
                }
                if ui.button(t("Offline All")).on_hover_text(t("Keep working without them; File > Link Media finds them later.")).clicked() {
                    close = true;
                }
            });
        });
    });
    if let Some(i) = locate {
        let m = form.missing[i].clone();
        let start = Path::new(&m.path)
            .parent()
            .filter(|d| d.is_dir())
            .map(|d| d.to_path_buf())
            .or_else(|| {
                s.ed.path
                    .as_ref()
                    .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            });
        let mut dlg = rfd::FileDialog::new()
            .set_title(tf("Where is {}?", &[&m.name]))
            .set_file_name(&m.name);
        if let Some(dir) = start {
            dlg = dlg.set_directory(dir);
        }
        if let Some(p) = dlg.pick_file() {
            // the others are often in the same folder
            if let Some(dir) = p.parent() {
                for (id, sib) in relink::siblings(dir, &form.missing) {
                    form.found.entry(id).or_insert(sib);
                }
            }
            form.found.insert(m.asset, p);
        }
    }
    if start_search {
        let hints: Vec<PathBuf> = form
            .found
            .values()
            .filter_map(|p| p.parent().map(|d| d.to_path_buf()))
            .collect();
        let roots = relink::search_roots(s.ed.path.as_deref(), &hints);
        let list: Vec<(MediaAsset, String)> = form
            .missing
            .iter()
            .filter(|m| !form.found.contains_key(&m.asset))
            .filter_map(|m| {
                s.ed.project
                    .asset(m.asset)
                    .map(|a| (a.clone(), m.name.clone()))
            })
            .collect();
        form.search = Some(relink::Search::start(list, roots));
    }
    if link {
        if let Some(x) = &form.search {
            x.cancel();
        }
        let links: Vec<(AssetId, PathBuf)> =
            form.found.iter().map(|(a, p)| (*a, p.clone())).collect();
        match s.ed.link_media(&links) {
            Ok(n) => s.ed.info(format!("Linked {n} media files")),
            Err(e) => s.ed.error(e),
        }
        return false;
    }
    if close || esc {
        if let Some(x) = &form.search {
            x.cancel();
        }
        return false;
    }
    true
}

/// Paste Attributes (from the copied clip) and Remove Attributes share their choices.
pub(super) fn attributes_dialog(
    s: &mut State,
    ctx: &egui::Context,
    a: &mut Attributes,
    paste: bool,
) -> bool {
    let title = if paste {
        t("Paste Attributes")
    } else {
        t("Remove Attributes")
    };
    let ((ok, cancel), esc) = modal(ctx, "attributes", title, 300.0, |ui| {
        ui.label(RichText::new(t("Video Attributes")).color(theme::TEXT_DIM));
        ui.checkbox(&mut a.motion, tn("Motion"));
        ui.checkbox(&mut a.opacity, tn("Opacity"));
        ui.checkbox(&mut a.effects, t("Effects"));
        ui.label(RichText::new(t("Audio Attributes")).color(theme::TEXT_DIM));
        ui.checkbox(&mut a.volume, tn("Volume"));
        ui.checkbox(&mut a.channel_volume, tn("Channel Volume"));
        ui.checkbox(&mut a.panner, tn("Panner"));
        buttons(ui, t("OK"))
    });
    if ok && paste {
        s.ed.paste_attributes(*a);
    } else if ok {
        s.ed.remove_attributes(*a);
    }
    !(ok || cancel || esc)
}
