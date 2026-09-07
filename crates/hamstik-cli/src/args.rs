// Copyright 2026 Blackboard Studios
// SPDX-License-Identifier: Apache-2.0

//! Command-line argument definitions (clap).
//!
//! Global options are declared once and propagate to every subcommand
//! (SPEC §37). Enum-valued options are validated here on the request side; the
//! server remains authoritative for whether a value is actually accepted.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Top-level CLI definition.
#[derive(Parser, Debug)]
#[command(
    name = "hamstik",
    version,
    about = "Official command-line interface for Hamstik",
    propagate_version = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOptions,

    #[command(subcommand)]
    pub command: Command,
}

/// Global options available to every command.
#[derive(Args, Debug, Clone)]
pub struct GlobalOptions {
    /// Hamstik host origin (overrides profile/context/default).
    #[arg(long, global = true, value_name = "URL")]
    pub host: Option<String>,

    /// Profile name to use for credentials and defaults.
    #[arg(long, global = true, value_name = "NAME")]
    pub profile: Option<String>,

    /// Organization slug override.
    #[arg(long, global = true, value_name = "SLUG")]
    pub org: Option<String>,

    /// Project key override.
    #[arg(long, global = true, value_name = "KEY")]
    pub project: Option<String>,

    /// Emit machine-readable JSON.
    #[arg(long, global = true)]
    pub json: bool,

    /// Emit only essential identifiers.
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Show diagnostic details on stderr.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Disable colored output.
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Never prompt interactively; fail instead.
    #[arg(long, global = true)]
    pub no_input: bool,

    /// Disable automatic retries.
    #[arg(long, global = true)]
    pub no_retry: bool,

    /// Additional PEM root certificate bundle.
    #[arg(long, global = true, value_name = "PATH")]
    pub ca_bundle: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Manage authentication and profiles.
    Auth(AuthArgs),
    /// Inspect and manage the working context.
    Context(ContextArgs),
    /// Work with organizations.
    Org(OrgArgs),
    /// Work with projects.
    Project(ProjectArgs),
    /// Work with work items.
    Work(Box<WorkArgs>),
    /// Verify configuration, credentials, and connectivity.
    Doctor,
    /// Generate a shell completion script.
    Completion(CompletionArgs),
    /// Print the CLI version.
    Version,
}

#[derive(Args, Debug)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Subcommand, Debug)]
pub enum AuthCommand {
    /// Authenticate with a Personal Access Token.
    Login {
        /// Read the token from stdin instead of prompting.
        #[arg(long)]
        with_token: bool,
    },
    /// Show the current authentication state.
    Status,
    /// List configured profiles.
    List,
    /// Select the active profile.
    Switch {
        /// Profile name to activate.
        profile: String,
    },
    /// Remove the local credential (does not revoke the server-side PAT).
    Logout,
    /// Log out and forget a profile: remove its stored credential and its
    /// config entry (does not revoke the server-side PAT).
    Forget {
        /// Profile to forget (defaults to the selected profile).
        profile: Option<String>,
    },
}

#[derive(Args, Debug)]
pub struct ContextArgs {
    #[command(subcommand)]
    pub command: ContextCommand,
}

#[derive(Subcommand, Debug)]
pub enum ContextCommand {
    /// Show the resolved context.
    Show {
        /// Show the source of each value.
        #[arg(long)]
        explain: bool,
    },
    /// Set context values in the nearest .hamstik.toml.
    Set {
        /// Organization slug.
        #[arg(long)]
        org: Option<String>,
        /// Project key.
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove the current directory's .hamstik.toml values.
    Clear,
    /// Create a .hamstik.toml in the current directory.
    Init,
}

#[derive(Args, Debug)]
pub struct OrgArgs {
    #[command(subcommand)]
    pub command: OrgCommand,
}

#[derive(Subcommand, Debug)]
pub enum OrgCommand {
    /// List organizations you belong to.
    List(PaginationArgs),
    /// View an organization.
    View {
        /// Organization slug.
        slug: String,
    },
    /// Set the default organization for the active profile.
    Use {
        /// Organization slug.
        slug: String,
    },
}

#[derive(Args, Debug)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommand,
}

#[derive(Subcommand, Debug)]
pub enum ProjectCommand {
    /// List projects in the organization.
    List(PaginationArgs),
    /// View a project.
    View {
        /// Project key.
        key: String,
    },
    /// Set the default project for the active profile.
    Use {
        /// Project key.
        key: String,
    },
}

/// Shared list pagination options.
#[derive(Args, Debug, Clone)]
pub struct PaginationArgs {
    /// Maximum items per page (1-100).
    #[arg(long, value_name = "N")]
    pub limit: Option<u32>,
    /// Opaque continuation cursor.
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,
    /// Follow all pages.
    #[arg(long)]
    pub all: bool,
}

