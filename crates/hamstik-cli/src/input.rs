// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Interactive input seam and long-text resolution.
//!
//! Commands gate *whether* prompting is allowed (SPEC §41); this module only
//! performs the reads. File arguments accept `-` to mean stdin (SPEC §46).

use std::io::{self, BufRead, Read, Write};

/// Reads a line or a secret from the user.
pub trait Prompt {
    fn read_line(&mut self, prompt: &str) -> io::Result<String>;
    fn read_secret(&mut self, prompt: &str) -> io::Result<String>;
}

/// Real terminal prompts (visible line + hidden secret).
pub struct TerminalInput;

impl Prompt for TerminalInput {
    fn read_line(&mut self, prompt: &str) -> io::Result<String> {
        print!("{prompt}");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }

    fn read_secret(&mut self, prompt: &str) -> io::Result<String> {
        let secret = rpassword::prompt_password(prompt)?;
        Ok(secret)
    }
}

/// Resolves long text from an inline value, a file, or stdin.
///
/// `file` of `-` reads all of stdin. Precedence is inline value, then file.
pub fn resolve_text(
    inline: Option<String>,
    file: Option<&str>,
    stdin: &mut dyn Read,
) -> io::Result<Option<String>> {
    if let Some(text) = inline {
        return Ok(Some(text));
    }
    match file {
        Some("-") => {
            let mut buffer = String::new();
            stdin.read_to_string(&mut buffer)?;
            Ok(Some(buffer))
        }
        Some(path) => std::fs::read_to_string(path).map(Some),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn inline_wins() {
        let mut stdin = Cursor::new(b"ignored".to_vec());
        let value = resolve_text(Some("hi".into()), Some("/nope"), &mut stdin).unwrap();
        assert_eq!(value.as_deref(), Some("hi"));
    }

    #[test]
    fn dash_reads_stdin() {
        let mut stdin = Cursor::new(b"from stdin".to_vec());
        let value = resolve_text(None, Some("-"), &mut stdin).unwrap();
        assert_eq!(value.as_deref(), Some("from stdin"));
    }

    #[test]
    fn none_returns_none() {
        let mut stdin = Cursor::new(Vec::new());
        assert!(resolve_text(None, None, &mut stdin).unwrap().is_none());
    }
}
