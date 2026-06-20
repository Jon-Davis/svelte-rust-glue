use std::process::Command;

#[cfg(windows)]
pub const NPM: &str = "npm.cmd";
#[cfg(not(windows))]
pub const NPM: &str = "npm";

pub fn run(args: &[&str]) {
    let status = Command::new(NPM)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("failed to execute `{NPM} {}`: {e}", args.join(" ")));
    assert!(
        status.success(),
        "`{NPM} {}` exited with {status}",
        args.join(" ")
    );
}
