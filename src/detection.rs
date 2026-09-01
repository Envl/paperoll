const MAX_AUTO_DETECTION_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetectedLanguage {
    Bash,
    C,
    Cpp,
    CSharp,
    Css,
    Go,
    Html,
    Java,
    JavaScript,
    Json,
    JsonLines,
    Kotlin,
    Lua,
    Markdown,
    Php,
    Python,
    Ruby,
    Rust,
    Sql,
    Swift,
    Text,
    Toml,
    Tsx,
    TypeScript,
    Xml,
    Yaml,
    Zig,
}

impl DetectedLanguage {
    pub const ALL: [Self; 27] = [
        Self::Text,
        Self::Bash,
        Self::C,
        Self::Cpp,
        Self::CSharp,
        Self::Css,
        Self::Go,
        Self::Html,
        Self::Java,
        Self::JavaScript,
        Self::Json,
        Self::JsonLines,
        Self::Kotlin,
        Self::Lua,
        Self::Markdown,
        Self::Php,
        Self::Python,
        Self::Ruby,
        Self::Rust,
        Self::Sql,
        Self::Swift,
        Self::Toml,
        Self::Tsx,
        Self::TypeScript,
        Self::Xml,
        Self::Yaml,
        Self::Zig,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Css => "css",
            Self::Go => "go",
            Self::Html => "html",
            Self::Java => "java",
            Self::JavaScript => "javascript",
            Self::Json => "json",
            Self::JsonLines => "jsonl",
            Self::Kotlin => "kotlin",
            Self::Lua => "lua",
            Self::Markdown => "markdown",
            Self::Php => "php",
            Self::Python => "python",
            Self::Ruby => "ruby",
            Self::Rust => "rust",
            Self::Sql => "sql",
            Self::Swift => "swift",
            Self::Text => "text",
            Self::Toml => "toml",
            Self::Tsx => "tsx",
            Self::TypeScript => "typescript",
            Self::Xml => "xml",
            Self::Yaml => "yaml",
            Self::Zig => "zig",
        }
    }

    pub fn highlighter_name(self) -> &'static str {
        match self {
            Self::Xml => "html",
            language => language.name(),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bash => "Bash",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::CSharp => "C#",
            Self::Css => "CSS",
            Self::Go => "Go",
            Self::Html => "HTML",
            Self::Java => "Java",
            Self::JavaScript => "JavaScript",
            Self::Json => "JSON",
            Self::JsonLines => "JSON Lines",
            Self::Kotlin => "Kotlin",
            Self::Lua => "Lua",
            Self::Markdown => "Markdown",
            Self::Php => "PHP",
            Self::Python => "Python",
            Self::Ruby => "Ruby",
            Self::Rust => "Rust",
            Self::Sql => "SQL",
            Self::Swift => "Swift",
            Self::Text => "Plain text",
            Self::Toml => "TOML",
            Self::Tsx => "TSX",
            Self::TypeScript => "TypeScript",
            Self::Xml => "XML",
            Self::Yaml => "YAML",
            Self::Zig => "Zig",
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Bash => "sh",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "cs",
            Self::Css => "css",
            Self::Go => "go",
            Self::Html => "html",
            Self::Java => "java",
            Self::JavaScript => "js",
            Self::Json => "json",
            Self::JsonLines => "jsonl",
            Self::Kotlin => "kt",
            Self::Lua => "lua",
            Self::Markdown => "md",
            Self::Php => "php",
            Self::Python => "py",
            Self::Ruby => "rb",
            Self::Rust => "rs",
            Self::Sql => "sql",
            Self::Swift => "swift",
            Self::Text => "txt",
            Self::Toml => "toml",
            Self::Tsx => "tsx",
            Self::TypeScript => "ts",
            Self::Xml => "xml",
            Self::Yaml => "yaml",
            Self::Zig => "zig",
        }
    }

    pub fn from_file_extension(extension: &str) -> Option<Self> {
        let extension = extension.to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|language| language.file_extension() == extension)
            .or(match extension.as_str() {
                "bash" => Some(Self::Bash),
                "cc" | "cxx" => Some(Self::Cpp),
                "htm" => Some(Self::Html),
                "jsx" => Some(Self::JavaScript),
                "markdown" => Some(Self::Markdown),
                "rbw" => Some(Self::Ruby),
                "yml" => Some(Self::Yaml),
                _ => None,
            })
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.name() == name || language.label() == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageSelection {
    Auto,
    Explicit(DetectedLanguage),
}

