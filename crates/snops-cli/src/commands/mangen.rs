use std::{fs::OpenOptions, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Command, CommandFactory, Parser, ValueHint};

use crate::Cli;

/// For generating snops manpages.
/// Only with the mangen feature enabled.
#[derive(Debug, Parser)]
pub struct Mangen {
    #[clap(value_hint = ValueHint::Other, default_value = "target/man/snops-cli")]
    directory: PathBuf,
}

impl Mangen {
    pub fn run(self) -> Result<()> {
        print_manpages(&self.directory, Cli::command())?;
        Ok(())
    }
}

fn print_manpages(dir: &PathBuf, cmd: Command) -> Result<()> {
    // `get_display_name()` is `Some` for all instances, except the root.
    let version = env!("CARGO_PKG_VERSION");
    let pkg_name = env!("CARGO_PKG_NAME");
    let name = cmd.get_name();
    std::fs::create_dir_all(dir).with_context(|| format!("creating {dir:?}"))?;
    let path = dir.join(format!("{name}.1"));

    let mut out = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .with_context(|| format!("opening {path:?}"))
        .map(std::io::BufWriter::new)?;
    clap_mangen::Man::new(cmd.clone())
        .title(pkg_name)
        .section("1")
        .source(format!("{pkg_name} {version}"))
        .render(&mut out)
        .with_context(|| format!("rendering {name}.1"))?;
    out.flush().context("flushing man page")?;
    drop(out);

    for subcmd in cmd.get_subcommands().filter(|c| !c.is_hide_set()) {
        let subname = format!("{}-{}", name, subcmd.get_name());
        // SAFETY: Latest clap 4 requires names are &'static - this is
        // not long-running production code, so we just leak the names here.
        let subname = &*std::boxed::Box::leak(subname.into_boxed_str());
        let subcmd = subcmd.clone().name(subname).alias(subname).version(version);
        print_manpages(dir, subcmd.clone().name(subname).version(version))?;
    }

    Ok(())
}