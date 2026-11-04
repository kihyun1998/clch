use clap::{Parser, Subcommand};

use clch::{env::Env, output, scan};

/// Find, track and safely clean developer caches and project artifacts.
#[derive(Parser)]
#[command(name = "clch", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List every reclaimable on this machine with its size.
    Scan {
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let env = Env::from_process();
    match cli.command {
        Command::Scan { json } => {
            let result = scan::run(&env);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).expect("scan serializes")
                );
            } else {
                print!("{}", output::scan_table(&result));
            }
        }
    }
}
