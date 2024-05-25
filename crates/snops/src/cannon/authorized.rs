use std::path::Path;

use snops_common::{aot_cmds::AotCmd, state::NetworkId};

use super::{
    error::{AuthorizeError, CannonError},
    Authorization,
};

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
    pub async fn run(self, bin: &Path, network: NetworkId) -> Result<Authorization, CannonError> {
        let auth_str = AotCmd::new(bin.to_path_buf(), network)
            .authorize(
                &self.private_key,
                &self.program_id,
                &self.function_name,
                &self.inputs,
                self.priority_fee,
                self.fee_record.as_ref(),
            )
            .await
            .map_err(AuthorizeError::from)?;

        // parse the json and bundle it up
        Ok(serde_json::from_str(&auth_str).map_err(AuthorizeError::Json)?)
    }
}
