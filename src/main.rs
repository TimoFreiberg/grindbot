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
    /// Signal that implementation is complete and reviewed
    #[command(
        after_help = "Examples:\n  grindbot handoff done --commit abc123 --plan-review 'accepted' --implementation-review 'accepted' --test 'cargo test=passed' --acceptance 'AC.1=verified' --summary 'Implemented the feature'\n\nRepeat --test and --acceptance for each entry. The command writes .grindbot/result.json; no manifest file is needed."
    )]
    Done {
        /// jj revision containing the implementation; must be ahead of .grindbot/base_commit
        #[arg(
            long,
            value_name = "REVISION",
            help = "jj revision containing the implementation; must be ahead of .grindbot/base_commit"
        )]
        commit: String,
        /// Reviewer attestation for the accepted plan
        #[arg(
            long,
            value_name = "TEXT",
            help = "Reviewer attestation for the accepted plan"
        )]
        plan_review: String,
        /// Reviewer attestation for the accepted implementation
        #[arg(
            long,
            value_name = "TEXT",
            help = "Reviewer attestation for the accepted implementation"
        )]
        implementation_review: String,
        /// Test inventory entry, formatted as NAME=RESULT; repeat for each test
        #[arg(long, value_name = "NAME=RESULT", action = clap::ArgAction::Append, required = true, help = "Test inventory entry formatted as NAME=RESULT; repeat for each test (at least one required)")]
        test: Vec<String>,
        /// Acceptance mapping entry, formatted as CRITERION=VERIFICATION; repeat for each criterion
        #[arg(long, value_name = "CRITERION=VERIFICATION", action = clap::ArgAction::Append, required = true, help = "Acceptance mapping formatted as CRITERION=VERIFICATION; repeat for each criterion (at least one required)")]
        acceptance: Vec<String>,
        /// Short summary of the completed work
        #[arg(
            long,
            value_name = "TEXT",
            default_value = "",
            help = "Short summary of the completed work"
        )]
        summary: String,
        /// Issue number associated with the implementation
        #[arg(
            long,
            value_name = "NUMBER",
            help = "Issue number associated with the implementation"
        )]
        issue: Option<u64>,
        /// Mark the handoff as having unresolved findings (this makes it fail)
        #[arg(
            long,
            help = "Mark unresolved findings; successful handoffs reject this flag"
        )]
        unresolved_findings: bool,
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
            HandoffAction::Done {
                commit,
                plan_review,
                implementation_review,
                test,
                acceptance,
                summary,
                issue,
                unresolved_findings,
            } => {
                grindbot::handoff::done(
                    &commit,
                    &plan_review,
                    &implementation_review,
                    &test,
                    &acceptance,
                    &summary,
                    issue,
                    unresolved_findings,
                )?;
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
