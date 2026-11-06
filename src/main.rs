use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};

use clch::clean::{self, BadName, Level, Selection};
use clch::config::{self, Config, ConfigError, RootError};
use clch::scan::Scope;
use clch::{dashboard, env::Env, output, scan, snapshots};

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
        /// Search this folder for project artifacts instead of the configured
        /// scan roots, for this run only. Can be given more than once.
        #[arg(long = "root", value_name = "FOLDER")]
        roots: Vec<PathBuf>,
        /// Of project artifacts, list only those in projects not changed for
        /// this many days.
        #[arg(long, value_name = "DAYS", value_parser = clap::value_parser!(u64).range(1..=36500))]
        older_than_days: Option<u64>,
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Show what cleaning would do, or with --yes, do it.
    Clean {
        /// What to clean: kinds of reclaimable by id, project artifacts by path.
        /// With none, every tool cache; project artifacts must be named or
        /// picked with --older-than-days.
        names: Vec<String>,
        /// Remove everything, not only what nothing uses.
        #[arg(long)]
        full: bool,
        /// Search this folder for project artifacts instead of the configured
        /// scan roots, for this run only. Can be given more than once.
        #[arg(long = "root", value_name = "FOLDER")]
        roots: Vec<PathBuf>,
        /// Clean only project artifacts in projects not changed for this many
        /// days. With no names, cleans those and no tool cache.
        #[arg(long, value_name = "DAYS", value_parser = clap::value_parser!(u64).range(1..=36500))]
        older_than_days: Option<u64>,
        /// Carry out the plan instead of only showing it.
        #[arg(long)]
        yes: bool,
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Open a dashboard in the browser, until Ctrl+C.
    Dashboard {
        /// Print the URL instead of opening the browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Manage the folders searched for project artifacts.
    Roots {
        #[command(subcommand)]
        command: RootsCommand,
    },
}

#[derive(Subcommand)]
enum RootsCommand {
    /// Add a folder to search.
    Add { folder: PathBuf },
    /// Stop searching a folder.
    Remove { folder: PathBuf },
    /// List the folders searched.
    List {
        /// Print JSON instead of a list.
        #[arg(long)]
        json: bool,
    },
}

/// What `clch roots list --json` prints.
#[derive(serde::Serialize)]
struct RootsList {
    scan_roots: Vec<PathBuf>,
}

/// The scan roots for this run: `roots` when given, else the configured ones.
fn scope(env: &Env, roots: Vec<PathBuf>) -> Result<Scope, ConfigError> {
    if !roots.is_empty() {
        return Ok(Scope {
            roots: roots.iter().map(|r| env.absolute(r)).collect(),
            one_off: true,
        });
    }
    config::configured_scope(env)
}

fn run_roots(env: &Env, command: RootsCommand) -> Result<ExitCode, ConfigError> {
    let changed = match command {
        RootsCommand::Add { folder } => config::add_root(env, &env.absolute(&folder)),
        RootsCommand::Remove { folder } => config::remove_root(env, &env.absolute(&folder)),
        RootsCommand::List { json } => {
            let scan_roots = Config::load(env)?.scan_roots(env);
            if json {
                print_json(&RootsList { scan_roots });
            } else {
                for root in scan_roots {
                    println!("{}", root.display());
                }
            }
            Ok(())
        }
    };
    match changed {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(RootError::Config(e)) => Err(e),
        Err(e) => {
            eprintln!("error: {e}");
            Ok(ExitCode::from(2))
        }
    }
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
    if !matches!(cli.command, Command::Dashboard { .. }) {
        env.report_progress_to(Duration::from_secs(1), Box::new(output::progress_on_stderr));
    }
    match run(&env, cli.command) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(env: &Env, command: Command) -> Result<ExitCode, ConfigError> {
    match command {
        Command::Scan {
            roots,
            older_than_days,
            json,
        } => {
            let mut result = scan::run(env, &scope(env, roots)?);
            if let Err(e) = snapshots::track(env, &mut result) {
                eprintln!("warning: snapshot history not available: {e}");
            }
            if let Some(days) = older_than_days {
                result.retain_older_than(days);
            }
            if json {
                print_json(&result);
            } else {
                print!("{}", output::scan_table(&result));
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Clean {
            names,
            full,
            roots,
            older_than_days,
            yes,
            json,
        } => {
            let level = if full { Level::Full } else { Level::Unused };
            let scope = scope(env, roots)?;
            let found = scan::run(env, &scope);
            let selection = Selection {
                names,
                older_than_days,
                ..Selection::default()
            };
            let plan = match clean::plan(env, &found, &selection, level) {
                Ok(plan) => plan,
                Err(BadName::Unknown(name)) => {
                    eprintln!(
                        "error: no reclaimable is called '{name}'. A project artifact is found \
                         only under the scan roots: add its folder with `clch roots add`, or \
                         give it for this run with --root."
                    );
                    return Ok(ExitCode::from(2));
                }
                Err(BadName::Ambiguous(name)) => {
                    eprintln!(
                        "error: '{name}' would mean every one under the scan roots, and this folder is \
                         part of a project of that kind. Use a path such as ./{name} for one, or \
                         run from outside the project for all of them."
                    );
                    return Ok(ExitCode::from(2));
                }
            };
            if !yes {
                if json {
                    print_json(&plan);
                } else {
                    print!("{}", output::plan_table(&plan));
                }
                return Ok(ExitCode::SUCCESS);
            }
            let (outcome, mut after) = clean::execute(env, plan, &found, &scope);
            if let Err(e) = snapshots::track_after_cleanup(env, &mut after, &outcome.attempted) {
                eprintln!("warning: snapshot not recorded: {e}");
            }
            if json {
                print_json(&outcome);
            } else {
                print!("{}", output::outcome_table(&outcome));
            }
            Ok(if outcome.any_failed() {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Dashboard { no_open } => match dashboard::serve(env, no_open) {
            Ok(()) => Ok(ExitCode::SUCCESS),
            Err(e) => {
                eprintln!("error: could not start the dashboard: {e}");
                Ok(ExitCode::from(2))
            }
        },
        Command::Roots { command } => run_roots(env, command),
    }
}
