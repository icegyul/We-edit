//! The AI panel lays out its chat bottom-up (buttons, input, then the scrolling chat above them).
//! Whatever the size of the panel, the chat must have room on screen and its lines must be inside
//! it, or the answers (and the user's own question) are not visible at all.

use egui::{Align, Context, Frame, Label, Layout, Margin, Pos2, RawInput, Rect, ScrollArea, TextEdit, vec2};

/// The same arrangement as `ai_panel::show`: returns the height the chat got and how many of its
/// lines have their top inside the visible area.
fn chat_with(width: f32, height: f32, lines: usize, top_down_inside: bool) -> (f32, usize, bool) {
    let ctx = Context::default();
    let mut input = RawInput::default();
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(width, height)));
    let mut input_text = String::new();
    let (mut chat_h, mut visible, mut last_visible) = (0.0, 0, false);
    // two frames: the first one measures
    for _ in 0..2 {
        visible = 0;
        last_visible = false;
        let out = ctx.run_ui(input.clone(), |ui| {
            Frame::new().inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.button("Claude");
                    ui.button("ChatGPT");
                    ui.label("● someone@example.com");
                });
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.button("보내기");
                        ui.button("중지");
                        ui.button("새 대화");
                    });
                    ui.add(TextEdit::multiline(&mut input_text).desired_rows(3).desired_width(f32::INFINITY));
                    ui.separator();
                    let out = ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                        let clip = ui.clip_rect();
                        let mut add_lines = |ui: &mut egui::Ui| {
                            for i in 0..lines {
                                let r = ui.add(Label::new(format!("AI: 줄 {i} 가나다라마바사 가나다라마바사 가나다라마바사")).wrap());
                                if clip.contains(r.rect.center_top()) {
                                    visible += 1;
                                    last_visible = i + 1 == lines;
                                }
                            }
                        };
                        if top_down_inside {
                            ui.with_layout(Layout::top_down(Align::Min), |ui| add_lines(ui));
                        } else {
                            add_lines(ui);
                        }
                    });
                    chat_h = out.inner_rect.height();
                });
            });
        });
        out.drop_without_applying_deltas();
    }
    (chat_h, visible, last_visible)
}

fn chat(width: f32, height: f32, lines: usize) -> (f32, usize, bool) {
    chat_with(width, height, lines, true)
}

/// What the panel did before: lines bottom-up inside the scroll area. Kept so the test says why the
/// lines are laid top-down: with enough lines nothing of the chat is visible.
#[test]
fn bottom_up_lines_in_the_scroll_area_leave_the_chat_empty() {
    let (_, visible, _) = chat_with(254.0, 444.0, 50, false);
    assert_eq!(visible, 0, "egui changed: the top-down layout inside the scroll area may no longer be needed");
}

#[test]
fn the_chat_has_room_in_the_panel_sizes_people_use() {
    // the user's AI column was about 254 x 444; also a wide and a short panel
    for (w, h) in [(254.0, 444.0), (400.0, 300.0), (600.0, 200.0)] {
        let (chat_h, visible, _) = chat(w, h, 3);
        assert!(chat_h > 40.0, "{w}x{h}: the chat got {chat_h} px");
        assert!(visible >= 1, "{w}x{h}: none of the lines is visible (chat height {chat_h})");
    }
}

#[test]
fn a_long_chat_shows_its_newest_lines() {
    for lines in [14, 20, 75, 200] {
        let (chat_h, visible, last_visible) = chat(254.0, 444.0, lines);
        assert!(chat_h > 100.0);
        assert!(visible >= 3, "{lines} lines: only {visible} visible in {chat_h} px");
        assert!(last_visible, "{lines} lines: the newest line is not visible");
    }
}
