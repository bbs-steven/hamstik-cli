// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Output rendering: human tables, machine JSON, and quiet/verbose handling.
//!
//! Contract (SPEC §39): success content goes to stdout, diagnostics/warnings go
//! to stderr, and `--json` output never contains ANSI codes. Failure rendering
//! switches between a human message and the stable JSON envelope.

use std::io::{self, Write};

use serde_json::Value;

use crate::error::CliError;

/// The three mutually-exclusive output modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Human-readable tables and detail views.
    Human,
    /// Stable JSON envelopes.
    Json,
    /// Only essential identifiers.
    Quiet,
}

/// Buffered output sink with mode awareness.
pub struct Output {
    mode: Mode,
    verbose: bool,
    out: Box<dyn Write>,
    err: Box<dyn Write>,
}

impl Output {
    /// Wraps the two streams with a mode and verbosity flag.
    #[must_use]
    pub fn new(mode: Mode, verbose: bool, out: Box<dyn Write>, err: Box<dyn Write>) -> Self {
        Self {
            mode,
            verbose,
            out,
            err,
        }
    }

    /// The active output mode.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// True when machine JSON envelopes are expected on stdout.
    #[must_use]
    pub fn is_json(&self) -> bool {
        self.mode == Mode::Json
    }

    /// True when only essential identifiers should be printed.
    #[must_use]
    pub fn is_quiet(&self) -> bool {
        self.mode == Mode::Quiet
    }

    /// Writes a pretty-printed JSON value to stdout.
    pub fn json(&mut self, value: &Value) -> io::Result<()> {
        let rendered = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
        writeln!(self.out, "{rendered}")
    }

    /// Writes a human content line to stdout.
    pub fn line(&mut self, text: &str) -> io::Result<()> {
        writeln!(self.out, "{text}")
    }

    /// Writes raw text directly to stdout (used for completion scripts).
    pub fn raw(&mut self, text: &str) -> io::Result<()> {
        write!(self.out, "{text}")
    }

    /// Prints nothing when quiet, otherwise a human line to stdout.
    pub fn human(&mut self, text: &str) -> io::Result<()> {
        if self.mode == Mode::Quiet {
            Ok(())
        } else {
            self.line(text)
        }
    }

    /// Writes a table to stdout (human mode).
    pub fn table(&mut self, headers: &[&str], rows: &[Vec<String>]) -> io::Result<()> {
        let rendered = render_table(headers, rows);
        if rendered.is_empty() {
            Ok(())
        } else {
            write!(self.out, "{rendered}")
        }
    }

    /// Diagnostics to stderr, only when verbose.
    pub fn verbose(&mut self, text: &str) {
        if self.verbose {
            let _ = writeln!(self.err, "[verbose] {text}");
        }
    }

    /// Warnings/progress to stderr.
    pub fn warn(&mut self, text: &str) {
        let _ = writeln!(self.err, "{text}");
    }

    /// Renders a failure: JSON envelope to stderr in `--json`, human otherwise.
    pub fn error(&mut self, error: &CliError) -> io::Result<()> {
        if self.is_json() {
            let rendered =
                serde_json::to_string_pretty(&error.to_json()).map_err(io::Error::other)?;
            writeln!(self.err, "{rendered}")
        } else {
            writeln!(self.err, "error: {}", error.message)
        }
    }
}

/// Renders a left-aligned, width-padded text table.
///
/// Column widths are derived from the widest cell by *visible* width, so cells
/// may contain ANSI SGR sequences (e.g. color swatches) without breaking
/// alignment. Empty input produces an empty string so callers can suppress the
/// whole table.
#[must_use]
pub fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    if headers.is_empty() {
        return String::new();
    }
    let columns = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| visible_width(h)).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate().take(columns) {
            widths[index] = widths[index].max(visible_width(cell));
        }
    }

    let mut out = String::new();
    write_row(&mut out, headers, &widths);
    let separator: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    let separator: Vec<&str> = separator.iter().map(String::as_str).collect();
    write_row(&mut out, &separator, &widths);
    for row in rows {
        let cells: Vec<&str> = (0..columns)
            .map(|index| row.get(index).map(String::as_str).unwrap_or(""))
            .collect();
        write_row(&mut out, &cells, &widths);
    }
    out
}

