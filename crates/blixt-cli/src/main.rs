mod commands;
mod tailwind;
mod validate;

#[cfg(test)]
mod test_helpers;

use clap::{Parser, Subcommand};
use console::style;

#[derive(Parser)]
#[command(name = "blixt", about = "Lightning-fast Rust web framework")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Blixt project
    New {
        /// Project name
        name: String,
    },
    /// Start the development server
    Dev,
    /// Build for production
    Build,
    /// Generate scaffolding
    Generate {
        #[command(subcommand)]
        kind: GenerateKind,
    },
    /// Run database migrations
    Db {
        #[command(subcommand)]
        action: DbAction,
    },
}

#[derive(Subcommand)]
enum GenerateKind {
    /// Generate a controller with views
    Controller { name: String },
    /// Generate a model with migration
    Model { name: String },
    /// Generate full CRUD scaffold
    Scaffold { name: String },
}

#[derive(Subcommand)]
enum DbAction {
    /// Run pending migrations
    Migrate,
    /// Rollback last migration
    Rollback,
    /// Check migration status
    Status,
}

/// Validates a name or prints a styled error and exits.
fn require_valid_name(name: &str) -> String {
    match validate::validate_name(name) {
        Ok(valid) => valid,
        Err(message) => {
            eprintln!("{} {message}", style("error:").red().bold());
            std::process::exit(1);
        }
    }
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name } => {
            let name = require_valid_name(&name);
            let runtime = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            if let Err(message) = runtime.block_on(commands::new::run(&name)) {
                eprintln!("{} {message}", style("error:").red().bold());
                std::process::exit(1);
            }
        }
        Commands::Dev => {
            let runtime = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            if let Err(message) = runtime.block_on(commands::dev::run()) {
                eprintln!("{} {message}", style("error:").red().bold());
                std::process::exit(1);
            }
        }
        Commands::Build => {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|err| format!("Failed to create tokio runtime: {err}"))
                .unwrap_or_else(|msg| {
                    eprintln!("{} {msg}", style("error:").red().bold());
                    std::process::exit(1);
                });
            if let Err(message) = runtime.block_on(commands::build::run()) {
                eprintln!("{} {message}", style("error:").red().bold());
                std::process::exit(1);
            }
        }
        Commands::Generate { kind } => match kind {
            GenerateKind::Controller { name } => {
                let name = require_valid_name(&name);
                if let Err(msg) = commands::generate::generate_controller(&name) {
                    eprintln!("{} {msg}", style("error:").red().bold());
                    std::process::exit(1);
                }
            }
            GenerateKind::Model { name } => {
                let name = require_valid_name(&name);
                if let Err(msg) = commands::generate::generate_model(&name) {
                    eprintln!("{} {msg}", style("error:").red().bold());
                    std::process::exit(1);
                }
            }
            GenerateKind::Scaffold { name } => {
                let name = require_valid_name(&name);
                if let Err(msg) = commands::generate::generate_scaffold(&name) {
                    eprintln!("{} {msg}", style("error:").red().bold());
                    std::process::exit(1);
                }
            }
        },
        Commands::Db { action } => {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|err| format!("Failed to create tokio runtime: {err}"))
                .unwrap_or_else(|msg| {
                    eprintln!("{} {msg}", style("error:").red().bold());
                    std::process::exit(1);
                });
            let result = match action {
                DbAction::Migrate => runtime.block_on(commands::db::migrate()),
                DbAction::Rollback => runtime.block_on(commands::db::rollback()),
                DbAction::Status => runtime.block_on(commands::db::status()),
            };
            if let Err(message) = result {
                eprintln!("{} {message}", style("error:").red().bold());
                std::process::exit(1);
            }
        }
    }
}
