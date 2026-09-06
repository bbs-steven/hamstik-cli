// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Binary entrypoint.
//!
//! Parses arguments, assembles the production services, and hands off to
//! `app::run`. All logic lives in the library crate so it can be tested.

use std::io::{self, Write};
use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches};

use hamstik_cli::app::{self, ProductionApiFactory, Services};
use hamstik_cli::args::Cli;
use hamstik_cli::banner;
use hamstik_cli::config::ConfigStore;
use hamstik_cli::credentials::KeyringCredentialStore;
use hamstik_cli::environment::SystemEnvironment;
use hamstik_cli::input::TerminalInput;

fn main() {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("error: failed to start runtime: {err}");
            std::process::exit(hamstik_cli::exit::GENERAL);
        }
    };
    let code = runtime.block_on(entry());
    let _ = io::stdout().flush();
    std::process::exit(code);
}

async fn entry() -> i32 {
    let command = Cli::command().help_template(banner::root_help_template());
    let matches = match command.try_get_matches() {
        Ok(matches) => matches,
        Err(err) => return render_clap_error(&err),
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(err) => return render_clap_error(&err),
    };
    let config = ConfigStore::new(resolve_config_path());
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    let environment = SystemEnvironment;
    let store = KeyringCredentialStore;
    let factory = ProductionApiFactory;
    let mut prompt = TerminalInput;

    let services = Services {
        env: &environment,
        store: &store,
        config,
        factory: &factory,
        prompt: &mut prompt,
        cwd,
        stdout: Box::new(io::stdout()),
        stderr: Box::new(io::stderr()),
    };

    app::run(cli, services).await
}

/// Renders clap-generated errors, keeping the banner on stdout only.
///
/// - `--version`/`-V`: print the banner to stdout, exit success.
/// - Help (root/subcommand `--help`, and the bare-invocation help): print to
///   stdout while preserving clap's numeric exit code (0 for `--help`, 2 for
///   the missing-subcommand help). The banner rides in the root help template.
/// - Anything else: defer to clap's own rendering on stderr.
fn render_clap_error(err: &clap::Error) -> i32 {
    match err.kind() {
        ErrorKind::DisplayVersion => {
            println!("{}", banner::banner());
            hamstik_cli::exit::SUCCESS
        }
        ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            print!("{err}");
            err.exit_code()
        }
        _ => err.exit(),
    }
}

fn resolve_config_path() -> PathBuf {
    if let Ok(path) = std::env::var("HAMSTIK_CONFIG") {
        return PathBuf::from(path);
    }
    directories::ProjectDirs::from("com", "blackboard", "hamstik")
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from(".hamstik").join("config.toml"))
}
