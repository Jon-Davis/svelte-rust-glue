mod npm;
mod typeshare;

pub mod build {
    use std::fs;
    use std::path::Path;

    use crate::npm;
    use crate::typeshare;

    /// Emit `cargo:rerun-if-changed` directives for all SvelteKit-related files,
    /// run `npm install` when dependencies change, and run `npm run build`.
    ///
    /// `build_dir` is the SvelteKit static adapter output directory (e.g. `"build"`).
    /// It is forwarded as `SVELTE_BUILD_DIR` at compile time so the runtime can use
    /// `env!("SVELTE_BUILD_DIR")` instead of hardcoding the path.
    pub fn frontend(build_dir: &str) {
        println!("cargo:rustc-env=SVELTE_BUILD_DIR={build_dir}");

        println!("cargo:rerun-if-changed=svelte.config.js");
        println!("cargo:rerun-if-changed=vite.config.js");
        println!("cargo:rerun-if-changed=package.json");
        println!("cargo:rerun-if-changed=src/app.html");
        println!("cargo:rerun-if-changed=src/routes");

        if needs_install() {
            npm::run(&["install"]);
        }

        npm::run(&["run", "build"]);
    }

    /// Whether `npm install` should run: `node_modules` is missing, or
    /// `package.json` is newer than `node_modules` (i.e. dependencies changed
    /// since the last install). Falls back to installing if either mtime is
    /// unavailable.
    fn needs_install() -> bool {
        let node_modules = Path::new("node_modules");
        if !node_modules.exists() {
            return true;
        }
        let mtime = |p: &str| fs::metadata(p).and_then(|m| m.modified()).ok();
        match (mtime("package.json"), node_modules.metadata().and_then(|m| m.modified()).ok()) {
            (Some(pkg), Some(installed)) => pkg > installed,
            _ => true,
        }
    }

    /// Generate TypeScript types from Rust source via the `typeshare` CLI.
    ///
    /// Scans `src_dirs` and runs `typeshare`, writing TS to `output`. `config`
    /// is an optional `typeshare.toml` path (e.g. for `type_mappings`).
    ///
    /// Emits `cargo:rerun-if-changed` **only** for `.rs` files that contain a
    /// `#[typeshare]` annotation (plus `config`), so editing non-DTO Rust files
    /// does not trigger regeneration or a downstream frontend rebuild.
    pub fn typescript_types(src_dirs: &[&str], output: &str, config: Option<&str>) {
        for dir in src_dirs {
            watch_typeshared(Path::new(dir));
        }
        if let Some(config) = config {
            println!("cargo:rerun-if-changed={config}");
        }

        let mut args: Vec<String> = src_dirs.iter().map(|d| d.to_string()).collect();
        args.push("--lang=typescript".to_string());
        args.push(format!("--output-file={output}"));
        if let Some(config) = config {
            args.push(format!("--config-file={config}"));
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        typeshare::run(&args);
    }

    /// Recurse `dir`, emitting `rerun-if-changed` for each `.rs` file that
    /// contains the literal `#[typeshare` annotation.
    fn watch_typeshared(dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                watch_typeshared(&path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
                && fs::read_to_string(&path)
                    .map(|src| src.contains("#[typeshare"))
                    .unwrap_or(false)
            {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }

}
