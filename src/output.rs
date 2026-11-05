//! Rendering results for people.

use crate::catalog::{self, Kind};
use crate::clean::{Level, Outcome, Plan, Status};
use crate::scan::{ChangeState, Scan};

/// A byte count in decimal units (kB, MB, GB).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A signed change in size, such as `+2.0 kB` or `-300 B`.
pub fn human_change(change: Option<i64>) -> String {
    match change {
        None => "unknown".to_string(),
        Some(0) => "0 B".to_string(),
        Some(c) if c > 0 => format!("+{}", human_size(c as u64)),
        Some(c) => format!("-{}", human_size(c.unsigned_abs())),
    }
}

/// A size, marked as a lower bound when part of it could not be read.
fn bounded(bytes: u64, complete: bool) -> String {
    if complete {
        human_size(bytes)
    } else {
        format!("at least {}", human_size(bytes))
    }
}

/// A freed size, marked as an estimate when a measurement behind it was a
/// lower bound.
fn estimated(bytes: u64, complete: bool) -> String {
    if complete {
        human_size(bytes)
    } else {
        format!("about {}", human_size(bytes))
    }
}

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::ToolCache => "tool cache",
    }
}

pub fn scan_table(scan: &Scan) -> String {
    let mut out = reclaimables_table(scan);
    if !scan.disks.is_empty() {
        let rows: Vec<[String; 3]> = scan
            .disks
            .iter()
            .map(|d| {
                [
                    d.disk.clone(),
                    d.free_bytes.map_or("unknown".to_string(), human_size),
                    bounded(d.reclaimable_bytes, d.complete),
                ]
            })
            .collect();
        out.push('\n');
        out.push_str(&table(["DISK", "FREE", "RECLAIMABLE"], &rows));
    }
    if !scan.gone.is_empty() {
        out.push_str("\nGone since the last snapshot:\n");
        let rows: Vec<[String; 3]> = scan
            .gone
            .iter()
            .map(|g| {
                let name = catalog::tool_cache(&g.id).map_or(g.id.as_str(), |k| k.name);
                [
                    name.to_string(),
                    if g.complete {
                        human_change(Some(-(g.previous_bytes as i64)))
                    } else {
                        format!(
                            "at least {}",
                            human_change(Some(-(g.previous_bytes as i64)))
                        )
                    },
                    g.path.display().to_string(),
                ]
            })
            .collect();
        out.push_str(&table(["NAME", "CHANGE", "PATH"], &rows));
    }
    out
}

fn reclaimables_table(scan: &Scan) -> String {
    if scan.reclaimables.is_empty() {
        return "No reclaimables found.\n".to_string();
    }
    let rows: Vec<[String; 6]> = scan
        .reclaimables
        .iter()
        .map(|r| {
            [
                kind_label(r.kind).to_string(),
                r.name.clone(),
                bounded(r.size_bytes, r.complete),
                match r.change_state {
                    ChangeState::Known => human_change(r.change_bytes),
                    ChangeState::New => "new".to_string(),
                    ChangeState::Unknown => "unknown".to_string(),
                },
                r.disk.clone().unwrap_or_else(|| "unknown".to_string()),
                r.path.display().to_string(),
            ]
        })
        .collect();
    table(["KIND", "NAME", "SIZE", "CHANGE", "DISK", "PATH"], &rows)
}

fn level_label(level: Level) -> &'static str {
    match level {
        Level::Unused => "unused cleanup",
        Level::Full => "full cleanup",
    }
}

pub fn plan_table(plan: &Plan) -> String {
    let mut out = format!(
        "Plan: {}. Nothing has been removed.\n\n",
        level_label(plan.level)
    );
    if plan.plan.is_empty() {
        out.push_str("No reclaimables found.\n");
        return out;
    }
    let rows: Vec<[String; 3]> = plan
        .plan
        .iter()
        .map(|e| {
            let space = match (e.expected_bytes, e.cleanable) {
                (Some(b), _) => bounded(b, e.complete),
                (None, true) => "known after cleaning".to_string(),
                (None, false) => String::new(),
            };
            let action = match (&e.action, &e.reason) {
                (Some(a), _) => a.clone(),
                (None, Some(r)) => format!("skipped: {r}"),
                _ => String::new(),
            };
            [e.name.clone(), action, space]
        })
        .collect();
    out.push_str(&table(["NAME", "ACTION", "SPACE"], &rows));
    out.push_str("\nRun again with --yes to carry out this plan.\n");
    out
}

pub fn outcome_table(outcome: &Outcome) -> String {
    let rows: Vec<[String; 3]> = outcome
        .results
        .iter()
        .map(|r| {
            let result = match r.status {
                Status::Cleaned => "cleaned".to_string(),
                Status::Skipped => format!("skipped: {}", r.reason.as_deref().unwrap_or_default()),
                Status::Failed => format!("failed: {}", r.output.as_deref().unwrap_or_default()),
            };
            [r.name.clone(), result, estimated(r.freed_bytes, r.complete)]
        })
        .collect();
    let mut out = table(["NAME", "RESULT", "FREED"], &rows);
    if !outcome.disks.is_empty() {
        out.push('\n');
        let rows: Vec<[String; 2]> = outcome
            .disks
            .iter()
            .map(|d| [d.disk.clone(), estimated(d.freed_bytes, d.complete)])
            .collect();
        out.push_str(&table(["DISK", "FREED"], &rows));
    }
    out.push_str(&format!(
        "\nFreed {} in total.\n",
        estimated(outcome.freed_bytes, outcome.complete)
    ));
    out
}

fn table<const N: usize>(header: [&str; N], rows: &[[String; N]]) -> String {
    let mut widths = header.map(str::len);
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.len());
        }
    }
    let line = |cells: Vec<&str>| {
        let padded: Vec<String> = cells
            .iter()
            .zip(widths)
            .map(|(c, w)| format!("{c:<w$}"))
            .collect();
        format!("{}\n", padded.join("  ").trim_end())
    };
    let mut out = line(header.to_vec());
    for row in rows {
        out.push_str(&line(row.iter().map(String::as_str).collect()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::human_size;

    #[test]
    fn changes_carry_their_sign() {
        use super::human_change;
        assert_eq!(human_change(None), "unknown");
        assert_eq!(human_change(Some(0)), "0 B");
        assert_eq!(human_change(Some(2000)), "+2.0 kB");
        assert_eq!(human_change(Some(-300)), "-300 B");
    }

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1_500_000), "1.5 MB");
        assert_eq!(human_size(1_600_000_000), "1.6 GB");
    }
}
