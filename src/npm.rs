use std::path::Path;
use std::process::Command;

#[cfg(windows)]
pub const NPM: &str = "npm.cmd";
#[cfg(not(windows))]
pub const NPM: &str = "npm";

pub fn run(args: &[&str]) {
    exec(Command::new(NPM).args(args), args);
}

/// Like [`run`], but executes with `dir` as the working directory. Needed when
/// the SvelteKit project does not live alongside the crate's `Cargo.toml` (e.g.
/// the build script belongs to a `server/` sub-crate while `package.json` sits
/// at the workspace root).
pub fn run_in(dir: &Path, args: &[&str]) {
    exec(Command::new(NPM).current_dir(dir).args(args), args);
}

fn exec(cmd: &mut Command, args: &[&str]) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to execute `{NPM} {}`: {e}", args.join(" ")));
    assert!(
        status.success(),
        "`{NPM} {}` exited with {status}",
        args.join(" ")
    );
}
