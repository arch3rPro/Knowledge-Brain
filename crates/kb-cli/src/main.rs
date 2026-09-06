use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use kb_app::{InitRequest, init_vault};
use kb_protocol::{Envelope, ErrorEnvelope};

#[derive(Parser)]
#[command(
    name = "kb",
    version,
    about = "Knowledge-Brain portable knowledge vault"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a minimum Vault in a nonexistent or empty directory.
    Init {
        target: PathBuf,
        /// Emit one stable JSON response on stdout.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { target, json } => match init_vault(&InitRequest { target }) {
            Ok(report) => {
                if json {
                    println!("{}", serde_json::to_string(&Envelope::new(report)).unwrap());
                } else {
                    println!("Initialized Vault at {}", report.root.display());
                }
                ExitCode::SUCCESS
            }
            Err(error) => {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string(&ErrorEnvelope::from(error)).unwrap()
                    );
                } else {
                    eprintln!("Error: {error}");
                }
                ExitCode::FAILURE
            }
        },
    }
}