#[derive(Args, Debug)]
pub struct WorkArgs {
    #[command(subcommand)]
    pub command: WorkCommand,
}

#[derive(Subcommand, Debug)]
pub enum WorkCommand {
    /// List work items.
    List(WorkListArgs),
    /// View a work item.
    View {
        /// Work item key (e.g. HAM-42).
        key: String,
    },
    /// Create a work item.
    Create(WorkCreateArgs),
    /// Edit a work item.
    Edit(WorkEditArgs),
    /// List allowed status transitions.
    Transitions {
        /// Work item key.
        key: String,
    },
    /// Transition a work item to a target status.
    Transition {
        /// Work item key.
        key: String,
        /// Target status.
        #[arg(value_enum)]
        target: StatusArg,
    },
    /// Start a work item (transition to in_progress).
    Start {
        /// Work item key.
        key: String,
    },
    /// Close a work item (transition to done).
    Close {
        /// Work item key.
        key: String,
    },
    /// Manage work item comments.
    Comment(CommentArgs),
}

#[derive(Args, Debug)]
pub struct WorkListArgs {
    /// Free-text search.
    #[arg(long = "search", value_name = "TEXT")]
    pub search: Option<String>,
    /// Filter by status (repeatable).
    #[arg(long, value_enum)]
    pub status: Vec<StatusArg>,
    /// Status scope.
    #[arg(long, value_enum)]
    pub scope: Option<ScopeArg>,
    /// Filter by type (repeatable).
    #[arg(long = "type", value_enum)]
    pub item_type: Vec<TypeArg>,
    /// Filter by priority (repeatable).
    #[arg(long, value_enum)]
    pub priority: Vec<PriorityArg>,
    /// Filter by assignee: me, none, or a user UUID.
    #[arg(long, value_name = "ME|NONE|UUID")]
    pub assignee: Option<String>,
    /// Filter by sprint: none or a sprint UUID.
    #[arg(long, value_name = "NONE|UUID")]
    pub sprint: Option<String>,
    /// Filter by label UUID (repeatable).
    #[arg(long)]
    pub label: Vec<String>,
    /// Filter by label name (repeatable).
    #[arg(long = "label-name")]
    pub label_name: Vec<String>,
    /// Filter by parent work item key.
    #[arg(long)]
    pub parent: Option<String>,
    /// Only top-level work items.
    #[arg(long = "top-level")]
    pub top_level: bool,
    /// Only items updated at/after this RFC 3339 timestamp.
    #[arg(long = "updated-after", value_name = "RFC3339")]
    pub updated_after: Option<String>,
    /// Shorthand for --assignee me.
    #[arg(long)]
    pub mine: bool,
    #[command(flatten)]
    pub pagination: PaginationArgs,
}

#[derive(Args, Debug)]
pub struct WorkCreateArgs {
    /// Work item title.
    #[arg(long)]
    pub title: Option<String>,
    /// Description text.
    #[arg(long, conflicts_with = "description_file")]
    pub description: Option<String>,
    /// Description source (path, or - for stdin).
    #[arg(long = "description-file", value_name = "PATH")]
    pub description_file: Option<String>,
    #[arg(long = "type", value_enum)]
    pub item_type: Option<TypeArg>,
    /// Initial status.
    #[arg(long, value_enum)]
    pub status: Option<StatusArg>,
    #[arg(long, value_enum)]
    pub priority: Option<PriorityArg>,
    #[arg(long, value_name = "USER|me")]
    pub assignee: Option<String>,
    #[arg(long, value_name = "SPRINT")]
    pub sprint: Option<String>,
    #[arg(long, value_name = "KEY")]
    pub parent: Option<String>,
    #[arg(long = "story-points", value_name = "N")]
    pub story_points: Option<i64>,
    #[arg(long = "due-date", value_name = "DATE")]
    pub due_date: Option<String>,
    /// Explicit idempotency key.
    #[arg(long = "idempotency-key", value_name = "KEY")]
    pub idempotency_key: Option<String>,
}

