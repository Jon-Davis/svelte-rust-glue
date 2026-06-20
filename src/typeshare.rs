use std::io::ErrorKind;
use std::process::Command;

pub const TYPESHARE: &str = "typeshare";

pub fn run(args: &[&str]) {
    let status = Command::new(TYPESHARE).args(args).status().unwrap_or_else(|e| {
        if e.kind() == ErrorKind::NotFound {
            panic!(
                "`{TYPESHARE}` not found — install it with `cargo install typeshare-cli`"
            );
        }
        panic!("failed to execute `{TYPESHARE} {}`: {e}", args.join(" "));
    });
    assert!(
        status.success(),
        "`{TYPESHARE} {}` exited with {status}",
        args.join(" ")
    );
}
