//! PROTOTYPE, throwaway. Answers one question: what should `clch clean --yes`
//! show while it works?
//!
//! Replays a made-up run (no files are touched) in three layouts:
//!   A  stacked lines only
//!   B  one line rewritten in place, with a phase prefix
//!   C  phase headers and per-item lines kept, one line rewritten only for the
//!      long scan steps
//!
//!   cargo run --example prototype_clean_progress -- A
//!   cargo run --example prototype_clean_progress -- all
//!
//! Progress goes to stderr and the final result to stdout, as in clch.

use std::io::{IsTerminal, Write};
use std::thread::sleep;
use std::time::{Duration, Instant};

const WIDTH: usize = 70;

struct Out {
    tty: bool,
}

impl Out {
    /// A line that the next write replaces. Dropped when stderr is not a terminal.
    fn ephemeral(&self, text: &str) {
        if self.tty {
            let chars: Vec<char> = text.chars().collect();
            let tail: String = chars[chars.len().saturating_sub(WIDTH)..].iter().collect();
            eprint!("\r\x1b[K{tail}");
            let _ = std::io::stderr().flush();
        }
    }

    fn clear(&self) {
        if self.tty {
            eprint!("\r\x1b[K");
        }
    }

    /// A line that stays.
    fn line(&self, text: &str) {
        self.clear();
        eprintln!("{text}");
    }

    /// The start of a line that is finished by `finish`.
    fn begin(&self, text: &str) {
        self.clear();
        eprint!("{text}");
        let _ = std::io::stderr().flush();
    }

    fn finish(&self, text: &str) {
        eprintln!("{text}");
    }
}

struct Item {
    name: &'static str,
    action: &'static str,
    millis: u64,
    fails: Option<&'static str>,
}

const ITEMS: [Item; 5] = [
    Item {
        name: "pnpm store",
        action: "pnpm store prune",
        millis: 1800,
        fails: None,
    },
    Item {
        name: "npm cache",
        action: "npm cache clean --force",
        millis: 900,
        fails: None,
    },
    Item {
        name: "D:\\github\\old-app\\node_modules",
        action: "delete folder",
        millis: 2600,
        fails: None,
    },
    Item {
        name: "cargo registry",
        action: "delete cache",
        millis: 1200,
        fails: None,
    },
    Item {
        name: "D:\\github\\web\\target",
        action: "delete folder",
        millis: 400,
        fails: Some("in use by another process"),
    },
];

fn steps(verb: &str) -> Vec<String> {
    (1..=48)
        .map(|n| format!("{verb} D:\\github\\project-{:02}\\node_modules", n % 16 + 1))
        .collect()
}

fn scan(out: &Out, v: char, phase: &str, verb: &str, summary: &str) {
    let (num, title) = phase.split_once(' ').unwrap_or(("", phase));
    let header = format!("[{num}] {title}");
    match v {
        'A' => {
            out.line(&header);
            for root in ["D:\\github", "D:\\work", "C:\\Users\\User\\code"] {
                for _ in 0..16 {
                    sleep(Duration::from_millis(25));
                }
                out.line(&format!("  searched {root}"));
            }
        }
        'B' => {
            for s in steps(verb) {
                out.ephemeral(&format!("[{phase}] {s}"));
                sleep(Duration::from_millis(25));
            }
            out.clear();
        }
        _ => {
            out.line(&header);
            for s in steps(verb) {
                out.ephemeral(&format!("  {s}"));
                sleep(Duration::from_millis(25));
            }
            out.line(&format!("  {summary}"));
        }
    }
}

fn clean(out: &Out, v: char) {
    let total = ITEMS.len();
    if v != 'B' {
        out.line(&format!("[2/3] Cleaning {total} items"));
    }
    let mut results = Vec::new();
    for (i, item) in ITEMS.iter().enumerate() {
        let n = i + 1;
        let started = Instant::now();
        match v {
            'A' => out.line(&format!("  [{n}/{total}] {}: {}", item.name, item.action)),
            'B' => out.ephemeral(&format!(
                "[2/3 Cleaning {n}/{total}] {}: {}",
                item.name, item.action
            )),
            _ => out.begin(&format!(
                "  [{n}/{total}] {}: {} ... ",
                item.name, item.action
            )),
        }
        sleep(Duration::from_millis(item.millis));
        let secs = started.elapsed().as_secs_f32();
        let result = match item.fails {
            None => format!("cleaned ({secs:.1} s)"),
            Some(why) => format!("failed: {why} ({secs:.1} s)"),
        };
        match v {
            'A' => out.line(&format!("        {result}")),
            'B' => {}
            _ => out.finish(&result),
        }
        results.push(result);
    }
    if v == 'B' {
        out.clear();
    }
}

fn run(v: char) {
    let out = Out {
        tty: std::io::stderr().is_terminal(),
    };
    eprintln!("--- variant {v} ---");
    scan(
        &out,
        v,
        "1/3 Scanning",
        "Measuring",
        "found 14 reclaimables",
    );
    clean(&out, v);
    scan(
        &out,
        v,
        "3/3 Measuring what was freed",
        "Measuring",
        "measured",
    );
    println!();
    println!("NAME            RESULT                          FREED");
    println!("pnpm store      cleaned                         1.2 GB");
    println!("npm cache       cleaned                         310 MB");
    println!("old-app         cleaned                         1.4 GB");
    println!("cargo registry  cleaned                         180 MB");
    println!("web\\target      failed: in use by another ...   0 B");
    println!("\nFreed about 3.1 GB in total.");
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default().to_uppercase();
    match arg.as_str() {
        "A" | "B" | "C" => run(arg.chars().next().unwrap()),
        "ALL" => {
            for v in ['A', 'B', 'C'] {
                run(v);
                println!("\n");
            }
        }
        _ => eprintln!("usage: prototype_clean_progress <A|B|C|all>"),
    }
}
