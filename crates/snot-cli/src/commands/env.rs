use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use serde_json::Value;

/// For interacting with snot tests.
#[derive(Debug, Parser)]
pub struct Env {
    /// The url the agent is on.
    #[clap(short, long, default_value = "http://localhost:1234")]
    url: url::Url,
    #[clap(subcommand)]
    command: Commands,
}

/// Env commands
#[derive(Debug, Parser)]
enum Commands {
    /// Start a Env.
    Start {
        /// The test spec file.
        spec: PathBuf,
    },
    /// Stop a Env(s).
    Stop {
        /// Stop all envs.
        // #[clap(short, long)]
        // all: bool,
        /// Stop a specific test.
        id: usize,
    },
}

impl Env {
    const START_ENDPOINT: &'static str = "api/v1/env/prepare";
    const STOP_ENDPOINT: &'static str = "api/v1/env/";

    pub fn run(self) -> Result<()> {
        let client = reqwest::blocking::Client::new();
        use Commands::*;
        match self.command {
            Start { spec } => {
                let file: String = std::fs::read_to_string(spec)?;
                let id: Value = client
                    .post(format!("{}{}", self.url, Self::START_ENDPOINT))
                    .body(file)
                    .send()?
                    .json()?;
                println!("{}", serde_json::to_string(&id)?);
                Ok(())
            }
            Stop { id } => {
                client
                    .delete(format!("{}{}{id}", self.url, Self::STOP_ENDPOINT))
                    .send()?;
                Ok(())
            }
        }
    }
}
