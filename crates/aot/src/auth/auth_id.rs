use anyhow::{bail, Result};
use snarkvm::{console::types::Field, ledger::block::Transaction};

use crate::{Authorization, Network};

// convert a fee authorization to a real (fake) fee :)
pub fn fee_from_auth<N: Network>(
    fee_auth: &Authorization<N>,
) -> Result<snarkvm::ledger::block::Fee<N>> {
    let Some(transition) = fee_auth.transitions().values().next().cloned() else {
        bail!("No transitions found in fee authorization");
    };
    snarkvm::ledger::block::Fee::from(transition, N::StateRoot::default(), None)
}

/// compute the transaction ID for an authorization using the transitions and
/// fee authorization
pub fn auth_tx_id<N: Network>(
    auth: &Authorization<N>,
    fee_auth: Option<&Authorization<N>>,
) -> Result<N::TransactionID> {
    let fee = fee_auth.map(fee_from_auth).transpose()?;

    let field: Field<N> = *Transaction::transitions_tree(auth.transitions().values(), &fee)?.root();

    Ok(field.into())
}

/// compute the transaction ID for a deployment using the deployment and fee
pub fn deploy_tx_id<N: Network>(
    deployment: &snarkvm::ledger::block::Deployment<N>,
    fee_auth: Option<&Authorization<N>>,
) -> Result<N::TransactionID> {
    let fee = fee_auth.map(fee_from_auth).transpose()?;

    let field: Field<N> = *Transaction::deployment_tree(deployment, fee.as_ref())?.root();

    Ok(field.into())
}
