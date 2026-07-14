use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "grindbot",
    version,
    about = "Autonomous issue implementation supervisor"
)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the supervisor daemon
    Supervise {
        #[arg(short, long)]
        config: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Show current supervisor state
    Status {
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Check dependencies and configuration
    Doctor {
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
        manifest: PathBuf,
    },
    /// Request more information from the issue author
    NeedsFeedback {
        #[arg(long)]
        message: Option<String>,
        #[arg(long, conflicts_with = "message")]
        message_file: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let default_level = if cli.quiet {
        "warn"
    } else if cli.verbose >= 2 {
        "trace"
    } else if cli.verbose >= 1 {
        "debug"
    } else {
        "info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level)),
        )
        .init();

    match cli.command {
        Command::Supervise { config, dry_run } => {
            let config_path = config.unwrap_or_else(|| PathBuf::from("grindbot.toml"));
            let cfg = grindbot::config::Config::load(&config_path)?;
            cfg.validate()?;
            grindbot::supervisor::run(cfg, dry_run).await?;
        }
        Command::Status { config } => {
            let config_path = config.unwrap_or_else(|| PathBuf::from("grindbot.toml"));
            let cfg = grindbot::config::Config::load(&config_path)?;
            cfg.validate()?;
            grindbot::status::run(cfg).await?;
        }
        Command::Doctor { config } => {
            let cfg = if let Some(config_path) = config {
                let cfg = grindbot::config::Config::load(&config_path)?;
                cfg.validate()?;
                Some(cfg)
            } else {
                // Try default config path
                let default_path = PathBuf::from("grindbot.toml");
                if default_path.exists() {
                    grindbot::config::Config::load(&default_path).ok()
                } else {
                    None
                }
            };
            grindbot::doctor::run(cfg.as_ref()).await?;
        }
        Command::Handoff { action } => match action {
            HandoffAction::Done { manifest } => {
                grindbot::handoff::done_manifest(&manifest)?;
            }
            HandoffAction::NeedsFeedback {
                message,
                message_file,
            } => {
                let msg = if let Some(path) = message_file {
                    std::fs::read_to_string(&path)?.trim().to_string()
                } else if let Some(msg) = message {
                    msg
                } else {
                    anyhow::bail!("either --message or --message-file must be provided");
                };
                grindbot::handoff::needs_feedback(&msg)?;
            }
        },
    }

    Ok(())
}
