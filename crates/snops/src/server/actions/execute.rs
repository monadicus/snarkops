use snops_common::{aot_cmds::AotCmd, state::KeyState};

use super::models::{AleoValue, ExecuteAction};
use crate::{
    cannon::{error::AuthorizeError, Authorization},
    env::{error::ExecutionError, Environment},
};

impl ExecuteAction {
    // TODO USEME
    #[allow(dead_code)]
    pub async fn execute(&self, env: &Environment) -> Result<(), ExecutionError> {
        let Self {
            cannon: cannon_id,
            private_key,
            program,
            function,
            inputs,
            priority_fee,
            fee_record,
        } = &self;

        let Some(cannon) = env.cannons.get(cannon_id) else {
            return Err(ExecutionError::UnknownCannon(*cannon_id));
        };

        let KeyState::Literal(resolved_pk) = env.storage.sample_keysource_pk(private_key) else {
            return Err(AuthorizeError::MissingPrivateKey(
                format!("{}.{cannon_id} {program}/{function}", env.id),
                private_key.to_string(),
            )
            .into());
        };

        let resolved_inputs = inputs
            .iter()
            .map(|input| match input {
                AleoValue::Key(key) => match env.storage.sample_keysource_addr(key) {
                    KeyState::Literal(key) => Ok(key),
                    _ => Err(AuthorizeError::InvalidProgramInputs(
                        format!("{program}/{function}"),
                        format!("key {key} does not resolve a valid addr"),
                    )),
                },
                AleoValue::Other(value) => Ok(value.clone()),
            })
            .collect::<Result<Vec<String>, AuthorizeError>>()?;

        // authorize the transaction
        let auth_str = AotCmd::new(env.aot_bin.clone(), env.network)
            .authorize(
                &resolved_pk,
                program,
                function,
                &resolved_inputs,
                *priority_fee,
                fee_record.as_ref(),
            )
            .await?;

        // parse the json and bundle it up
        let authorization: Authorization =
            serde_json::from_str(&auth_str).map_err(AuthorizeError::Json)?;

        // proxy it to a listen cannon
        cannon.proxy_auth(authorization)?;
        Ok(())
    }
}
