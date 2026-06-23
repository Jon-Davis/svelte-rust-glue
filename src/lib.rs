mod npm;

pub mod build {
    use std::collections::hash_map::DefaultHasher;
    use std::fs;
    use std::hash::{Hash, Hasher};
    use std::path::{Path, PathBuf};

    use crate::npm;

    /// Project config files that affect the frontend build. Each is watched only
    /// if it exists, so projects that omit one (e.g. `tsconfig.json`) don't get a
    /// spurious `rerun-if-changed` for a missing path.
    const CONFIG_FILES: &[&str] = &[
        "svelte.config.js",
        "vite.config.js",
        "vite.config.ts",
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

        if needs_install(Path::new(".")) {
            npm::run(&["install"]);
        }

        npm::run(&["run", "build"]);
    }

    /// An OpenAPI spec to materialise and turn into a typed client/types before
    /// the Svelte build. Configured via [`Frontend::openapi`].
    struct OpenApi {
        /// The serialized spec (e.g. utoipa's `to_pretty_json()` output).
        spec: String,
        /// Where to write the spec, relative to the project root (e.g.
        /// `openapi.json`). Committed, and consumed by `openapi-ts`.
        json: PathBuf,
        /// Directory `openapi-ts` writes the generated SDK + types into, relative
        /// to the project root (e.g. `src/lib/api/gen`). Excluded from the change
        /// hash since it's derived purely from the spec.
        out_dir: PathBuf,
    }

    /// Root-aware, content-gated build for a Rust web service that serves a
    /// SvelteKit frontend and shares typed contracts with it via OpenAPI.
    ///
    /// Built for projects where the SvelteKit tree does **not** sit beside the
    /// build script's `Cargo.toml` (e.g. a `server/` sub-crate whose `build.rs`
    /// drives a frontend at the workspace root), and for build scripts that rerun
    /// far more often than the frontend actually changes — most notably when the
    /// script links a library as a *build-dependency*, so any backend `.rs` edit
    /// recompiles that dep and forces the script to rerun. Plain
    /// `cargo:rerun-if-changed` can't suppress that, so this gates the `npm` steps
    /// behind a content hash of the frontend inputs and no-ops when nothing moved.
    ///
    /// With [`openapi`](Frontend::openapi) set, `run` writes the spec to disk and
    /// regenerates the matching typed client + types (via `openapi-ts`,
    /// i.e. `@hey-api/openapi-ts`) before building — the contract flows Rust DTOs →
    /// `openapi.json` → generated SDK → Svelte in a single `cargo build`. The
    /// caller produces the spec string (it needs the route tree); everything
    /// downstream is owned here. The generator is config-driven: a project-root
    /// `openapi-ts.config.ts` declares the input/output (the `out_dir` passed here
    /// must match its `output`, and is what gets excluded from the change hash).
    ///
    /// ```no_run
    /// let spec = my_app::openapi_document().to_pretty_json().unwrap();
    /// svelte_rust::build::Frontend::new(workspace_root, "build")
    ///     .openapi(spec, "openapi.json", "src/lib/api/gen")
    ///     .run();
    /// ```
    pub struct Frontend {
        root: PathBuf,
        build_dir: String,
        openapi: Option<OpenApi>,
        skip_env: String,
    }

    impl Frontend {
        /// `root` is the SvelteKit project root (where `package.json` lives);
        /// `build_dir` is the static-adapter output dir relative to `root`.
        pub fn new(root: impl Into<PathBuf>, build_dir: impl Into<String>) -> Self {
            Self {
                root: root.into(),
                build_dir: build_dir.into(),
                openapi: None,
                skip_env: "SKIP_FRONTEND_BUILD".to_string(),
            }
        }

        /// Materialise an OpenAPI spec and derive a typed client + types from it.
        ///
        /// `spec` is the serialized document (the caller builds it from its route
        /// tree). `json` and `out_dir` are paths relative to the project root. On
        /// `run`: the spec is written to `json` (only when it changed, to keep the
        /// working tree clean) and folded into the change-detection hash; then,
        /// inside the gated section, `openapi-ts` regenerates `out_dir` from it.
        /// `out_dir` is excluded from the hash since it's derived purely from the
        /// spec — hashing it would loop (generate → hash changes → regenerate → …).
        pub fn openapi(
            mut self,
            spec: impl Into<String>,
            json: impl Into<PathBuf>,
            out_dir: impl Into<PathBuf>,
        ) -> Self {
            self.openapi = Some(OpenApi {
                spec: spec.into(),
                json: json.into(),
                out_dir: out_dir.into(),
            });
            self
        }

        /// Override the env var that force-skips the npm steps (default
        /// `SKIP_FRONTEND_BUILD`), e.g. for a Rust-only CI job with no Node. The
        /// OpenAPI spec is still written (it needs no Node); only the npm/Svelte
        /// steps are skipped.
        pub fn skip_env(mut self, var: impl Into<String>) -> Self {
            self.skip_env = var.into();
            self
        }

