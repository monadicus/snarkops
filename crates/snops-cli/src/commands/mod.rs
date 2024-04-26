use anyhow::Result;
use clap::{CommandFactory, Parser};
use serde_json::Value;

use crate::cli::Cli;

mod env;
mod list;

#[derive(Debug, Parser)]
pub enum Commands {
    /// Generate shell completions.
    #[command(arg_required_else_help = true)]
    Autocomplete {
        /// Which shell you want to generate completions for.
        shell: clap_complete::Shell,
    },
    #[clap(alias = "l")]
    List(list::List),
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
            Commands::Env(env) => env.run(url, client),
            Commands::List(list) => list.run(url, client),
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
