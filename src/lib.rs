mod npm;
mod typeshare;

pub mod build {
    use std::fs;
    use std::path::Path;

    use crate::npm;
    use crate::typeshare;

    /// Project config files that affect the frontend build. Each is watched only
    /// if it exists, so projects that omit one (e.g. `tsconfig.json`) don't get a
    /// spurious `rerun-if-changed` for a missing path.
    const CONFIG_FILES: &[&str] = &[
        "svelte.config.js",
        "vite.config.js",
        "package.json",
        "package-lock.json",
        "tsconfig.json",
    ];

    /// Manifests whose modification should force a re-`npm install`.
    const INSTALL_MANIFESTS: &[&str] = &["package.json", "package-lock.json"];

    /// Frontend source extensions. Files under `src/` with these extensions are
    /// watched; `.rs` files (which may be colocated in the same directories, e.g.
    /// SvelteKit routes alongside backend route handlers) are deliberately not,
    /// so editing backend code doesn't trigger a frontend rebuild.
    const FRONTEND_EXTS: &[&str] = &["svelte", "ts", "js", "mjs", "css", "html"];

    /// Emit `cargo:rerun-if-changed` directives for all SvelteKit-related files,
    /// run `npm install` when dependencies change, and run `npm run build`.
    ///
    /// `build_dir` is the SvelteKit static adapter output directory (e.g. `"build"`).
    /// It is forwarded as `SVELTE_BUILD_DIR` at compile time so the runtime can use
    /// `env!("SVELTE_BUILD_DIR")` instead of hardcoding the path.
    ///
    /// Frontend sources under `src/` are watched **by extension** (see
    /// [`FRONTEND_EXTS`]) so that `.rs` files colocated in the same directories do
    /// not trigger a frontend rebuild. `static/`, if present, is watched wholesale.
    pub fn frontend(build_dir: &str) {
        println!("cargo:rustc-env=SVELTE_BUILD_DIR={build_dir}");

        for config in CONFIG_FILES {
            if Path::new(config).exists() {
                println!("cargo:rerun-if-changed={config}");
            }
        }

        watch_frontend(Path::new("src"));
        if Path::new("static").exists() {
            // No `.rs` lives under `static/`, so watch it wholesale — this also
            // covers images, fonts, and favicons that the build copies through.
            println!("cargo:rerun-if-changed=static");
        }

        if needs_install() {
            npm::run(&["install"]);
        }

        npm::run(&["run", "build"]);
    }

    /// Whether `npm install` should run: `node_modules` is missing, or a manifest
    /// in [`INSTALL_MANIFESTS`] is newer than `node_modules` (i.e. dependencies
    /// changed since the last install). Falls back to installing if a mtime is
    /// unavailable.
    fn needs_install() -> bool {
        let node_modules = Path::new("node_modules");
        if !node_modules.exists() {
            return true;
        }
        let Some(installed) = node_modules.metadata().and_then(|m| m.modified()).ok() else {
            return true;
        };
        INSTALL_MANIFESTS.iter().any(|manifest| {
            fs::metadata(manifest)
                .and_then(|m| m.modified())
                .is_ok_and(|mtime| mtime > installed)
        })
    }

    /// Recurse `dir`, emitting `rerun-if-changed` for each file whose extension is
    /// in [`FRONTEND_EXTS`].
    fn watch_frontend(dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                watch_frontend(&path);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| FRONTEND_EXTS.contains(&e))
            {
                println!("cargo:rerun-if-changed={}", path.display());
            }
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
