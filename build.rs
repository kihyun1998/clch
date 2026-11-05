//! Builds the dashboard's UI where its sources are here, and embeds `ui/dist`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let ui = root.join("ui");
    let dist = ui.join("dist");
    if ui.join("package.json").is_file() {
        for watched in [
            "src",
            "index.html",
            "package.json",
            "package-lock.json",
            "tsconfig.json",
            "vite.config.ts",
            "dist/index.html",
        ] {
            println!("cargo:rerun-if-changed=ui/{watched}");
        }
        build_ui(&ui, &dist);
    } else {
        println!("cargo:rerun-if-changed=ui/dist");
    }
    if !dist.join("index.html").is_file() {
        panic!("no built dashboard UI in {}", dist.display());
    }
    let mut files = Vec::new();
    collect(&dist, &mut files);
    files.sort();
    let mut code = String::from("static ASSETS: &[(&str, &[u8])] = &[\n");
    for file in &files {
        let name = file.strip_prefix(&dist).unwrap();
        let name: Vec<String> = name
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        writeln!(
            code,
            "    ({:?}, include_bytes!({:?})),",
            name.join("/"),
            file
        )
        .unwrap();
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("ui_assets.rs");
    std::fs::write(out, code).unwrap();
}

fn build_ui(ui: &Path, dist: &Path) {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let run = |args: &[&str]| Command::new(npm).args(args).current_dir(ui).status();
    let outcome = if needs_install(ui) {
        run(&["ci"]).and_then(|status| {
            if status.success() {
                run(&["run", "build"])
            } else {
                Ok(status)
            }
        })
    } else {
        run(&["run", "build"])
    };
    match outcome {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("building the dashboard UI failed: npm ended with {status}"),
        Err(e) if dist.join("index.html").is_file() => {
            println!("cargo:warning=using the UI already in ui/dist, as npm could not run: {e}");
        }
        Err(e) => panic!("building the dashboard UI needs Node.js and npm: {e}"),
    }
}

/// Whether `node_modules` is missing, or older than the lock file.
fn needs_install(ui: &Path) -> bool {
    let modified = |path: PathBuf| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let installed = modified(ui.join("node_modules").join(".package-lock.json"));
    let locked = modified(ui.join("package-lock.json"));
    match (installed, locked) {
        (Some(installed), Some(locked)) => installed < locked,
        _ => true,
    }
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else {
            files.push(path);
        }
    }
}
