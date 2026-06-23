# svelte-rust

Build glue for a Rust web service that serves a [SvelteKit](https://kit.svelte.dev)
frontend and shares typed contracts with it via OpenAPI. It builds the frontend
from `build.rs`, regenerates TypeScript types from your OpenAPI spec, and wires up
`cargo:rerun-if-changed` so rebuilds happen only when they need to.

It's an opinionated build script for one architecture — a single Rust binary
(e.g. Axum) serving a prerendered SvelteKit bundle, with the Rust ↔ TypeScript
contract flowing **Rust DTOs → `openapi.json` → generated client/types → Svelte**.
If that's your shape, a plain `cargo build` produces everything.

## What it does

- **`Frontend::new(root, build_dir).…run()`** — the main entry point. Root-aware
  and content-gated: it works when the SvelteKit tree does **not** sit beside the
  build script's `Cargo.toml` (e.g. a `server/` sub-crate driving a frontend at
  the workspace root), and it stays fast when the build script reruns far more
  often than the frontend changes — e.g. a script that links a library as a
  *build-dependency*, so every backend `.rs` edit forces a rerun that
  `rerun-if-changed` can't suppress. It hashes the frontend inputs and skips
  `npm` when nothing moved.
  - **`.openapi(spec, json, out_dir)`** — writes the serialized OpenAPI `spec` to
    `json` (only when it changed) and regenerates a typed client + types into
    `out_dir` with [`openapi-ts`](https://heyapi.dev) (`@hey-api/openapi-ts`,
    config-driven via a project-root `openapi-ts.config.ts`). The caller produces
    the spec string (it needs the route tree); the spec is folded into the gate,
    and `out_dir` is excluded from it (it's derived purely from `json`, so hashing
    it would loop).
- **`frontend(build_dir)`** — a simpler, CWD-relative helper: `npm install` (only
  when dependencies changed) + `npm run build`, exposing the static-adapter output
  dir as `SVELTE_BUILD_DIR` at compile time. Use it when your SvelteKit project
  lives directly alongside `Cargo.toml` and you don't need the hash gate or
  OpenAPI step.

Rust and SvelteKit can share the same `src/` tree — even the same directories
(e.g. a `+page.svelte` next to a backend route handler). Frontend sources are
watched **by extension** (`.svelte`, `.ts`, `.js`, `.mjs`, `.css`, `.html`) plus
`static/`, so colocated `.rs` files never trigger a frontend rebuild.

## Usage

Add it as a build dependency:

```toml
# Cargo.toml
[build-dependencies]
svelte-rust = { git = "https://github.com/Jon-Davis/svelte-rust-glue.git" } # or a path/version reference
```

Call it from your build script. Producing the spec is the only app-specific part
(it needs your route tree); typically the route tree lives in a library that this
binary crate also links as a build-dependency, so the spec is available here:

```rust
// build.rs
fn main() {
    let root = /* workspace root where package.json lives */;
    let spec = my_app::openapi_document()   // your utoipa document
        .to_pretty_json()
        .expect("serialize OpenAPI");

    svelte_rust::build::Frontend::new(&root, "build") // "build" = static adapter output dir
        .openapi(spec, "openapi.json", "src/lib/api/gen") // out_dir matches openapi-ts.config.ts
        .run();
}
```

At runtime, locate the built assets via the env var set at compile time (when the
build script and the serving crate are the same crate):

```rust
const BUILD_DIR: &str = env!("SVELTE_BUILD_DIR");
```

> If the build script lives in a different crate than the one that serves the
> bundle (e.g. a `build.rs` in `server/` but a `fallback` in a shared lib), a
> `cargo:rustc-env` var won't cross the crate boundary — share the directory as a
> plain `const` instead and pass it to `Frontend::new`.

## Requirements

- A Rust toolchain (edition 2024).
- `node` / `npm` on `PATH` (the crate calls `npm.cmd` on Windows, `npm`
  elsewhere).
- [`@hey-api/openapi-ts`](https://heyapi.dev) as a project devDependency, plus an
  `openapi-ts.config.ts` at the project root, if you use `.openapi(...)` — it's run
  via `npm exec -- openapi-ts`.
