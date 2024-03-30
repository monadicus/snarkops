use anyhow::Result;
use clap::{CommandFactory, Parser};

use crate::cli::Cli;

mod env;

#[derive(Debug, Parser)]
pub enum Commands {
    /// Generate shell completions.
    Autocomplete {
        /// Which shell you want to generate completions for.
        shell: clap_complete::Shell,
    },
    Env(env::Env),
}

impl Commands {
    pub fn run(self) -> Result<()> {
        match self {
            Commands::Autocomplete { shell } => {
                let mut cmd = Cli::command();
                let cmd_name = cmd.get_name().to_string();

                clap_complete::generate(shell, &mut cmd, cmd_name, &mut std::io::stdout());
                Ok(())
            }
            Commands::Env(test) => test.run(),
        }
    }
}
