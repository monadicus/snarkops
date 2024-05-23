use std::path::Path;

use serde_json::json;
use snops_common::{aot_cmds::AotCmd, state::NetworkId};

use super::error::{AuthorizeError, CannonError};

#[derive(Clone, Debug)]
pub struct Authorize {
    pub private_key: String,
    pub program_id: String,
    pub function_name: String,
    pub inputs: Vec<String>,
    pub priority_fee: Option<u64>,
    pub fee_record: Option<String>,
}

impl Authorize {
    pub async fn run(
        self,
        bin: &Path,
        network: NetworkId,
    ) -> Result<serde_json::Value, CannonError> {
        let aot = AotCmd::new(bin.to_path_buf(), network);
        let func_auth = aot
            .authorize(
                &self.private_key,
                &self.program_id,
                &self.function_name,
                &self.inputs,
            )
            .await
            .map_err(AuthorizeError::from)?;

        let fee_auth = aot
            .authorize_fee(
                &self.private_key,
                self.priority_fee,
                self.fee_record.as_ref(),
            )
            .await
            .map_err(AuthorizeError::from)?;

        Ok(json!( {
            "func": func_auth,
            "fee": fee_auth,
        }))
    }
}
