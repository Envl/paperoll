# Paperoll editor sizing patch

Source: https://github.com/longbridge/gpui-component/tree/a8d1d2650493bbb01eb39412dea823173e1ea94c/crates/base
License: Apache-2.0 (LICENSE-APACHE).

This is the pinned upstream gpui-base source. Cargo workspace dependencies are
expanded into a standalone manifest; unused bench configuration is omitted.

Local changes in src/input/base:
- state.rs adds opt-in grow_to_content and uses intrinsic, non-shrinking layout.
- element.rs measures the entire display map at the actual available width,
  using the same font, line-number gutter, right margin and wrapping indent as
  painting. Height has no row cap. Existing editor padding and search/replace
  panels participate in normal layout outside the measured text element.

The code-editor mode remains intact, preserving highlighting and language features.
When updating the dependency, port these changes or remove this patch if upstream
provides equivalent intrinsic code-editor sizing. Do not restore estimated rows
or a fixed editor height in Paperoll.

## Viewport virtualization

Full-height editors derive visible buffer lines from the ancestor content mask,
not the editor's full bounds. Within long buffer lines, only visible wrapped rows
(with one row of overscan), the indentation row, and the caret row are shaped.
Skipped rows retain byte lengths using ShapedLine::with_len, preserving hit-testing
and selection offsets. Text and gutter painting skip offscreen wrapped rows.

Pure regression tests (no window or UI interaction):
`./scripts/cargo.sh test -p gpui-base --lib virtualization`

Initial wrapping and edits/resizes still measure the full text to preserve exact
roll height. Scrolling reuses those wrap measurements.
