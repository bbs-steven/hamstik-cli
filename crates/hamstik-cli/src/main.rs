// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "hamstik",
    about = "Official command-line interface for Hamstik",
    version,
    arg_required_else_help = true
)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
