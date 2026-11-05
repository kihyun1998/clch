mod common;

use common::{Dashboard, Sandbox};

/// A fake npm that takes `seconds` to say where its cache is.
fn slow_npm(sb: &Sandbox, seconds: u32) {
    sb.fake_tool(
        "npm",
        &format!("sleep {seconds}"),
        &format!("ping -n {} 127.0.0.1 >nul", seconds + 1),
    );
}

fn with_project(sb: &Sandbox) -> String {
    let root = sb.root().join("code");
    sb.node_project(&root.join("app"), 1);
    root.display().to_string()
}

#[test]
fn a_long_scan_says_what_it_is_doing_on_stderr() {
    let sb = Sandbox::new();
    let root = with_project(&sb);
    slow_npm(&sb, 2);

    let out = sb.clch().args(["scan", "--root", &root]).output().unwrap();

    let told = String::from_utf8_lossy(&out.stderr);
    assert!(
        told.contains("Searching") || told.contains("Measuring"),
        "{told}"
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("Searching"));
}

#[test]
fn a_quick_scan_says_nothing_extra() {
    let sb = Sandbox::new();
    let root = with_project(&sb);

    let out = sb.clch().args(["scan", "--root", &root]).output().unwrap();

    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn json_output_stays_clean_during_a_long_scan() {
    let sb = Sandbox::new();
    let root = with_project(&sb);
    slow_npm(&sb, 2);

    let out = sb
        .clch()
        .args(["scan", "--root", &root, "--json"])
        .output()
        .unwrap();

    assert!(!out.stderr.is_empty());
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).expect("only JSON on stdout");
    assert!(scan["reclaimables"].is_array());
}

#[test]
fn a_piped_stderr_holds_no_terminal_controls() {
    let sb = Sandbox::new();
    let root = with_project(&sb);
    slow_npm(&sb, 2);

    let out = sb.clch().args(["scan", "--root", &root]).output().unwrap();

    assert!(!out.stderr.contains(&0x1b), "{:?}", out.stderr);
    assert!(!out.stderr.contains(&b'\r') || cfg!(windows));
}

#[test]
fn the_dashboard_can_ask_what_a_scan_is_doing() {
    let sb = Sandbox::new();
    with_project(&sb);
    slow_npm(&sb, 4);
    let running = Dashboard::start(&sb, &["--no-open"]);

    let idle = running.api("GET", "/api/progress", "").json();
    assert!(idle["message"].is_null(), "{idle}");

    std::thread::scope(|scope| {
        let scanning = scope.spawn(|| running.api("POST", "/api/scan", ""));
        let mut seen = None;
        for _ in 0..500 {
            let now = running.api("GET", "/api/progress", "").json();
            if now["message"].is_string() {
                seen = Some(now);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(seen.is_some(), "no progress reported");
        assert_eq!(scanning.join().unwrap().status, 200);
    });

    let done = running.api("GET", "/api/progress", "").json();
    assert!(done["message"].is_null(), "{done}");
    assert_eq!(running.send("GET", "/api/progress", &[], "").status, 401);
}
