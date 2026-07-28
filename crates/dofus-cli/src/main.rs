//! Command line front end for Dofus-tab.
//!
//! This is the v0 surface: it exercises the whole platform layer (enumerate,
//! identify, focus) without any GUI, so the hard part can be validated on every
//! target OS before a single pixel is drawn.

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "dofus-tab",
    version,
    about = "Switch between Dofus client windows from the command line"
)]
struct Cli {
    /// Print backend diagnostics to stderr.
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the detected Dofus windows as numbered tabs.
    List,

    /// Give focus to a tab, by slot number or by label.
    Focus {
        /// Slot number shown by `list`, or part of an account label.
        target: String,
    },

    /// Set a custom label for the account behind a slot.
    Label {
        /// Slot number shown by `list`.
        slot: u32,
        /// New label; pass an empty string to restore the launcher nickname.
        label: String,
    },

    /// Persist the current tab order so it survives restarts.
    Save,

    /// Register the global hotkeys and switch tabs until interrupted.
    Daemon,

    /// Report what the platform layer detects, for troubleshooting.
    Doctor,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::List => commands::list(),
        Command::Focus { target } => commands::focus(&target),
        Command::Label { slot, label } => commands::label(slot, &label),
        Command::Save => commands::save_order(),
        Command::Daemon => commands::daemon(),
        Command::Doctor => commands::doctor(),
    }
}

/// Sends logs to stderr so stdout stays parseable by scripts.
fn init_tracing(verbose: bool) {
    let default = if verbose { "debug" } else { "warn" };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .init();
}
