use std::time::{Duration, Instant};

use snops_common::{
    rpc::error::ReconcileError,
    state::{ReconcileCondition, ReconcileStatus},
};
use tokio::{process::Child, select};
use tracing::{error, info};

use super::{command::NodeCommand, Reconcile};
use crate::state::NODE_GRACEFUL_SHUTDOWN_TIMEOUT;

/// Information about the current process
pub struct ProcessContext {
    /// The command used to start the node. If the next command is different,
    /// the node should be restarted
    pub command: NodeCommand,
    /// The child process that is running the node
    pub child: Child,
    /// Time the child process was started
    #[allow(dead_code)]
    started_at: Instant,
    /// Time a sigint was sent to the child process
    sigint_at: Option<Instant>,
    /// Time a sigkill was sent to the child process
    sigkill_at: Option<Instant>,
}

impl ProcessContext {
    pub fn new(command: NodeCommand) -> Result<Self, ReconcileError> {
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
                ReconcileError::SpawnError(e.to_string())
            })
    }

    /// Returns true when the child process has not exited
    pub fn is_running(&mut self) -> bool {
        // This code is mutable because try_wait modifies the Child. Without
        // mutability, the current running status would never be updated.
        self.child.try_wait().is_ok_and(|status| status.is_none())
    }

    /// A helper function to gracefully shutdown the node process without
    /// a reconciler
    pub async fn graceful_shutdown(&mut self) {
        if !self.is_running() {
            return;
        }

        self.send_sigint();

        select! {
            _ = tokio::time::sleep(NODE_GRACEFUL_SHUTDOWN_TIMEOUT) => {
                info!("Sending SIGKILL to node process");
                self.send_sigkill();
            },
            _ = tokio::signal::ctrl_c() => {
                info!("Received SIGINT, sending SIGKILL to node process");
                self.send_sigkill();
            },
            _ = self.child.wait() => {
                info!("Node process has exited gracefully");
                return;
            }
        }

        let _ = self.child.wait().await;
        info!("Node process has exited");
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

/// The EndProcessReconciler will return true when the child process has exited.
/// It will wait NODE_GRACEFUL_SHUTDOWN_TIMEOUT seconds after sending a SIGINT
/// before sending a SIGKILL (if the childi process has not exited),
pub struct EndProcessReconciler<'a>(pub &'a mut ProcessContext);

impl<'a> Reconcile<(), ReconcileError> for EndProcessReconciler<'a> {
    async fn reconcile(&mut self) -> Result<ReconcileStatus<()>, ReconcileError> {
        if !self.0.is_running() {
            return Ok(ReconcileStatus::default());
        }

        let Some(sigint_at) = self.0.sigint_at else {
            if self.0.send_sigint() {
                info!("Sent SIGINT to node process");
            }
            return Ok(ReconcileStatus::empty()
                .add_condition(ReconcileCondition::PendingShutdown)
                .requeue_after(Duration::from_secs(1)));
        };

        if sigint_at.elapsed() > NODE_GRACEFUL_SHUTDOWN_TIMEOUT
            && self.0.sigkill_at.is_none()
            && self.0.send_sigkill()
        {
            info!("Sent SIGKILL to node process");
        }

        Ok(ReconcileStatus::empty()
            .add_condition(ReconcileCondition::PendingShutdown)
            .requeue_after(Duration::from_secs(1)))
    }
}
