// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! `hamstik completion <shell>`.

use clap::CommandFactory;

use crate::app::Session;
use crate::args::{Cli, CompletionArgs};
use crate::error::CliError;

/// Prints the completion script for the requested shell.
pub fn run(session: &mut Session<'_>, args: &CompletionArgs) -> Result<(), CliError> {
    let mut command = Cli::command();
    let mut script = Vec::new();
    clap_complete::generate(args.shell, &mut command, "hamstik", &mut script);
    let rendered = String::from_utf8(script).map_err(|err| {
        CliError::general(format!("completion script was not valid UTF-8: {err}"))
    })?;
    session.out.raw(&rendered).map_err(CliError::general)
}
