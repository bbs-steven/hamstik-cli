// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Binary entrypoint.
//!
//! Parses arguments, assembles the production services, and hands off to
//! `app::run`. All logic lives in the library crate so it can be tested.

use std::io;
use std::path::PathBuf;

use clap::Parser;

use hamstik_cli::app::{self, ProductionApiFactory, Services};
use hamstik_cli::args::Cli;
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
    std::process::exit(code);
}

async fn entry() -> i32 {
    let cli = Cli::parse();
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

fn resolve_config_path() -> PathBuf {
    if let Ok(path) = std::env::var("HAMSTIK_CONFIG") {
        return PathBuf::from(path);
    }
    directories::ProjectDirs::from("com", "blackboard", "hamstik")
        .map(|dirs| dirs.config_dir().join("config.toml"))
        .unwrap_or_else(|| PathBuf::from(".hamstik").join("config.toml"))
}
