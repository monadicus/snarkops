use std::{fs::OpenOptions, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, ValueHint};

/// For generating cli markdown.
/// Only with the clipages feature enabled.
#[derive(Debug, Parser)]
pub struct Clipages {
    #[clap(value_hint = ValueHint::Other, default_value = "snops_book/user_guide/clis")]
    directory: PathBuf,
}

impl Clipages {
    pub fn run<T: CommandFactory>(self, pkg_name: &'static str) -> Result<()> {
        std::fs::create_dir_all(&self.directory)
            .with_context(|| format!("creating {:?}", self.directory))?;

        let pkg_name = pkg_name.to_string().to_uppercase().replace('-', "_");
        let file_path = self.directory.join(format!("{}.md", pkg_name));
        let mut md_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(file_path)
            .with_context(|| format!("opening {:?}", self.directory))
            .map(std::io::BufWriter::new)?;
        let md_content = clap_markdown::help_markdown::<T>();

        md_file.write_all(md_content.as_bytes())?;
        Ok(())
    }
}
