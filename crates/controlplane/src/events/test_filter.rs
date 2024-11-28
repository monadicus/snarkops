use std::str::FromStr;

use chrono::Utc;
use lazy_static::lazy_static;
use snops_common::node_targets::NodeTargets;
use snops_common::rpc::error::ReconcileError;
use snops_common::state::InternedId;
use snops_common::state::LatestBlockInfo;
use snops_common::state::NodeKey;
use snops_common::state::NodeStatus;
use snops_common::state::ReconcileStatus;

use super::EventFilter::*;
use super::EventKind::*;
use super::EventKindFilter as EKF;
use crate::events::Event;

lazy_static! {
    static ref A: InternedId = InternedId::from_str("a").unwrap();
    static ref B: InternedId = InternedId::from_str("b").unwrap();
    static ref C: InternedId = InternedId::from_str("c").unwrap();
    static ref D: InternedId = InternedId::from_str("d").unwrap();
}

#[test]
fn test_unfiltered() {
    assert!(AgentConnected.event().matches(&Unfiltered));
    assert!(AgentHandshakeComplete.event().matches(&Unfiltered));
    assert!(AgentDisconnected.event().matches(&Unfiltered));
    assert!(ReconcileComplete.event().matches(&Unfiltered));
    assert!(Reconcile(ReconcileStatus::empty())
        .event()
        .matches(&Unfiltered));
    assert!(ReconcileError(ReconcileError::Offline)
        .event()
        .matches(&Unfiltered));
    assert!(NodeStatus(NodeStatus::Unknown).event().matches(&Unfiltered));
    assert!(Block(LatestBlockInfo::default())
        .event()
        .matches(&Unfiltered));
}

#[test]
fn test_all_of() {
    assert!(AgentConnected
        .event()
        .matches(&AllOf(vec![EventIs(EKF::AgentConnected)])));

    let e = Event {
        created_at: Utc::now(),
        agent: Some(*A),
        node_key: Some(NodeKey::from_str("client/foo").unwrap()),
        env: Some(*B),
        kind: AgentConnected,
    };

    assert!(e.matches(&(EKF::AgentConnected & AgentIs(*A))));
    assert!(e.matches(&(EKF::AgentConnected & NodeKeyIs(NodeKey::from_str("client/foo").unwrap()))));
    assert!(e.matches(&(EKF::AgentConnected & EnvIs(*B))));
    assert!(e.matches(&(AgentIs(*A) & NodeTargetIs(NodeTargets::ALL) & EnvIs(*B))));

    assert!(!e.matches(&(EKF::AgentConnected & AgentIs(*B))));
    assert!(
        !e.matches(&(EKF::AgentConnected & NodeKeyIs(NodeKey::from_str("client/bar").unwrap())))
    );
    assert!(!e.matches(&(EKF::AgentConnected & EnvIs(*A))));
    assert!(!e.matches(&(AgentIs(*B) & NodeTargetIs(NodeTargets::ALL) & EnvIs(*B))));
}

#[test]
fn test_any_of() {
    assert!(AgentConnected
        .event()
        .matches(&AnyOf(vec![EventIs(EKF::AgentConnected)])));

    let e = Event {
        created_at: Utc::now(),
        agent: Some(*A),
        node_key: Some(NodeKey::from_str("client/foo").unwrap()),
        env: Some(*B),
        kind: AgentConnected,
    };

    assert!(e.matches(&(EKF::AgentConnected | AgentIs(*A))));
    assert!(e.matches(&(EKF::AgentConnected | NodeKeyIs(NodeKey::from_str("client/foo").unwrap()))));
    assert!(e.matches(&(EKF::AgentConnected | EnvIs(*B))));
    assert!(e.matches(&(AgentIs(*A) | NodeTargetIs(NodeTargets::ALL) | EnvIs(*B))));

    assert!(e.matches(&(EKF::AgentConnected | AgentIs(*B))));
    assert!(e.matches(&(EKF::AgentConnected | NodeKeyIs(NodeKey::from_str("client/bar").unwrap()))));
    assert!(e.matches(&(EKF::AgentConnected | EnvIs(*A))));

    assert!(e.matches(&(AgentIs(*B) | NodeTargetIs(NodeTargets::ALL) | EnvIs(*B))));

    assert!(!e.matches(&(EKF::AgentDisconnected | AgentIs(*C))));
    assert!(
        !e.matches(&(EKF::AgentDisconnected | NodeKeyIs(NodeKey::from_str("client/bar").unwrap())))
    );
}

#[test]
fn test_one_of() {
    assert!(AgentConnected
        .event()
        .matches(&OneOf(vec![EventIs(EKF::AgentConnected)])));

    let e = Event {
        created_at: Utc::now(),
        agent: Some(*A),
        node_key: Some(NodeKey::from_str("client/foo").unwrap()),
        env: Some(*B),
        kind: AgentConnected,
    };

    assert!(e.matches(&(EKF::AgentConnected ^ AgentIs(*B))));
    assert!(e.matches(&(EKF::AgentConnected & (AgentIs(*A) ^ AgentIs(*B) ^ AgentIs(*C)))));

    assert!(!e.matches(&(EKF::AgentConnected ^ AgentIs(*A))));
    assert!(e.matches(&(!(EKF::AgentConnected ^ AgentIs(*A)))));
}
