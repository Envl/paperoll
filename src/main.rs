mod app_theme;
mod detection;
mod formatting;
mod model;
mod persistence;
mod tab_drag;
mod updater;
mod workspace;

use std::time::Duration;

use gpui::{
    App, AppContext as _, Bounds, KeyBinding, WindowBounds, WindowKind, WindowOptions, actions, px,
    size,
};
use gpui_component::{Root, TitleBar};
use gpui_component_assets::Assets;
use workspace::{
    CloseRoll, DeleteEmptySnippet, DeleteEmptySnippetWithCommand, FormatSnippet, MoveToNextSnippet,
    MoveToPreviousSnippet, NewRoll, NewSnippet, NextRoll, Paperoll, PreviousRoll,
};

actions!(paperoll_app, [Quit]);

fn main() {
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            gpui_component::init(cx);
            app_theme::sync_with_system(None, cx);

            cx.on_action(|_: &Quit, cx| cx.quit());

            // Replace GPUI Component's secondary-enter editor binding. The stock
            // binding inserts a newline before emitting its event; Paperoll owns
            // this command and creates a sibling page instead.
            // Backspace similarly passes through Paperoll first so an already-empty
            // extra page is removed without deleting it on the keystroke that empties it.
            cx.bind_keys([
                KeyBinding::new("secondary-enter", NewSnippet, Some("Input")),
                KeyBinding::new("backspace", DeleteEmptySnippet, Some("Input")),
                KeyBinding::new("shift-backspace", DeleteEmptySnippet, Some("Input")),
                KeyBinding::new(
                    "cmd-backspace",
                    DeleteEmptySnippetWithCommand,
                    Some("Input"),
                ),
                KeyBinding::new("alt-shift-f", FormatSnippet, Some("Input")),
                KeyBinding::new("secondary-t", NewRoll, None),
                KeyBinding::new("secondary-w", CloseRoll, None),
                KeyBinding::new("secondary-q", Quit, None),
                KeyBinding::new("ctrl-tab", NextRoll, None),
                KeyBinding::new("ctrl-shift-tab", PreviousRoll, None),
                KeyBinding::new("up", MoveToPreviousSnippet, Some("Input")),
                KeyBinding::new("down", MoveToNextSnippet, Some("Input")),
            ]);

            open_main_window(cx);
        });
}

fn open_main_window(cx: &mut App) {
    let bounds = Bounds::centered(None, size(px(980.), px(760.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(560.), px(420.))),
        inactive_frame_interval: Some(Duration::from_millis(500)),
        kind: WindowKind::Normal,
        ..TitleBar::window_options()
    };

    cx.spawn(async move |cx| {
        let window = cx
            .open_window(options, |window, cx| {
                let paperoll = cx.new(|cx| Paperoll::new(window, cx));
                cx.new(|cx| Root::new(paperoll, window, cx))
            })
            .expect("failed to open Paperoll window");

        window.update(cx, |_, window, _| {
            window.set_window_title("Paperoll");
            window.activate_window();
        })?;

        Ok::<_, anyhow::Error>(())
    })
    .detach();
}
