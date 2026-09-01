use std::{borrow::Cow, path::Path};

use crate::detection::DetectedLanguage;

const LINE_WIDTH: u32 = 80;
const INDENT_WIDTH: u8 = 2;

#[derive(Debug, PartialEq, Eq)]
pub enum FormatOutcome {
    Formatted(String),
    Unchanged,
    Unsupported,
}

pub fn format_snippet(text: &str, language: DetectedLanguage) -> Result<FormatOutcome, String> {
    if text.trim().is_empty() {
        return Ok(FormatOutcome::Unchanged);
    }

    let formatted = match language {
        DetectedLanguage::JavaScript => format_typescript_family(text, "js")?,
        DetectedLanguage::TypeScript => format_typescript_family(text, "ts")?,
        DetectedLanguage::Tsx => format_typescript_family(text, "tsx")?,
        DetectedLanguage::Json => format_json(text)?,
        DetectedLanguage::JsonLines => format_json_lines(text)?,
        DetectedLanguage::Markdown => format_markdown(text)?,
        DetectedLanguage::Html => format_markup(text, markup_fmt::Language::Html)?,
        DetectedLanguage::Xml => format_markup(text, markup_fmt::Language::Xml)?,
        DetectedLanguage::Css => malva::format_text(
            text,
            malva::Syntax::Css,
            &malva::config::FormatOptions::default(),
        )
        .map_err(|error| error.to_string())?,
        DetectedLanguage::Yaml => {
            pretty_yaml::format_text(text, &pretty_yaml::config::FormatOptions::default())
                .map_err(|error| error.to_string())?
        }
        DetectedLanguage::Toml => format_toml(text)?,
        DetectedLanguage::Rust => {
            let syntax = syn::parse_file(text).map_err(|error| error.to_string())?;
            prettyplease::unparse(&syntax)
        }
        DetectedLanguage::Sql => sqlformat::format(
            text,
            &sqlformat::QueryParams::None,
            &sqlformat::FormatOptions {
                indent: sqlformat::Indent::Spaces(INDENT_WIDTH),
                ..Default::default()
            },
        ),
        _ => return Ok(FormatOutcome::Unsupported),
    };

    Ok(if formatted == text {
        FormatOutcome::Unchanged
    } else {
        FormatOutcome::Formatted(formatted)
    })
}

fn format_typescript_family(text: &str, extension: &str) -> Result<String, String> {
    let mut builder = dprint_plugin_typescript::configuration::ConfigurationBuilder::new();
    builder.line_width(LINE_WIDTH).indent_width(INDENT_WIDTH);
    let config = builder.build();
    let path = format!("snippet.{extension}");

    dprint_plugin_typescript::format_text(dprint_plugin_typescript::FormatTextOptions {
        path: Path::new(&path),
        extension: Some(extension),
        text: text.to_string(),
        config: &config,
        external_formatter: None,
    })
    .map(|formatted| formatted.unwrap_or_else(|| text.to_string()))
    .map_err(|error| error.to_string())
}

fn format_json(text: &str) -> Result<String, String> {
    let mut builder = dprint_plugin_json::configuration::ConfigurationBuilder::new();
    builder.line_width(LINE_WIDTH).indent_width(INDENT_WIDTH);
    let config = builder.build();

    dprint_plugin_json::format_text(Path::new("snippet.json"), text, &config)
        .map(|formatted| formatted.unwrap_or_else(|| text.to_string()))
        .map_err(|error| error.to_string())
}

fn format_json_lines(text: &str) -> Result<String, String> {
    let mut formatted = text
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .and_then(|value| serde_json::to_string(&value))
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    if text.ends_with('\n') {
        formatted.push('\n');
    }
    Ok(formatted)
}

fn format_markdown(text: &str) -> Result<String, String> {
    let mut builder = dprint_plugin_markdown::configuration::ConfigurationBuilder::new();
    builder.line_width(LINE_WIDTH);
    let config = builder.build();

    dprint_plugin_markdown::format_text(text, &config, |_, _, _| Ok(None))
        .map(|formatted| formatted.unwrap_or_else(|| text.to_string()))
        .map_err(|error| error.to_string())
}

fn format_markup(text: &str, language: markup_fmt::Language) -> Result<String, String> {
    markup_fmt::format_text(
        text,
        language,
        &markup_fmt::config::FormatOptions::default(),
        |code, _| Ok::<Cow<'_, str>, anyhow::Error>(Cow::Borrowed(code)),
    )
    .map_err(|error| error.to_string())
}

fn format_toml(text: &str) -> Result<String, String> {
    let mut builder = dprint_plugin_toml::configuration::ConfigurationBuilder::new();
    builder.line_width(LINE_WIDTH).indent_width(INDENT_WIDTH);
    let config = builder.build();

    dprint_plugin_toml::format_text(Path::new("snippet.toml"), text, &config)
        .map(|formatted| formatted.unwrap_or_else(|| text.to_string()))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formatted(text: &str, language: DetectedLanguage) -> String {
        match format_snippet(text, language).unwrap() {
            FormatOutcome::Formatted(text) => text,
            outcome => panic!("expected formatted text, got {outcome:?}"),
        }
    }

    #[test]
    fn formats_rust_snippet() {
        assert_eq!(
            formatted(
                "fn greet(name:&str){println!(\"Hello, {name}!\");}",
                DetectedLanguage::Rust,
            ),
            "fn greet(name: &str) {\n    println!(\"Hello, {name}!\");\n}\n"
        );
    }

    #[test]
    fn formats_prettier_family_snippets() {
        assert_eq!(
            formatted("const answer={value:42}", DetectedLanguage::JavaScript),
            "const answer = { value: 42 };\n"
        );
        assert_eq!(
            formatted("{\"items\":[1,2]}", DetectedLanguage::Json),
            "{ \"items\": [1, 2] }\n"
        );
        assert_eq!(
            formatted(
                "{ \"page\": 1 }\n{ \"page\": 2 }\n",
                DetectedLanguage::JsonLines,
            ),
            "{\"page\":1}\n{\"page\":2}\n"
        );
        assert_eq!(
            formatted("-   first\n- second", DetectedLanguage::Markdown),
            "- first\n- second\n"
        );
    }

    #[test]
    fn formats_markup_and_data_snippets() {
        assert_eq!(
            formatted("<div class=container></div>", DetectedLanguage::Html),
            "<div class=\"container\"></div>\n"
        );
        assert_eq!(
            formatted("a{color:red}", DetectedLanguage::Css),
            "a {\n  color: red;\n}\n"
        );
        assert_eq!(
            formatted("-  a\n-     b", DetectedLanguage::Yaml),
            "- a\n- b\n"
        );
    }

    #[test]
    fn invalid_or_unsupported_snippets_are_safe() {
        assert!(format_snippet("fn {", DetectedLanguage::Rust).is_err());
        assert_eq!(
            format_snippet("hello", DetectedLanguage::Text).unwrap(),
            FormatOutcome::Unsupported
        );
    }
}
