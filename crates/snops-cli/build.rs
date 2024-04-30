#![allow(dead_code)]

#[path = "src/main.rs"]
mod scli;

use std::io::Write;

use scli::*;

static CLI_MD_FILE: &str = "CLI_USAGE.md";

// TODO maybe replace this with an xtask as well
fn main() -> std::io::Result<()> {
    let out_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let md_content = clap_markdown::help_markdown::<scli::Cli>();
    let mut md_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(out_dir.join(CLI_MD_FILE))?;
    md_file.write_all(md_content.as_bytes())?;

    Ok(())
}
