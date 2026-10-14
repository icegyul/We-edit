//! Where widgets overlap, egui gives the click to the one added last. The timeline track headers
//! have a whole-area widget (the menu of the empty header space): it must be added BEFORE the
//! patch, target, lock, eye, mute and solo buttons, or it takes every click from them (the V2, V3
//! and A2 target buttons did not switch on).

use egui::{Context, Event, PointerButton, Pos2, RawInput, Rect, Sense, pos2, vec2};

#[derive(Clone, Copy, PartialEq)]
enum Area {
    None,
    BeforeButton,
    AfterButton,
}

fn frame(ctx: &Context, events: Vec<Event>, area: Area) -> (bool, bool) {
    let mut input = RawInput::default();
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0)));
    input.events = events;
    let (mut button, mut whole) = (false, false);
    let out = ctx.run_ui(input, |ui| {
        let r = ui.max_rect();
        let cell = Rect::from_center_size(pos2(r.min.x + 42.0, r.min.y + 20.0), vec2(28.0, 18.0));
        let whole_rect = Rect::from_min_size(r.min, vec2(150.0, 200.0));
        if area == Area::BeforeButton {
            whole = ui.interact(whole_rect, ui.id().with("hdr-empty"), Sense::click()).clicked();
        }
        button = ui.interact(cell, ui.id().with("target"), Sense::click()).clicked();
        if area == Area::AfterButton {
            whole = ui.interact(whole_rect, ui.id().with("hdr-empty"), Sense::click()).clicked();
        }
    });
    out.drop_without_applying_deltas();
    (button, whole)
}

/// A click on the button: (the button was clicked, the whole-area widget was clicked).
fn click(area: Area) -> (bool, bool) {
    let ctx = Context::default();
    let at = pos2(42.0, 20.0);
    let press = |pressed| Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() };
    frame(&ctx, vec![], area);
    frame(&ctx, vec![Event::PointerMoved(at)], area);
    frame(&ctx, vec![press(true)], area);
    frame(&ctx, vec![press(false)], area)
}

#[test]
fn the_button_gets_the_click_when_the_whole_area_is_added_first() {
    assert_eq!(click(Area::None), (true, false), "the button works alone");
    assert_eq!(click(Area::BeforeButton), (true, false), "the whole-area widget must not take the button's click");
}

#[test]
fn a_whole_area_widget_added_last_takes_the_click_which_is_why_it_goes_first() {
    // the behaviour the order above works around: if egui ever changes this, the order is
    // merely no longer needed
    assert_eq!(click(Area::AfterButton), (false, true));
}
