use std::collections::HashMap;

use axum::{debug_handler, response::Response, Json};

use super::{models::Reconfig, Env, WithTargets};
use crate::schema::NodeTargets;

pub enum ConfigMode {
    // one config for all nodes
    Bulk(WithTargets<Reconfig>),
    // specify multiple configs for different nodes simultaneously
    Particular(HashMap<NodeTargets, Reconfig>),
}

#[debug_handler]
pub async fn config(
    Env { env, state, .. }: Env,
    Json(WithTargets {
        nodes,
        data: config,
    }): Json<WithTargets<Reconfig>>,
) -> Response {
    unimplemented!()
}
