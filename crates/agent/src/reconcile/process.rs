use std::time::Instant;

use snops_common::rpc::error::ReconcileError2;
use tokio::process::Child;
use tracing::error;

use super::command::NodeCommand;

/// Information about the current process
pub struct ProcessContext {
    /// The command used to start the node. If the next command is different,
    /// the node should be restarted
    pub command: NodeCommand,
    /// The child process that is running the node
    child: Child,
    /// Time the child process was started
    started_at: Instant,
    /// Time a sigint was sent to the child process
    sigint_at: Option<Instant>,
    /// Time a sigkill was sent to the child process
    sigkill_at: Option<Instant>,
}

impl ProcessContext {
    pub fn new(command: NodeCommand) -> Result<Self, ReconcileError2> {
        command
            .build()
            .spawn()
            .map(|child| Self {
                command,
                child,
                started_at: Instant::now(),
                sigint_at: None,
                sigkill_at: None,
            })
            .map_err(|e| {
                error!("failed to start node process: {e:?}");
                ReconcileError2::SpawnError(e.to_string())
            })
    }

    /// Returns true when the child process has not exited
    pub fn is_running(&self) -> bool {
        self.child.id().is_some()
    }

    /// Send a SIGINT to the child process
    pub fn send_sigint(&mut self) -> bool {
        use nix::{
            sys::signal::{self, Signal},
            unistd::Pid,
        };

        // prevent multiple sigints
        if self.sigint_at.is_some() {
            return false;
        }

        // obtain the id, or return false if the child is not running
        let Some(id) = self.child.id() else {
            return false;
        };

        // send SIGINT to the child process
        signal::kill(Pid::from_raw(id as i32), Signal::SIGINT)
            .inspect(|_| {
                // update the sigint time if the sigint was successful
                self.sigint_at = Some(Instant::now());
            })
            .is_ok()
    }

    /// Send a SIGKILL to the child process
    pub fn send_sigkill(&mut self) -> bool {
        // start_kill return Err if the process is already killed
        self.child
            .start_kill()
            .inspect(|_| {
                // update the kill time if the kill was successful
                self.sigkill_at = Some(Instant::now());
            })
            .is_ok()
    }
}
