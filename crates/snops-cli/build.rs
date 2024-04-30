#![allow(dead_code)]

use clap::Command;
use std::{fs::File, io::Write, path::Path};

#[path = "src/main.rs"]
mod scli;

use scli::*;

static CLI_MD_FILE: &str = "CLI_USAGE.md";

fn print_manpages(dir: &Path, app: &Command, parent_name: Option<String>) -> std::io::Result<()> {
    // `get_display_name()` is `Some` for all instances, except the root.
    let name = app.get_display_name().unwrap_or_else(|| app.get_name());
    let file_name = match &parent_name {
        Some(parent) if !parent.is_empty() => format!("{parent}-{name}.1"),
        _ => format!("{name}.1"),
    };

    let mut out = File::create(dir.join(file_name))?;

    clap_mangen::Man::new(app.clone()).render(&mut out)?;
    out.flush()?;

    let new_parent_name = match &parent_name {
        Some(parent) if !parent.is_empty() => format!("{parent}-{name}"),
        _ => name.to_string(),
    };
    for sub in app.get_subcommands() {
        print_manpages(dir, sub, Some(new_parent_name.clone()))?;
    }

    Ok(())
}

fn main() -> std::io::Result<()> {
    let out_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let md_content = clap_markdown::help_markdown::<scli::Cli>();
    let mut md_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(out_dir.join(CLI_MD_FILE))?;
    md_file.write_all(md_content.as_bytes())?;

    // TODO lets not have this in the build script
    // out_dir.push("man");
    // std::fs::create_dir_all(&out_dir)?;

    // let cmd = scli::Cli::command();
    // print_manpages(out_dir.as_path(), &cmd, None)?;

    Ok(())
}