impl LanguageSelection {
    pub fn from_persisted(value: &str) -> Self {
        match value {
            "auto" => Self::Auto,
            name => DetectedLanguage::from_name(name)
                .map(Self::Explicit)
                .unwrap_or(Self::Auto),
        }
    }

    pub fn persisted(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Explicit(language) => language.name(),
        }
    }

    pub fn from_file_extension(extension: Option<&str>) -> Self {
        extension
            .and_then(DetectedLanguage::from_file_extension)
            .map(Self::Explicit)
            .unwrap_or(Self::Auto)
    }

    pub fn file_extension(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Explicit(language) => Some(language.file_extension()),
        }
    }

    pub fn resolve(self, text: &str) -> DetectedLanguage {
        match self {
            Self::Auto => detect(text),
            Self::Explicit(language) => language,
        }
    }
}

pub fn detect(text: &str) -> DetectedLanguage {
    let mut inspected_end = text.len().min(MAX_AUTO_DETECTION_BYTES);
    while !text.is_char_boundary(inspected_end) {
        inspected_end -= 1;
    }
    let inspected = &text[..inspected_end];
    let inspected = if inspected_end < text.len() {
        inspected
            .rsplit_once('\n')
            .map_or(inspected, |(complete_lines, _)| complete_lines)
    } else {
        inspected
    };
    let trimmed = inspected.trim();
    if trimmed.is_empty() {
        return DetectedLanguage::Text;
    }

    let json_lines = trimmed.lines().collect::<Vec<_>>();
    if json_lines.len() >= 2
        && json_lines
            .iter()
            .all(|line| serde_json::from_str::<serde_json::Value>(line).is_ok())
    {
        return DetectedLanguage::JsonLines;
    }

    if ((trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']')))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    {
        return DetectedLanguage::Json;
    }

    // Structural Markdown should win over loose token heuristics such as
    // JavaScript's `let `, which also occurs at the end of words like
    // "bullet" in ordinary list items.
    let markdown_list_lines = lines_match(trimmed, is_markdown_list_item);
    if markdown_list_lines >= 2 {
        return DetectedLanguage::Markdown;
    }

    if trimmed.starts_with("#!")
        || contains_any(
            trimmed,
            &["#!/bin/bash", "#!/usr/bin/env bash", "set -euo pipefail"],
        )
    {
        return DetectedLanguage::Bash;
    }

    if trimmed.starts_with("<?xml") {
        return DetectedLanguage::Xml;
    }

    if trimmed.starts_with("<?php") || contains_any(trimmed, &["namespace App\\", "echo $"]) {
        return DetectedLanguage::Php;
    }

    if contains_any(
        trimmed,
        &[
            "fn main()",
            "let mut ",
            "impl ",
            "use std::",
            "pub struct ",
            "match ",
        ],
    ) {
        return DetectedLanguage::Rust;
    }

    if contains_any(
        trimmed,
        &[
            "import SwiftUI",
            "struct ContentView: View",
            "@State ",
            "guard let ",
            "func ",
        ],
    ) {
        return DetectedLanguage::Swift;
    }

    if contains_any(
        trimmed,
        &[
            "using System;",
            "Console.WriteLine(",
            "namespace ",
            "public class ",
        ],
    ) {
        return DetectedLanguage::CSharp;
    }

    if contains_any(
        trimmed,
        &[
            "fun main(",
            "import kotlin.",
            "data class ",
            "val mutableListOf",
        ],
    ) {
        return DetectedLanguage::Kotlin;
    }

    if contains_any(
        trimmed,
        &[
            "public static void main",
            "System.out.println(",
            "import java.",
        ],
    ) {
        return DetectedLanguage::Java;
    }

    if contains_any(
        trimmed,
        &[
            "#include <iostream>",
            "std::",
            "cout <<",
            "using namespace std",
        ],
    ) {
        return DetectedLanguage::Cpp;
    }

    if contains_any(
        trimmed,
        &["#include <stdio.h>", "printf(", "typedef struct "],
    ) || (trimmed.contains("int main(") && !trimmed.contains("std::"))
    {
        return DetectedLanguage::C;
    }

    if contains_any(
        trimmed,
        &["package main", "func main()", "fmt.Println(", "go func("],
    ) {
        return DetectedLanguage::Go;
    }

    if contains_any(trimmed, &["const std = @import", "pub fn main()", "@as("]) {
        return DetectedLanguage::Zig;
    }

    if contains_any(
        trimmed,
        &["local function ", "require(\"", "end --", "ipairs("],
    ) {
        return DetectedLanguage::Lua;
    }

    if contains_any(
        trimmed,
        &["def initialize", "puts ", "require '", "attr_reader "],
    ) {
        return DetectedLanguage::Ruby;
    }

    if contains_any(trimmed, &["import React", "React.FC", "return ("])
        && trimmed.contains('<')
        && trimmed.contains('>')
    {
        return DetectedLanguage::Tsx;
    }

    if contains_any(
        trimmed,
        &[
            "interface ",
            "type ",
            " as const",
            ": string",
            ": number",
            "satisfies ",
        ],
    ) {
        return DetectedLanguage::TypeScript;
    }

    if contains_any(
        trimmed,
        &[
            "const ",
            "let ",
            "function ",
            "=>",
            "console.log(",
            "import {",
        ],
    ) {
        return DetectedLanguage::JavaScript;
    }

    if contains_any(
        trimmed,
        &[
            "def ",
            "if __name__ ==",
            "from typing import",
            "import asyncio",
            "print(",
        ],
    ) {
        return DetectedLanguage::Python;
    }

    let upper = trimmed.to_ascii_uppercase();
    if contains_any(
        &upper,
        &[
            "SELECT ",
            "INSERT INTO ",
            "UPDATE ",
            "CREATE TABLE ",
            "DELETE FROM ",
        ],
    ) {
        return DetectedLanguage::Sql;
    }

    if trimmed.starts_with("<!DOCTYPE html")
        || (trimmed.starts_with('<') && contains_any(trimmed, &["</", "/>", ">\n"]))
    {
        return DetectedLanguage::Html;
    }

    if contains_any(
        trimmed,
        &["@media ", "display: flex", "color:", "font-family:"],
    ) && trimmed.contains('{')
    {
        return DetectedLanguage::Css;
    }

    if is_markdown_heading(trimmed)
        || lines_match(trimmed, is_markdown_heading) >= 1
        || contains_any(trimmed, &["```", "- [ ] ", "- [x] ", "]("])
    {
        return DetectedLanguage::Markdown;
    }

    if trimmed.starts_with("---\n")
        || lines_match(trimmed, |line| {
            line.contains(": ") && !line.trim_start().starts_with("http")
        }) >= 2
    {
        return DetectedLanguage::Yaml;
    }

    if lines_match(trimmed, |line| {
        let line = line.trim();
        (line.starts_with('[') && line.ends_with(']')) || line.contains(" = ")
    }) >= 2
    {
        return DetectedLanguage::Toml;
    }

    DetectedLanguage::Text
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn lines_match(text: &str, predicate: impl Fn(&str) -> bool) -> usize {
    text.lines().filter(|line| predicate(line)).count()
}

