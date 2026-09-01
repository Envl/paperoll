use std::{ops::Range, rc::Rc};

use gpui::{Context, HighlightStyle, SharedString, Window};
use gpui_component::highlighter::SyntaxHighlighter;
use gpui_component::{
    ActiveTheme as _,
    input::{
        EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter,
        InputHighlighterFactory, Rope, RopeExt as _, TextDecoration, TextDecorationCollection,
    },
};

const MAX_BUILT_IN_HIGHLIGHT_LINE_LENGTH: usize = 10_000;

pub fn factory(long_line_decorations: TextDecorationCollection) -> InputHighlighterFactory {
    Rc::new(move |language| {
        Some(if language == "jsonl" {
            Box::new(JsonLinesHighlighter::new(long_line_decorations.clone()))
                as Box<dyn InputHighlighter>
        } else {
            Box::new(DefaultSyntaxHighlighter::new(language)) as Box<dyn InputHighlighter>
        })
    })
}

struct DefaultSyntaxHighlighter(SyntaxHighlighter);

impl DefaultSyntaxHighlighter {
    fn new(language: &str) -> Self {
        Self(SyntaxHighlighter::new(language))
    }
}

impl InputHighlighter for DefaultSyntaxHighlighter {
    fn language(&self) -> SharedString {
        self.0.language().clone()
    }

    fn update(
        &mut self,
        _: Option<InputEdit>,
        text: &Rope,
        _: bool,
        _: &mut Window,
        _: &mut Context<EditorState>,
    ) {
        self.0.update(None, text, None);
    }

    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        self.0.styles(range, resolver)
    }

    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

struct JsonLinesHighlighter {
    text: Rope,
    long_line_decorations: TextDecorationCollection,
    decorated_line_ranges: Vec<Range<usize>>,
}

impl JsonLinesHighlighter {
    fn new(long_line_decorations: TextDecorationCollection) -> Self {
        Self {
            text: Rope::new(),
            long_line_decorations,
            decorated_line_ranges: Vec::new(),
        }
    }
}

impl InputHighlighter for JsonLinesHighlighter {
    fn language(&self) -> SharedString {
        "jsonl".into()
    }

    fn update(
        &mut self,
        edit: Option<InputEdit>,
        text: &Rope,
        _: bool,
        _: &mut Window,
        cx: &mut Context<EditorState>,
    ) {
        let should_rebuild_long_lines = edit.is_none()
            || edit.is_some_and(|edit| {
                let changed_old_range = edit.start_byte..edit.old_end_byte.max(edit.start_byte + 1);
                let touches_decorated_line = self.decorated_line_ranges.iter().any(|range| {
                    range.start < changed_old_range.end && changed_old_range.start < range.end
                });
                let changed_row = text.offset_to_point(edit.start_byte.min(text.len())).row;
                touches_decorated_line
                    || text.slice_line(changed_row).len() > MAX_BUILT_IN_HIGHLIGHT_LINE_LENGTH
            });
        self.text = text.clone();
        if should_rebuild_long_lines {
            let theme = cx.theme().highlight_theme.clone();
            let (decorations, ranges) = long_line_decorations(text, theme.as_ref());
            self.decorated_line_ranges = ranges;
            let collection = self.long_line_decorations.clone();
            cx.defer(move |cx| collection.set(decorations, cx));
        }
    }

    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        if range.is_empty() || self.text.len() == 0 {
            return Vec::new();
        }

        let start = range.start.min(self.text.len());
        let end = range.end.min(self.text.len());
        let start_row = self.text.offset_to_point(start).row;
        let end_row = self.text.offset_to_point(end.saturating_sub(1)).row;
        let expanded_start = self.text.line_start_offset(start_row);
        let expanded_end = self.text.line_end_offset(end_row).min(self.text.len());
        let source = self.text.slice(expanded_start..expanded_end).to_string();
        let tokens = json_tokens(&source, expanded_start);
        cover_range_with_styles(start..end, tokens, resolver)
    }

    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

fn long_line_decorations(
    text: &Rope,
    resolver: &dyn HighlightStyleResolver,
) -> (Vec<TextDecoration>, Vec<Range<usize>>) {
    let mut decorations = Vec::new();
    let mut decorated_line_ranges = Vec::new();
    for row in 0..text.lines_len() {
        let line = text.slice_line(row);
        if line.len() <= MAX_BUILT_IN_HIGHLIGHT_LINE_LENGTH {
            continue;
        }
        let start = text.line_start_offset(row);
        let end = start + line.len();
        let source = line.to_string();
        decorations.extend(
            json_tokens(&source, start)
                .into_iter()
                .filter_map(|(range, name)| {
                    resolver
                        .style(name)
                        .map(|style| TextDecoration::new(range, style))
                }),
        );
        decorated_line_ranges.push(start..end);
    }
    (decorations, decorated_line_ranges)
}

