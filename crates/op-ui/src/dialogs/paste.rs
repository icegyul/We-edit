//! Pasted images: where they are saved.

use super::*;

/// Asks where to save a pasted image; "Always save here" skips the question next time.
pub(super) fn paste_dialog(
    s: &mut State,
    ctx: &egui::Context,
    id: u64,
    folder: &mut String,
    name: &mut String,
    always: &mut bool,
) -> bool {
    let Some(pos) = s.pastes.iter().position(|j| j.id == id) else {
        return false;
    };
    // the preview is made once the data is there (links download in the background)
    let state = {
        let data = s.pastes[pos].data.lock();
        match data.as_ref() {
            None => None,
            Some(Ok(p)) => Some(Ok(crate::paste::preview_image(p))),
            Some(Err(e)) => Some(Err(e.clone())),
        }
    };
    if s.pastes[pos].preview.is_none()
        && let Some(Ok(Some(img))) = &state
    {
        s.pastes[pos].preview = Some(ctx.load_texture(
            format!("paste-{id}"),
            img.clone(),
            egui::TextureOptions::LINEAR,
        ));
    }
    let mut open = true;
    let mut save = false;
    let mut cancel = false;
    let mut browse = false;
    window(ctx, t("Paste Image"))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_width(480.0);
            let job = &s.pastes[pos];
            if let Some(tex) = &job.preview {
                let size = tex.size_vec2();
                let k = (480.0 / size.x).min(240.0 / size.y).min(1.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::new((tex.id(), size * k)));
                });
            }
            ui.label(
                RichText::new(&job.source)
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
            match &state {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t("Downloading..."));
                    });
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(tf("Could not get the image: {}", &[e])).color(theme::ERROR));
                }
                Some(Ok(_)) => {}
            }
            ui.add_space(6.0);
            egui::Grid::new("paste-grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("File Name"));
                    ui.add(egui::TextEdit::singleline(name).desired_width(340.0));
                    ui.end_row();
                    ui.label(t("Save In"));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(folder).desired_width(260.0));
                        if ui.button(t("Browse...")).clicked() {
                            browse = true;
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(always, t("Always save here"))
                        .on_hover_text(t("Pasted images will be saved in this folder without asking. You can change it in Preferences."));
                    ui.end_row();
                });
            let ready = matches!(state, Some(Ok(_)));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("Cancel")).clicked() {
                        cancel = true;
                    }
                    if ui
                        .add_enabled(
                            ready && !folder.trim().is_empty(),
                            egui::Button::new(t("Save and Import")),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if browse
        && let Some(dir) = rfd::FileDialog::new()
            .set_directory(folder.trim())
            .pick_folder()
    {
        *folder = dir.display().to_string();
    }
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if save {
        let dir = PathBuf::from(folder.trim());
        s.ed.prefs.paste_folder = Some(dir.clone());
        if *always {
            s.ed.prefs.paste_always = true;
        }
        let _ = s.ed.prefs.save(&s.ed.dirs);
        let job = &mut s.pastes[pos];
        job.folder = Some(dir);
        job.name = name.trim().to_string();
        return false;
    }
    if cancel || esc || !open {
        s.pastes.remove(pos);
        return false;
    }
    true
}