/// The number of display columns a cell occupies, skipping ANSI SGR escapes.
///
/// Block-element swatch fills (U+2588) render as one terminal column each in
/// every terminal the CLI targets, so a plain `char` count outside escape
/// sequences is the width.
#[must_use]
pub fn visible_width(text: &str) -> usize {
    let mut count = 0;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip an entire CSI sequence (ESC [ ... final byte).
            if chars.next() == Some('[') {
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            count += 1;
        }
    }
    count
}

fn write_row(out: &mut String, cells: &[&str], widths: &[usize]) {
    let mut line = String::new();
    let last = cells.len().saturating_sub(1);
    for (index, cell) in cells.iter().enumerate() {
        line.push_str(cell);
        if index != last {
            let pad = widths[index].saturating_sub(visible_width(cell)) + 2;
            line.push_str(&" ".repeat(pad));
        }
    }
    out.push_str(line.trim_end());
    out.push('\n');
}

#[cfg(test)]
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::exit;

    // A cheap shared in-memory writer for capturing output in tests.
    #[derive(Clone, Default)]
    struct Sink(Rc<RefCell<Vec<u8>>>);

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn text(sink: &Sink) -> String {
        String::from_utf8(sink.0.borrow().clone()).unwrap()
    }

    #[test]
    fn renders_padded_table() {
        let out = render_table(
            &["KEY", "TITLE"],
            &[
                vec!["HAM-1".to_string(), "Short".to_string()],
                vec!["HAM-10".into(), "A longer title".into()],
            ],
        );
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "KEY     TITLE");
        assert_eq!(lines[1], "------  --------------");
        assert_eq!(lines[2], "HAM-1   Short");
        assert_eq!(lines[3], "HAM-10  A longer title");
    }

    #[test]
    fn empty_headers_render_empty() {
        assert_eq!(render_table(&[], &[vec!["x".to_string()]]), "");
    }

    #[test]
    fn json_output_captured() {
        let sink = Sink::default();
        let mut out = Output::new(
            Mode::Json,
            false,
            Box::new(sink.clone()),
            Box::new(Sink::default()),
        );
        out.json(&serde_json::json!({"a": 1})).unwrap();
        assert_eq!(text(&sink).trim(), "{\n  \"a\": 1\n}");
    }

    #[test]
    fn quiet_suppresses_human_lines() {
        let sink = Sink::default();
        let mut out = Output::new(
            Mode::Quiet,
            false,
            Box::new(sink.clone()),
            Box::new(Sink::default()),
        );
        out.human("hidden").unwrap();
        assert!(text(&sink).is_empty());
    }

    #[test]
    fn error_json_envelope_written_to_stderr() {
        let err_sink = Sink::default();
        let mut out = Output::new(
            Mode::Json,
            false,
            Box::new(Sink::default()),
            Box::new(err_sink.clone()),
        );
        let err = CliError::from_api(hamstik_api_client::error::ApiError {
            status: 412,
            code: "REVISION_CONFLICT".into(),
            message: "changed".into(),
            request_id: Some("r1".into()),
            field_errors: Default::default(),
            retry_after: None,
        });
        out.error(&err).unwrap();
        let rendered: Value = serde_json::from_str(&text(&err_sink)).unwrap();
        assert_eq!(rendered["error"]["code"], "REVISION_CONFLICT");
        assert_eq!(err.exit_code(), exit::CONFLICT);
    }

    #[test]
    fn verbose_writes_only_when_enabled() {
        let err_sink = Sink::default();
        let mut out = Output::new(
            Mode::Human,
            true,
            Box::new(Sink::default()),
            Box::new(err_sink.clone()),
        );
        out.verbose("note");
        assert!(text(&err_sink).contains("note"));
    }
}
