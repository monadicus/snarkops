use snops_common::{
    format::{DataFormat, PackedUint},
    state::AgentId,
};
use tokio::sync::mpsc::Sender;

pub struct TransactionStatusSender(Option<Sender<TransactionStatus>>);

impl TransactionStatusSender {
    pub fn new(sender: Sender<TransactionStatus>) -> Self {
        Self(Some(sender))
    }

    pub fn empty() -> Self {
        Self(None)
    }

    pub fn send(&self, status: TransactionStatus) {
        if let Some(sender) = &self.0 {
            let _ = sender.try_send(status);
        }
    }
}

pub enum TransactionStatus {
    /// Authorization has been aborted
    ExecuteAborted,
    /// Authorization has been queued for execution.
    ExecuteQueued,
    /// No agents are available for the execution
    ExecuteAwaitingCompute,
    /// An agent was found and the authorization is being executed
    Executing(AgentId),
    /// Execute RPC failed
    ExecuteFailed(String),
    /// Agent has completed the execution
    ExecuteComplete(String),
    // TODO: Implement the following statuses
    // /// API has received the transaction broadcast
    // BroadcastReceived,
    // /// Control plane has forwarded the transaction to a peer
    // BroadcastForwarded,
    // /// An error occurred while broadcasting the transaction
    // BroadcastFailed,
    // /// Transaction was found in the network, return the block hash
    // TransactionConfirmed(String),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CannonTransactionStatus {
    /// Authorization has been received. This step is skipped if a
    /// transaction is created/broadcasted directly.
    Authorized,
    /// Authorization is being executed
    Executing(AgentId),
    /// Authorization has been executed and is pending broadcast.
    Executed,
    /// Authorization has been broadcasted but not confirmed
    /// by the network.
    ///
    /// This step is skipped if a cannon does not re-attempt to send
    /// the transaction.
    Broadcasted {
        /// Latest height of the network at the time of the broadcast
        height: Option<u32>,
        /// Number of tries to broadcast the transaction
        tries: usize,
    },
}

impl DataFormat for CannonTransactionStatus {
    type Header = u8;

    const LATEST_HEADER: Self::Header = 1u8;

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, snops_common::format::DataWriteError> {
        Ok(match self {
            CannonTransactionStatus::Authorized => 0u8.write_data(writer)?,
            CannonTransactionStatus::Executing(agent_id) => {
                1u8.write_data(writer)? + agent_id.write_data(writer)?
            }
            CannonTransactionStatus::Executed => 2u8.write_data(writer)?,
            CannonTransactionStatus::Broadcasted { height, tries } => {
                3u8.write_data(writer)?
                    + height.write_data(writer)?
                    + PackedUint::from(*tries).write_data(writer)?
            }
        })
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, snops_common::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(snops_common::format::DataReadError::unsupported(
                "CannonTransactionStatus",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        let tag = u8::read_data(reader, &())?;
        Ok(match tag {
            0 => CannonTransactionStatus::Authorized,
            1 => CannonTransactionStatus::Executing(AgentId::read_data(reader, &())?),
            2 => CannonTransactionStatus::Executed,
            3 => CannonTransactionStatus::Broadcasted {
                height: Option::<u32>::read_data(reader, &())?,
                tries: PackedUint::read_data(reader, &())?.into(),
            },
            _ => {
                return Err(snops_common::format::DataReadError::Custom(
                    "Invalid CannonTransactionStatus tag".to_string(),
                ))
            }
        })
    }
}

#[cfg(test)]
mod test {
    use snops_common::{format::DataFormat, state::AgentId};

    use crate::cannon::status::CannonTransactionStatus;

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr, $b:expr) => {
            #[test]
            fn $name() {
                let mut data = Vec::new();
                let value: $ty = $a;
                value.write_data(&mut data).unwrap();
                // we're not doing an assert here because
                // the order of the elements in the collection is not guaranteed
                assert_eq!(data, $b);

                let mut reader = &data[..];
                let read_value =
                    <$ty>::read_data(&mut reader, &<$ty as DataFormat>::LATEST_HEADER).unwrap();
                assert_eq!(read_value, value);
            }
        };
    }

    case!(
        test_cannon_transaction_status_received,
        CannonTransactionStatus,
        CannonTransactionStatus::Authorized,
        [0u8]
    );
    case!(
        test_cannon_transaction_status_executing,
        CannonTransactionStatus,
        CannonTransactionStatus::Executing(AgentId::default()),
        [vec![1u8], AgentId::default().to_byte_vec().unwrap()].concat()
    );
    case!(
        test_cannon_transaction_status_executed,
        CannonTransactionStatus,
        CannonTransactionStatus::Executed,
        [2u8]
    );
    case!(
        test_cannon_transaction_status_broadcasted,
        CannonTransactionStatus,
        CannonTransactionStatus::Broadcasted {
            height: Some(1),
            tries: 2
        },
        [3u8, 1u8, 1u8, 2u8]
    );
}
