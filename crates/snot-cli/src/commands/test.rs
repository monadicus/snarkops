use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

/// For interacting with snot tests.
#[derive(Debug, Parser)]
pub struct Test {
    /// The url the agent is on.
    #[clap(short, long, default_value = "http://localhost:1234")]
    url: url::Url,
    #[clap(subcommand)]
    command: Commands,
}

/// Test commands
#[derive(Debug, Parser)]
enum Commands {
    Start {
        /// The test spec file.
        spec: PathBuf,
    },
    Stop,
}

impl Test {
    const START_ENDPOINT: &'static str = "api/v1/test/prepare";
    const STOP_ENDPOINT: &'static str = "api/v1/test";

    pub fn run(self) -> Result<()> {
        let client = reqwest::blocking::Client::new();
        use Commands::*;
        match self.command {
            Start { spec } => {
                let file: String = std::fs::read_to_string(spec)?;
                client
                    .post(&format!("{}{}", self.url, Self::START_ENDPOINT))
                    .body(file)
                    .send()?;
                Ok(())
            }
            Stop => {
                client
                    .delete(&format!("{}{}", self.url, Self::STOP_ENDPOINT))
                    .send()?;
                Ok(())
            }
        }
    }
}
