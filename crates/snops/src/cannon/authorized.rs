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
        let aot = AotCmd::new(bin.to_path_buf(), network);
        let auth = aot
            .authorize_program(
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
                &auth,
                self.priority_fee,
                self.fee_record.as_ref(),
            )
            .await
            .map_err(AuthorizeError::from)?;

        Ok(Authorization {
            auth: serde_json::from_str(&auth).map_err(AuthorizeError::Json)?,
            fee_auth: Some(serde_json::from_str(&fee_auth).map_err(AuthorizeError::Json)?),
        })
    }
}