fn is_markdown_heading(line: &str) -> bool {
    let trimmed = line.trim_start();
    let hashes = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    (1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ')
}

fn is_markdown_list_item(line: &str) -> bool {
    let trimmed = line.trim_start();
    if ["- ", "* ", "+ ", "> "]
        .iter()
        .any(|marker| trimmed.starts_with(marker))
    {
        return true;
    }

    let digit_count = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    digit_count > 0
        && matches!(trimmed.as_bytes().get(digit_count), Some(b'.' | b')'))
        && trimmed.as_bytes().get(digit_count + 1) == Some(&b' ')
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_component::highlighter::{HighlightTheme, SyntaxHighlighter};
    use ropey::Rope;

    #[test]
    fn detects_json_before_javascript() {
        assert_eq!(detect(r#"{"paper": true}"#), DetectedLanguage::Json);
    }

    #[test]
    fn detects_json_lines_and_its_file_extension() {
        assert_eq!(
            detect("{\"page\":1}\n{\"page\":2}"),
            DetectedLanguage::JsonLines
        );
        assert_eq!(
            DetectedLanguage::from_file_extension("JSONL"),
            Some(DetectedLanguage::JsonLines)
        );

        let large_json_lines = (0..10_000)
            .map(|index| format!(r#"{{"index":{index}}}"#))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(detect(&large_json_lines), DetectedLanguage::JsonLines);
    }

    #[test]
    fn detects_representative_languages() {
        assert_eq!(
            detect("fn main() { println!(\"hi\"); }"),
            DetectedLanguage::Rust
        );
        assert_eq!(
            detect("# Notes\n\n- one\n- two"),
            DetectedLanguage::Markdown
        );
        assert_eq!(detect("SELECT * FROM pages"), DetectedLanguage::Sql);
        assert_eq!(
            detect("def hello():\n    print('hi')"),
            DetectedLanguage::Python
        );
    }

    #[test]
    fn ordinary_prose_stays_plain_text() {
        assert_eq!(detect("Call Morgan after lunch."), DetectedLanguage::Text);
    }

    #[test]
    fn explicit_language_overrides_detection_and_round_trips() {
        let selection = LanguageSelection::Explicit(DetectedLanguage::Cpp);
        assert_eq!(selection.resolve("ordinary prose"), DetectedLanguage::Cpp);
        assert_eq!(
            LanguageSelection::from_persisted(selection.persisted()),
            selection
        );
        assert_eq!(
            LanguageSelection::from_persisted("unknown"),
            LanguageSelection::Auto
        );
    }

    #[test]
    fn file_extensions_round_trip_language_overrides() {
        for language in DetectedLanguage::ALL {
            assert_eq!(
                DetectedLanguage::from_file_extension(language.file_extension()),
                Some(language)
            );
        }
        assert_eq!(
            LanguageSelection::from_file_extension(None),
            LanguageSelection::Auto
        );
        assert_eq!(
            LanguageSelection::from_file_extension(Some("RS")),
            LanguageSelection::Explicit(DetectedLanguage::Rust)
        );
        assert_eq!(
            LanguageSelection::from_file_extension(Some("unknown")),
            LanguageSelection::Auto
        );
    }

    #[test]
    fn detects_additional_mainstream_languages() {
        assert_eq!(
            detect("#include <iostream>\nstd::cout << 1;"),
            DetectedLanguage::Cpp
        );
        assert_eq!(
            detect("public static void main(String[] args) {}"),
            DetectedLanguage::Java
        );
        assert_eq!(
            detect("<?xml version=\"1.0\"?><note />"),
            DetectedLanguage::Xml
        );
    }

    #[test]
    fn detects_markdown_lists_without_a_heading() {
        assert_eq!(
            detect("- asdy\n- asdgsd\n- asdg\n1. 232\n2. asdgs"),
            DetectedLanguage::Markdown
        );
        assert_eq!(detect("- bullet 1\n- bullet 2"), DetectedLanguage::Markdown);
        assert_eq!(detect("1) first\n2) second"), DetectedLanguage::Markdown);
    }

    #[test]
    fn rust_highlighter_produces_colored_spans_in_both_themes() {
        let source = "fn main() { let message = \"Paperoll\"; }";
        let text = Rope::from_str(source);
        let mut highlighter = SyntaxHighlighter::new("rust");

        assert!(highlighter.update(None, &text, None));
        for theme_source in [
            include_str!("../resources/highlight-theme.json"),
            include_str!("../resources/highlight-theme-dark.json"),
        ] {
            let theme: HighlightTheme = serde_json::from_str(theme_source).unwrap();
            let styles = highlighter.styles(&(0..source.len()), &theme);

            assert!(!styles.is_empty());
            assert!(
                styles
                    .iter()
                    .filter_map(|(_, style)| style.color)
                    .any(|color| color.s > 0.2)
            );
        }
    }
}