fn cover_range_with_styles(
    range: Range<usize>,
    tokens: Vec<(Range<usize>, &'static str)>,
    resolver: &dyn HighlightStyleResolver,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut result = Vec::new();
    let mut cursor = range.start;
    for (token_range, name) in tokens {
        let token_start = token_range.start.max(range.start);
        let token_end = token_range.end.min(range.end);
        if token_start >= token_end {
            continue;
        }
        if cursor < token_start {
            result.push((cursor..token_start, HighlightStyle::default()));
        }
        result.push((
            token_start..token_end,
            resolver.style(name).unwrap_or_default(),
        ));
        cursor = token_end;
    }
    if cursor < range.end {
        result.push((cursor..range.end, HighlightStyle::default()));
    }
    result
}

fn json_tokens(source: &str, base: usize) -> Vec<(Range<usize>, &'static str)> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut ix = 0;
    while ix < bytes.len() {
        match bytes[ix] {
            b'"' => {
                let start = ix;
                ix += 1;
                while ix < bytes.len() {
                    match bytes[ix] {
                        b'\\' => ix = (ix + 2).min(bytes.len()),
                        b'"' => {
                            ix += 1;
                            break;
                        }
                        _ => ix += 1,
                    }
                }
                let mut next = ix;
                while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                    next += 1;
                }
                let style = if bytes.get(next) == Some(&b':') {
                    "property"
                } else {
                    "string"
                };
                tokens.push((base + start..base + ix, style));
            }
            b'-' | b'0'..=b'9' => {
                let start = ix;
                ix += 1;
                while ix < bytes.len()
                    && matches!(bytes[ix], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    ix += 1;
                }
                tokens.push((base + start..base + ix, "number"));
            }
            b't' if bytes[ix..].starts_with(b"true") => {
                tokens.push((base + ix..base + ix + 4, "boolean"));
                ix += 4;
            }
            b'f' if bytes[ix..].starts_with(b"false") => {
                tokens.push((base + ix..base + ix + 5, "boolean"));
                ix += 5;
            }
            b'n' if bytes[ix..].starts_with(b"null") => {
                tokens.push((base + ix..base + ix + 4, "constant"));
                ix += 4;
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                tokens.push((base + ix..base + ix + 1, "punctuation"));
                ix += 1;
            }
            _ => ix += 1,
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use gpui::HighlightStyle;
    use gpui_component::input::{HighlightStyleResolver, Rope};

    use super::{json_tokens, long_line_decorations};

    struct TestResolver;

    impl HighlightStyleResolver for TestResolver {
        fn style(&self, _: &str) -> Option<HighlightStyle> {
            Some(HighlightStyle::default())
        }
    }

    #[test]
    fn highlights_each_json_line_independently() {
        let source = "{\"name\":\"one\",\"ok\":true}\n{\"name\":\"two\",\"n\":2}\n";
        let tokens = json_tokens(source, 0);
        assert_eq!(
            tokens
                .iter()
                .filter(|(_, style)| *style == "property")
                .count(),
            4
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|(_, style)| *style == "string")
                .count(),
            2
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|(_, style)| *style == "boolean")
                .count(),
            1
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|(_, style)| *style == "number")
                .count(),
            1
        );
    }

    #[test]
    fn highlights_the_tail_of_a_very_large_nested_record() {
        let long_description = format!(
            "{}\\nFilesystem sandboxing defines which files can be read or written. \\
             `sandbox_mode` is enabled.\\n{}",
            "nested escaped content ".repeat(700),
            "additionalProperties:false; ".repeat(350),
        );
        let first = serde_json::json!({
            "timestamp": "2026-05-12T14:38:59.986Z",
            "type": "session_meta",
            "payload": {
                "tools": [{
                    "name": "read_thread_terminal",
                    "description": long_description,
                    "inputSchema": { "type": "object", "properties": {} }
                }],
                "git": {
                    "commit_hash": "a0969263044b9fc465922de79a2ccb616c0e52bf",
                    "branch": "main"
                }
            }
        })
        .to_string();
        let second = serde_json::json!({
            "timestamp": "2026-05-12T14:38:59.987Z",
            "type": "event_msg",
            "payload": { "type": "task_started", "context_window": 258400 }
        })
        .to_string();
        let source = format!("{first}\n{second}\n");
        assert!(source.len() > 25_000);

        let tokens = json_tokens(&source, 0);
        let commit_hash = source.rfind("\"commit_hash\"").unwrap();
        assert!(tokens.iter().any(|(range, style)| {
            *style == "property" && range.start == commit_hash && range.end > commit_hash
        }));
        let second_timestamp = source.rfind("\"timestamp\"").unwrap();
        assert!(tokens.iter().any(|(range, style)| {
            *style == "property" && range.start == second_timestamp && range.end > second_timestamp
        }));
    }

    #[test]
    fn decorates_records_skipped_by_gpui_long_line_guard() {
        let source = format!(
            "{{\"payload\":\"{}\",\"ok\":true}}\n{{\"short\":1}}\n",
            "x".repeat(10_050)
        );
        let rope = Rope::from(source.as_str());
        let (decorations, ranges) = long_line_decorations(&rope, &TestResolver);

        assert_eq!(ranges.len(), 1);
        assert!(ranges[0].end - ranges[0].start > 10_000);
        assert!(!decorations.is_empty());
        assert!(decorations.iter().all(|decoration| {
            decoration.range.start >= ranges[0].start && decoration.range.end <= ranges[0].end
        }));
    }
}
