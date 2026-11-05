use std::process::ExitCode;

use clap::{Parser, Subcommand};

use clch::clean::{self, Level};
use clch::{env::Env, output, scan, snapshots};

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
    /// Show what cleaning would do, or with --yes, do it.
    Clean {
        /// Reclaimables to clean, by id; all of them when none are given.
        ids: Vec<String>,
        /// Remove everything, not only what nothing uses.
        #[arg(long)]
        full: bool,
        /// Carry out the plan instead of only showing it.
        #[arg(long)]
        yes: bool,
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
}

fn print_json(value: &impl serde::Serialize) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("result serializes")
    );
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let env = Env::from_process();
    match cli.command {
        Command::Scan { json } => {
            let mut result = scan::run(&env);
            if let Err(e) = snapshots::track(&env, &mut result) {
                eprintln!("warning: snapshot history not available: {e}");
            }
            if json {
                print_json(&result);
            } else {
                print!("{}", output::scan_table(&result));
            }
            ExitCode::SUCCESS
        }
        Command::Clean {
            ids,
            full,
            yes,
            json,
        } => {
            let level = if full { Level::Full } else { Level::Unused };
            let found = scan::run(&env);
            let plan = match clean::plan(&env, &found, &ids, level) {
                Ok(plan) => plan,
                Err(clean::UnknownId(id)) => {
                    eprintln!("error: no reclaimable is called '{id}'");
                    return ExitCode::from(2);
                }
            };
            if !yes {
                if json {
                    print_json(&plan);
                } else {
                    print!("{}", output::plan_table(&plan));
                }
                return ExitCode::SUCCESS;
            }
            let (outcome, after) = clean::execute(&env, plan, &found);
            if json {
                print_json(&outcome);
            } else {
                print!("{}", output::outcome_table(&outcome));
            }
            if let Err(e) = snapshots::record(&env, &after) {
                eprintln!("warning: snapshot not recorded: {e}");
            }
            if outcome.any_failed() {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}
