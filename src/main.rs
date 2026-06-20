mod npm;

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("build") => npm_build(),
        _ => {
            eprintln!("Usage: svelte-rust <command> [options]");
            eprintln!();
            eprintln!("Commands:");
            eprintln!("  build  npm install + npm run build");
            std::process::exit(1);
        }
    }
}

fn npm_build() {
    npm::run(&["install"]);
    npm::run(&["run", "build"]);
}