#[derive(Args, Debug)]
pub struct WorkEditArgs {
    /// Work item key.
    pub key: String,
    #[arg(long, conflicts_with = "clear_description")]
    pub title: Option<String>,
    #[arg(long, conflicts_with = "clear_description")]
    pub description: Option<String>,
    #[arg(
        long = "description-file",
        value_name = "PATH",
        conflicts_with = "clear_description"
    )]
    pub description_file: Option<String>,
    #[arg(long = "type", value_enum)]
    pub item_type: Option<TypeArg>,
    #[arg(long, value_enum)]
    pub priority: Option<PriorityArg>,
    #[arg(long, value_name = "USER|me", conflicts_with = "clear_assignee")]
    pub assignee: Option<String>,
    #[arg(long, value_name = "SPRINT", conflicts_with = "clear_sprint")]
    pub sprint: Option<String>,
    #[arg(long, value_name = "KEY", conflicts_with = "clear_parent")]
    pub parent: Option<String>,
    #[arg(
        long = "story-points",
        value_name = "N",
        conflicts_with = "clear_story_points"
    )]
    pub story_points: Option<i64>,
    #[arg(
        long = "due-date",
        value_name = "DATE",
        conflicts_with = "clear_due_date"
    )]
    pub due_date: Option<String>,
    #[arg(long = "clear-description")]
    pub clear_description: bool,
    #[arg(long = "clear-assignee")]
    pub clear_assignee: bool,
    #[arg(long = "clear-sprint")]
    pub clear_sprint: bool,
    #[arg(long = "clear-parent")]
    pub clear_parent: bool,
    #[arg(long = "clear-story-points")]
    pub clear_story_points: bool,
    #[arg(long = "clear-due-date")]
    pub clear_due_date: bool,
    /// Bypass revision conflict protection (If-Match: *).
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct CommentArgs {
    #[command(subcommand)]
    pub command: CommentCommand,
}

#[derive(Subcommand, Debug)]
pub enum CommentCommand {
    /// List comments on a work item.
    List {
        /// Work item key.
        key: String,
        #[command(flatten)]
        pagination: PaginationArgs,
    },
    /// Add a comment to a work item.
    Add {
        /// Work item key.
        key: String,
        /// Comment body.
        #[arg(long, conflicts_with = "body_file")]
        body: Option<String>,
        /// Comment body source (path, or - for stdin).
        #[arg(long = "body-file", value_name = "PATH")]
        body_file: Option<String>,
        /// Parent comment UUID (for replies).
        #[arg(long, value_name = "UUID")]
        parent: Option<String>,
        /// Explicit idempotency key.
        #[arg(long = "idempotency-key", value_name = "KEY")]
        idempotency_key: Option<String>,
    },
}

#[derive(Args, Debug)]
pub struct CompletionArgs {
    /// Target shell.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}

// ---- Request-side value enums ---------------------------------------------

/// Work item statuses (request-side validation).
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusArg {
    #[value(name = "backlog")]
    Backlog,
    #[value(name = "todo")]
    Todo,
    #[value(name = "in_progress")]
    InProgress,
    #[value(name = "in_review")]
    InReview,
    #[value(name = "done")]
    Done,
}

impl StatusArg {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            StatusArg::Backlog => "backlog",
            StatusArg::Todo => "todo",
            StatusArg::InProgress => "in_progress",
            StatusArg::InReview => "in_review",
            StatusArg::Done => "done",
        }
    }
}

/// Work item types (request-side validation).
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeArg {
    Task,
    Bug,
    Story,
    Feature,
    Epic,
}

impl TypeArg {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            TypeArg::Task => "task",
            TypeArg::Bug => "bug",
            TypeArg::Story => "story",
            TypeArg::Feature => "feature",
            TypeArg::Epic => "epic",
        }
    }
}

/// Priorities (request-side validation).
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriorityArg {
    Low,
    Medium,
    High,
    Urgent,
}

impl PriorityArg {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PriorityArg::Low => "low",
            PriorityArg::Medium => "medium",
            PriorityArg::High => "high",
            PriorityArg::Urgent => "urgent",
        }
    }
}

/// List scope filter.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeArg {
    All,
    Open,
    Closed,
}

impl ScopeArg {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeArg::All => "all",
            ScopeArg::Open => "open",
            ScopeArg::Closed => "closed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_work_transition() {
        let cli = Cli::try_parse_from(["hamstik", "work", "transition", "HAM-1", "in_review"])
            .expect("parses");
        match cli.command {
            Command::Work(work) => match work.command {
                WorkCommand::Transition { key, target } => {
                    assert_eq!(key, "HAM-1");
                    assert_eq!(target, StatusArg::InReview);
                }
                _ => panic!("wrong subcommand"),
            },
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn global_flags_propagate() {
        let cli =
            Cli::try_parse_from(["hamstik", "work", "list", "--json", "--org", "acme"]).unwrap();
        assert!(cli.global.json);
        assert_eq!(cli.global.org.as_deref(), Some("acme"));
    }

    #[test]
    fn rejects_bad_status_value() {
        assert!(Cli::try_parse_from(["hamstik", "work", "transition", "HAM-1", "nope"]).is_err());
    }

    #[test]
    fn edit_conflict_flags_rejected() {
        assert!(
            Cli::try_parse_from([
                "hamstik",
                "work",
                "edit",
                "HAM-1",
                "--assignee",
                "me",
                "--clear-assignee"
            ])
            .is_err()
        );
    }
}
