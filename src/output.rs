//! Rendering results for people.

use crate::catalog::Kind;
use crate::scan::Scan;

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

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::ToolCache => "tool cache",
    }
}

pub fn scan_table(scan: &Scan) -> String {
    if scan.reclaimables.is_empty() {
        return "No reclaimables found.\n".to_string();
    }
    let rows: Vec<[String; 4]> = scan
        .reclaimables
        .iter()
        .map(|r| {
            [
                kind_label(r.kind).to_string(),
                r.name.clone(),
                human_size(r.size_bytes),
                r.path.display().to_string(),
            ]
        })
        .collect();
    table(["KIND", "NAME", "SIZE", "PATH"], &rows)
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
    fn sizes_use_decimal_units() {
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1_500_000), "1.5 MB");
        assert_eq!(human_size(1_600_000_000), "1.6 GB");
    }
}
