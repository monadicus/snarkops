use std::collections::HashMap;

use axum::{response::Response, Json};

use super::{models::Reconfig, Env, WithTargets};
use crate::schema::NodeTargets;

// TODO USEME
#[allow(dead_code)]
pub enum ConfigMode {
    // one config for all nodes
    Bulk(WithTargets<Reconfig>),
    // specify multiple configs for different nodes simultaneously
    Particular(HashMap<NodeTargets, Reconfig>),
}

pub async fn config(
    Env { .. }: Env,
    Json(WithTargets { data: _config, .. }): Json<WithTargets<Reconfig>>,
) -> Response {
    unimplemented!()
}
