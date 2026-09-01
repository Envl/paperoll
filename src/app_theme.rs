use std::sync::Arc;

use gpui::{App, Window};
use gpui_component::{Theme, highlighter::HighlightTheme};

pub fn sync_with_system(window: Option<&mut Window>, cx: &mut App) {
    Theme::sync_system_appearance(window, cx);
    let source = if Theme::global(cx).is_dark() {
        include_str!("../resources/highlight-theme-dark.json")
    } else {
        include_str!("../resources/highlight-theme.json")
    };
    let highlight_theme: HighlightTheme =
        serde_json::from_str(source).expect("invalid Paperoll highlight theme");
    Theme::global_mut(cx).highlight_theme = Arc::new(highlight_theme);
    Theme::sync_base(cx);
}
