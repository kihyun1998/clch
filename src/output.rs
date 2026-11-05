//! Rendering results for people.

use std::io::IsTerminal;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::catalog::{self, Kind};
use crate::clean::{Level, Outcome, Plan, Status};
use crate::scan::{ChangeState, MS_PER_DAY, Scan, ToolNote};

/// A byte count in decimal units (kB, MB, GB).
/// `path` with control characters replaced, so that a folder's name cannot move
/// the cursor or invent a row in a table.
fn shown(path: &Path) -> String {
    path.display()
        .to_string()
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Says on stderr what a scan is doing: one line replaced in place on a terminal,
/// otherwise a line per message.
pub fn progress_on_stderr(message: Option<&str>) {
    let terminal = std::io::stderr().is_terminal();
    match message {
        Some(text) if terminal => {
            let chars: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
            let start = chars.len().saturating_sub(PROGRESS_WIDTH);
            let tail: String = chars[start..].iter().collect();
            eprint!("\r\x1b[K{tail}");
        }
        Some(text) => eprintln!("{text}"),
        None if terminal => eprint!("\r\x1b[K"),
        None => {}
    }
}

/// The most of a message shown on a terminal, so that it stays on one line.
const PROGRESS_WIDTH: usize = 70;

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

/// How long before `now_ms` the moment `ms` was, in whole days.
fn days_ago(ms: i64, now_ms: i64) -> String {
    match (now_ms - ms).max(0) / MS_PER_DAY {
        0 => "today".to_string(),
        1 => "1 day ago".to_string(),
        days => format!("{days} days ago"),
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
        Kind::ProjectArtifact => "project artifact",
    }
}

/// A line per tool whose numbers are missing, saying why.
fn tool_notes(notes: &[ToolNote]) -> String {
    notes
        .iter()
        .map(|n| format!("\nNothing from {}: {}.\n", n.tool, n.reason))
        .collect()
}

pub fn scan_table(scan: &Scan) -> String {
    let mut out = reclaimables_table(scan);
    out.push_str(&tool_notes(&scan.tool_notes));
    if scan.project_artifacts_skipped {
        out.push_str(
            "\nProject artifacts were not looked for: no scan roots are set. \
             Add one with `clch roots add <folder>`.\n",
        );
    }
    for root in &scan.unavailable_roots {
        out.push_str(&format!("\nScan root not available: {}\n", shown(root)));
    }
    for root in &scan.incomplete_roots {
        out.push_str(&format!(
            "\nPart of this scan root could not be read: {}\n",
            shown(root)
        ));
    }
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
                    shown(&g.path),
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
    let rows: Vec<[String; 7]> = scan
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
                r.last_modified_ms
                    .map_or("-".to_string(), |ms| days_ago(ms, scan.taken_at_ms)),
                shown(&r.path),
            ]
        })
        .collect();
    table(
        [
            "KIND",
            "NAME",
            "SIZE",
            "CHANGE",
            "DISK",
            "LAST CHANGED",
            "PATH",
        ],
        &rows,
    )
}

fn level_label(level: Level) -> &'static str {
    match level {
        Level::Unused => "unused cleanup",
        Level::Full => "full cleanup",
    }
}

pub fn plan_table(plan: &Plan) -> String {
    let heading = if plan.uses_level() {
        level_label(plan.level)
    } else {
        "delete project artifacts"
    };
    let mut out = format!("Plan: {heading}. Nothing has been removed.\n\n");
    if plan.project_artifacts_skipped {
        out.push_str(
            "Project artifacts were not looked for: no scan roots are set. \
             Add one with `clch roots add <folder>`.\n\n",
        );
    }
    if plan.plan.is_empty() && plan.not_selected == 0 {
        out.push_str("No reclaimables found.\n");
        out.push_str(&tool_notes(&plan.tool_notes));
        return out;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let rows: Vec<[String; 5]> = plan
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
            let path = e.path.as_ref().map_or(String::new(), |p| shown(p));
            let changed = e
                .last_modified_ms
                .map_or(String::new(), |ms| days_ago(ms, now));
            [e.name.clone(), action, space, changed, path]
        })
        .collect();
    out.push_str(&table(
        ["NAME", "ACTION", "SPACE", "LAST CHANGED", "PATH"],
        &rows,
    ));
    let mut warned: Vec<&str> = Vec::new();
    for entry in &plan.plan {
        if let Some(warning) = entry.warning.as_deref()
            && !warned.contains(&warning)
        {
            warned.push(warning);
            out.push_str(&format!("\nWarning: {warning}\n"));
        }
    }
    if plan.expected_total_bytes > 0 {
        out.push_str(&format!(
            "\nFrees at most {}.\n",
            human_size(plan.expected_total_bytes)
        ));
    }
    if plan.not_selected > 0 {
        out.push_str(&format!(
            "\n{} project artifact(s) not selected: name a kind or a path, or use --older-than-days.\n",
            plan.not_selected
        ));
    }
    out.push_str(&tool_notes(&plan.tool_notes));
    if plan.plan.iter().any(|e| e.cleanable) {
        out.push_str("\nRun again with --yes to carry out this plan.\n");
    }
    out
}

pub fn outcome_table(outcome: &Outcome) -> String {
    let rows: Vec<[String; 4]> = outcome
        .results
        .iter()
        .map(|r| {
            let result = match r.status {
                Status::Cleaned => "cleaned".to_string(),
                Status::Skipped => format!("skipped: {}", r.reason.as_deref().unwrap_or_default()),
                Status::Failed => format!("failed: {}", r.output.as_deref().unwrap_or_default()),
            };
            let path = r.path.as_ref().map_or(String::new(), |p| shown(p));
            [
                r.name.clone(),
                result,
                estimated(r.freed_bytes, r.complete),
                path,
            ]
        })
        .collect();
    let mut out = table(["NAME", "RESULT", "FREED", "PATH"], &rows);
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
    out.push_str(&tool_notes(&outcome.tool_notes));
    for note in &outcome.notes {
        out.push_str(&format!("\n{note}\n"));
    }
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
