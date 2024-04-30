use anyhow::Result;
use clap::{CommandFactory, Parser};
use serde_json::Value;

use crate::Cli;

/// The dummy value for the ids to hack around the missing required argument.
pub(crate) static DUMMY_ID: &str = "dummy_value___";

mod agent;
mod env;
#[cfg(feature = "mangen")]
mod mangen;

#[derive(Debug, Parser)]
pub enum Commands {
    /// Generate shell completions.
    #[command(arg_required_else_help = true)]
    Autocomplete {
        /// Which shell you want to generate completions for.
        shell: clap_complete::Shell,
    },
    #[clap(alias = "a")]
    Agent(agent::Agent),
    #[cfg(feature = "mangen")]
    Mangen(mangen::Mangen),
    #[clap(alias = "e")]
    Env(env::Env),
}

impl Commands {
    pub fn run(self, url: &str) -> Result<()> {
        let client = reqwest::blocking::Client::new();

        let response = match self {
            Commands::Autocomplete { shell } => {
                let mut cmd = Cli::command();
                let cmd_name = cmd.get_name().to_string();

                clap_complete::generate(shell, &mut cmd, cmd_name, &mut std::io::stdout());
                return Ok(());
            }
            Commands::Agent(agent) => agent.run(url, client),
            #[cfg(feature = "mangen")]
            Commands::Mangen(mangen) => {
                mangen.run()?;
                return Ok(());
            }
            Commands::Env(env) => env.run(url, client),
        }?;

        if !response.status().is_success() {
            eprintln!("error {}", response.status());
        }

        let value = match response.content_length() {
            Some(0) | None => None,
            _ => response.json::<Value>().map(Some)?,
        };

        println!("{}", serde_json::to_string_pretty(&value)?);

        Ok(())
    }
}
