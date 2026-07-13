use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "grindbot", about = "Autonomous issue implementation supervisor")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the supervisor daemon
    Supervise {
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Signal session completion (called by implementer agents)
    Handoff {
        #[command(subcommand)]
        action: HandoffAction,
    },
}

#[derive(Subcommand)]
enum HandoffAction {
    /// Signal that implementation is complete
    Done {
        #[arg(long)]
        commit: String,
    },
    /// Request more information from the issue author
    NeedsFeedback {
        #[arg(long)]
        message: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Supervise { config } => {
            let config_path = config.unwrap_or_else(|| PathBuf::from("grindbot.toml"));
            let cfg = grindbot::config::Config::load(&config_path)?;
            grindbot::supervisor::run(cfg).await?;
        }
        Command::Handoff { action } => {
            match action {
                HandoffAction::Done { commit } => {
                    grindbot::handoff::done(&commit)?;
                }
                HandoffAction::NeedsFeedback { message } => {
                    grindbot::handoff::needs_feedback(&message)?;
                }
            }
        }
    }

    Ok(())
}
