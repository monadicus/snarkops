use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueHint};
use serde_json::Value;

/// For interacting with snop environments.
#[derive(Debug, Parser)]
pub struct Env {
    /// The url the control plane is on.
    #[clap(short, long, default_value = "http://localhost:1234", value_hint = ValueHint::Url)]
    url: String,
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// Prepare a (test) environment.
    #[command(arg_required_else_help = true)]
    Prepare {
        /// The test spec file.
        #[clap(value_hint = ValueHint::AnyPath)]
        spec: PathBuf,
    },

    /// Start an environment's timeline (a test).
    #[command(arg_required_else_help = true)]
    Start {
        /// Start a specific env.
        #[clap(value_hint = ValueHint::Other)]
        id: usize,
    },

    /// Stop an environment's timeline.
    #[command(arg_required_else_help = true)]
    Stop {
        /// Stop a specific env.
        #[clap(value_hint = ValueHint::Other)]
        id: usize,
    },
}

impl Env {
    pub fn run(self) -> Result<()> {
        let client = reqwest::blocking::Client::new();

        use Commands::*;
        let response = match self.command {
            Prepare { spec } => {
                let ep = format!("{}/api/v1/env/prepare", self.url);
                let file: String = std::fs::read_to_string(spec)?;

                client.post(ep).body(file).send()?
            }

            Start { id } => {
                let ep = format!("{}/api/v1/env/{id}", self.url);

                client.post(ep).send()?
            }

            Stop { id } => {
                let ep = format!("{}/api/v1/env/{id}", self.url);

                client.delete(ep).send()?
            }
        };

        let value = match response.content_length() {
            Some(0) | None => None,
            _ => response.json::<Value>().map(Some)?,
        };

        println!("{}", serde_json::to_string_pretty(&value)?);

        Ok(())
    }
}
