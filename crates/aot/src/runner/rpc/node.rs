#![allow(dead_code)]

use std::sync::{Arc, RwLock};

use snarkvm::ledger::store::{helpers::rocksdb::BlockDB, BlockStorage};
use snops_common::{
    define_rpc_mux,
    rpc::{
        agent::{
            node::{NodeService, NodeServiceRequest, NodeServiceResponse},
            AgentNodeServiceRequest, AgentNodeServiceResponse,
        },
        error::AgentError,
    },
    state::snarkos_status::SnarkOSLiteBlock,
};
use tarpc::context;

use crate::{
    cli::{make_env_filter, ReloadHandler},
    runner::rpc::get_block_info_for_height,
    Network,
};

define_rpc_mux!(child;
    AgentNodeServiceRequest => AgentNodeServiceResponse;
    NodeServiceRequest => NodeServiceResponse;
);

#[derive(Clone)]
pub struct NodeRpcServer<N: Network> {
    pub log_level_handler: ReloadHandler,
    pub block_db: Arc<RwLock<Option<BlockDB<N>>>>,
}

impl<N: Network> NodeService for NodeRpcServer<N> {
    async fn status(self, _: context::Context) -> Result<(), AgentError> {
        Ok(())
    }

    async fn set_log_level(self, _: context::Context, verbosity: u8) -> Result<(), AgentError> {
        self.log_level_handler
            .modify(|filter| *filter = make_env_filter(verbosity))
            .map_err(|_| AgentError::FailedToChangeLogLevel)?;

        Ok(())
    }

    async fn find_transaction(
        self,
        _: context::Context,
        tx_id: String,
    ) -> Result<Option<String>, AgentError> {
        let block_guard = self.block_db.read();
        let Ok(Some(block_db)) = block_guard.as_deref() else {
            return Err(AgentError::NodeClientNotReady);
        };

        let tx_id: N::TransactionID = tx_id
            .parse()
            .map_err(|_| AgentError::InvalidTransactionId)?;

        block_db
            .find_block_hash(&tx_id)
            .map_err(|_| AgentError::FailedToMakeRequest)
            .map(|hash| hash.map(|hash| hash.to_string()))
    }

    async fn get_block_lite(
        self,
        _: context::Context,
        block_hash: String,
    ) -> Result<Option<SnarkOSLiteBlock>, AgentError> {
        let block_guard = self.block_db.read();
        let Ok(Some(block_db)) = block_guard.as_deref() else {
            return Err(AgentError::NodeClientNotReady);
        };

        let hash: N::BlockHash = block_hash
            .parse()
            .map_err(|_| AgentError::InvalidBlockHash)?;

        let Some(height) = block_db
            .get_block_height(&hash)
            .map_err(|_| AgentError::FailedToMakeRequest)?
        else {
            return Ok(None);
        };

        let Some(info) = get_block_info_for_height(block_db, height) else {
            return Ok(None);
        };

        let Some(transactions) = block_db
            .get_block_transactions(&hash)
            .map_err(|_| AgentError::FailedToMakeRequest)?
        else {
            return Ok(None);
        };

        // convert snarkVM transactions into transaction ids
        let tx_ids = transactions.iter().map(|tx| tx.id().to_string()).collect();

        Ok(Some(SnarkOSLiteBlock {
            info,
            transactions: tx_ids,
        }))
    }
}
