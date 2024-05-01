#![allow(dead_code)]

use std::{env, process::Command};

use anyhow::{bail, Context, Result};
use xshell::{cmd, Shell};

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

const TASKS: &[&str] = &["help", "clipages", "manpages"];

fn try_main() -> Result<()> {
    // Ensure our working directory is the toplevel
    {
        let toplevel_path = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("Invoking git rev-parse")?;
        if !toplevel_path.status.success() {
            bail!("Failed to invoke git rev-parse");
        }
        let path = String::from_utf8(toplevel_path.stdout)?;
        std::env::set_current_dir(path.trim()).context("Changing to toplevel")?;
    }

    let task = env::args().nth(1);
    let sh = Shell::new()?;
    match task.as_deref() {
        Some("help") => print_help()?,
        Some("clipages") => clipages(&sh)?,
        Some("manpages") => manpages(&sh)?,
        _ => print_help()?,
    }

    Ok(())
}

fn clipages(sh: &Shell) -> Result<()> {
    cmd!(sh, "cargo run -p snarkos-aot --features=docpages -- md").run()?;
    cmd!(sh, "cargo run -p snops --features=docpages -- md").run()?;
    cmd!(sh, "cargo run -p snops-agent --features=docpages -- md").run()?;
    cmd!(sh, "cargo run -p snops-cli --features=docpages -- md").run()?;
    Ok(())
}

fn manpages(sh: &Shell) -> Result<()> {
    cmd!(sh, "cargo run -p snarkos-aot --features=docpages -- man").run()?;
    cmd!(sh, "cargo run -p snops --features=docpages -- man").run()?;
    cmd!(sh, "cargo run -p snops-agent --features=docpages -- man").run()?;
    cmd!(sh, "cargo run -p snops-cli --features=docpages -- man").run()?;
    Ok(())
}

fn print_help() -> Result<()> {
    println!("Tasks:");
    for name in TASKS {
        println!("  - {name}");
    }
    Ok(())
}