        /// Write the OpenAPI spec (if any), emit rerun directives, then — unless
        /// skipped or unchanged — `npm install` → `openapi-typescript` → `npm run
        /// build`.
        pub fn run(self) {
            println!("cargo:rustc-env=SVELTE_BUILD_DIR={}", self.build_dir);

            // Write openapi.json first, before any gate or skip: it's pure Rust
            // (no Node needed) and keeps the committed spec current even on a
            // SKIP_FRONTEND_BUILD / no-frontend-change build. Only write on change
            // so the working tree's mtime stays clean.
            if let Some(api) = &self.openapi {
                let path = self.root.join(&api.json);
                if fs::read_to_string(&path).unwrap_or_default() != api.spec {
                    fs::write(&path, &api.spec).expect("write OpenAPI json");
                }
            }

            for config in CONFIG_FILES {
                let p = self.root.join(config);
                if p.exists() {
                    println!("cargo:rerun-if-changed={}", p.display());
                }
            }
            watch_frontend(&self.root.join("src"));
            let static_dir = self.root.join("static");
            if static_dir.exists() {
                println!("cargo:rerun-if-changed={}", static_dir.display());
            }
            println!("cargo:rerun-if-env-changed={}", self.skip_env);

            if std::env::var_os(&self.skip_env).is_some() {
                println!(
                    "cargo:warning={} set — skipping npm/svelte build",
                    self.skip_env
                );
                return;
            }

            // --- Gate: hash frontend inputs; skip npm when nothing changed. ---
            let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR set by cargo"));
            let stamp = out_dir.join("svelte-rust.stamp");
            let generated: Vec<PathBuf> = self
                .openapi
                .as_ref()
                .map(|api| vec![self.root.join(&api.out_dir)])
                .unwrap_or_default();
            let mut hasher = DefaultHasher::new();
            if let Some(api) = &self.openapi {
                api.spec.hash(&mut hasher);
            }
            hash_tree(&self.root.join("src"), &generated, &mut hasher);
            for f in CONFIG_FILES {
                hash_file(&self.root.join(f), &mut hasher);
            }
            let current = format!("{:x}", hasher.finish());
            let previous = fs::read_to_string(&stamp).unwrap_or_default();
            let bundle_ok = self.root.join(&self.build_dir).join("index.html").exists();

            if current != previous || !bundle_ok {
                ensure_install(&self.root, &out_dir);
                if self.openapi.is_some() {
                    // `npm exec` runs the project-local `openapi-ts` binary, which
                    // reads `openapi-ts.config.ts` (input/output) from the project
                    // root — so the glue owns the spec→client step without depending
                    // on the project naming an npm script for it.
                    npm::run_in(&self.root, &["exec", "--", "openapi-ts"]);
                }
                npm::run_in(&self.root, &["run", "build"]);
                let _ = fs::write(&stamp, &current);
            } else {
                // Plain println (hidden unless `-vv`) so a normal build stays quiet.
                println!("frontend unchanged — skipping npm build");
            }
        }
    }

    /// Run `npm install` only when it's actually needed: `node_modules` is
    /// missing, or the lockfile *content* changed since the last install (tracked
    /// by a stamp in `out_dir`). Crucially this is **content**-based, not
    /// mtime-based: a no-op `npm install` rewrites `package-lock.json`'s mtime
    /// without changing its bytes, and an mtime check would therefore reinstall on
    /// every build — which re-touches the lockfile and loops forever.
    fn ensure_install(root: &Path, out_dir: &Path) {
        let mut hasher = DefaultHasher::new();
        for manifest in INSTALL_MANIFESTS {
            if let Ok(bytes) = fs::read(root.join(manifest)) {
                bytes.hash(&mut hasher);
            }
        }
        let want = format!("{:x}", hasher.finish());
        let stamp = out_dir.join("svelte-rust-install.stamp");
        let have = fs::read_to_string(&stamp).unwrap_or_default();
        if !root.join("node_modules").exists() || have != want {
            npm::run_in(root, &["install"]);
            let _ = fs::write(&stamp, &want);
        }
    }

    /// Whether `npm install` should run: `node_modules` is missing, or a manifest
    /// in [`INSTALL_MANIFESTS`] is newer than `node_modules` (i.e. dependencies
    /// changed since the last install). Falls back to installing if a mtime is
    /// unavailable. Paths are resolved relative to `root`. Used by the legacy
    /// [`frontend`] entry point; [`Frontend`] uses [`ensure_install`] instead.
    fn needs_install(root: &Path) -> bool {
        let node_modules = root.join("node_modules");
        if !node_modules.exists() {
            return true;
        }
        let Some(installed) = node_modules.metadata().and_then(|m| m.modified()).ok() else {
            return true;
        };
        INSTALL_MANIFESTS.iter().any(|manifest| {
            fs::metadata(root.join(manifest))
                .and_then(|m| m.modified())
                .is_ok_and(|mtime| mtime > installed)
        })
    }

    /// Fold every **frontend** file under `dir` (path + content) into the hasher,
    /// skipping any path at or under an `exclude` entry (so a whole generated
    /// directory can be excluded, not just a single file). Only files whose
    /// extension is in [`FRONTEND_EXTS`] are hashed — `.rs` files colocated in the
    /// shared `src/` tree are ignored, mirroring [`watch_frontend`], so a
    /// backend-only edit (which can still rerun this script via a build-dependency
    /// recompile) doesn't needlessly rebuild the frontend. API changes that affect
    /// the frontend flow through `openapi.json`, which is hashed separately.
    /// Content-based (not mtime) so npm steps that rewrite a file without changing
    /// its bytes — e.g. a no-op `npm install` touching `package-lock.json` — don't
    /// spuriously invalidate the gate.
    fn hash_tree(dir: &Path, exclude: &[PathBuf], hasher: &mut DefaultHasher) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort(); // deterministic order
        for path in paths {
            if exclude.iter().any(|ex| path.starts_with(ex)) {
                continue;
            }
            if path.is_dir() {
                hash_tree(&path, exclude, hasher);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| FRONTEND_EXTS.contains(&e))
            {
                hash_file(&path, hasher);
            }
        }
    }

    fn hash_file(path: &Path, hasher: &mut DefaultHasher) {
        if let Ok(bytes) = fs::read(path) {
            path.to_string_lossy().hash(hasher);
            bytes.hash(hasher);
        }
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
}
