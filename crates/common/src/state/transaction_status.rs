use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::format::DataFormat;

/// Status of a transaction as presented internally for tracking and
/// preventing data loss.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum TransactionSendState {
    /// Authorization has been received. This step is skipped if a
    /// transaction is created/broadcasted directly.
    Authorized,
    /// Authorization is being executed
    Executing(DateTime<Utc>),
    /// Authorization has been executed, or a transaction is received and is
    /// pending broadcast.
    Unsent,
    /// Authorization has been broadcasted but not confirmed
    /// by the network.
    ///
    /// This step is skipped if a cannon does not re-attempt to send
    /// the transaction.
    Broadcasted(
        /// Latest height of the network at the time of the broadcast
        Option<u32>,
        /// Time of the broadcast
        DateTime<Utc>,
    ),
}

impl TransactionSendState {
    pub fn label(&self) -> &'static str {
        match self {
            TransactionSendState::Authorized => "authorized",
            TransactionSendState::Executing(_) => "executing",
            TransactionSendState::Unsent => "unsent",
            TransactionSendState::Broadcasted(_, _) => "broadcasted",
        }
    }
}

impl DataFormat for TransactionSendState {
    type Header = u8;

    const LATEST_HEADER: Self::Header = 1u8;

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        Ok(match self {
            TransactionSendState::Authorized => 0u8.write_data(writer)?,
            TransactionSendState::Executing(timestamp) => {
                1u8.write_data(writer)? + timestamp.write_data(writer)?
            }
            TransactionSendState::Unsent => 2u8.write_data(writer)?,
            TransactionSendState::Broadcasted(height, timestamp) => {
                3u8.write_data(writer)?
                    + height.write_data(writer)?
                    + timestamp.write_data(writer)?
            }
        })
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(crate::format::DataReadError::unsupported(
                "CannonTransactionStatus",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        let tag = u8::read_data(reader, &())?;
        Ok(match tag {
            0 => TransactionSendState::Authorized,
            1 => TransactionSendState::Executing(DateTime::<Utc>::read_data(reader, &())?),
            2 => TransactionSendState::Unsent,
            3 => TransactionSendState::Broadcasted(
                Option::<u32>::read_data(reader, &())?,
                DateTime::<Utc>::read_data(reader, &())?,
            ),
            _ => {
                return Err(crate::format::DataReadError::Custom(
                    "Invalid CannonTransactionStatus tag".to_string(),
                ));
            }
        })
    }
}

#[cfg(test)]
mod test {
    use chrono::DateTime;

    use super::TransactionSendState;
    use crate::format::DataFormat;

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr_2021, $b:expr_2021) => {
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
        TransactionSendState,
        TransactionSendState::Authorized,
        [0u8]
    );

    lazy_static::lazy_static! {
        static ref NOW: DateTime<chrono::Utc> = chrono::Utc::now();
    }

    case!(
        test_cannon_transaction_status_executing,
        TransactionSendState,
        TransactionSendState::Executing(*NOW),
        [vec![1u8], NOW.to_byte_vec().unwrap()].concat()
    );
    case!(
        test_cannon_transaction_status_executed,
        TransactionSendState,
        TransactionSendState::Unsent,
        [2u8]
    );
    case!(
        test_cannon_transaction_status_broadcasted,
        TransactionSendState,
        TransactionSendState::Broadcasted(Some(1), *NOW),
        [
            vec![3u8, 1u8, 1u8, 0u8, 0u8, 0u8],
            NOW.to_byte_vec().unwrap()
        ]
        .concat()
    );
}
