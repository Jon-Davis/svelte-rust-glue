# svelte-rust

Build glue for embedding a [SvelteKit](https://kit.svelte.dev) frontend into a
Rust project. It builds the frontend from `build.rs`, keeps Rust ⇄ TypeScript
types in sync via [`typeshare`](https://github.com/1Password/typeshare), and
wires up `cargo:rerun-if-changed` so rebuilds happen only when they need to.

## What it does

- **`frontend(build_dir)`** — runs `npm install` (only when dependencies
  changed) and `npm run build`, then exposes the SvelteKit static-adapter output
  directory to your crate at compile time as `SVELTE_BUILD_DIR` (read it with
  `env!("SVELTE_BUILD_DIR")` instead of hardcoding the path).
- **`typescript_types(src_dirs, output, config)`** — generates TypeScript types
  from your `#[typeshare]`-annotated Rust types. It watches *only* `.rs` files
  that actually contain `#[typeshare]`, so editing unrelated Rust code doesn't
  trigger a frontend rebuild.

Both functions run from the crate root, so they assume your SvelteKit project
(`package.json`, `svelte.config.js`, `src/`, …) lives alongside `Cargo.toml`.
Rust and SvelteKit can share the same `src/` tree — even the same directories
(e.g. a `+page.svelte` next to a backend route handler). `frontend` watches
frontend sources **by extension** (`.svelte`, `.ts`, `.js`, `.mjs`, `.css`,
`.html`) plus `static/`, so colocated `.rs` files never trigger a frontend
rebuild.

## Usage

Add it as a build dependency:

```toml
# Cargo.toml
[build-dependencies]
svelte-rust = { path = "../svelte-rust" } # or a git/version reference
```

Call it from your build script:

```rust
// build.rs
fn main() {
    // Generate TS types before the frontend builds so SvelteKit picks them up.
    svelte_rust::build::typescript_types(
        &["src"],                    // Rust source dirs to scan for #[typeshare]
        "src/lib/api/generated.ts",  // generated TypeScript output
        Some("typeshare.toml"),      // optional typeshare config, or None
    );

    // Build the SvelteKit frontend; "build" is the static adapter output dir.
    svelte_rust::build::frontend("build");
}
```

At runtime, locate the built assets via the env var set at compile time:

```rust
const BUILD_DIR: &str = env!("SVELTE_BUILD_DIR");
```

## Requirements

- A Rust toolchain (edition 2024).
- `node` / `npm` on `PATH` (the crate calls `npm.cmd` on Windows, `npm`
  elsewhere).
- `typeshare` CLI, if you use `typescript_types`:
  `cargo install typeshare-cli`.
```
