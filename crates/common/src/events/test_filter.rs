use std::str::FromStr;

use chrono::Utc;
use lazy_static::lazy_static;

use super::{AgentEvent::*, EventFilter::*, EventKind::*, EventKindFilter::*};
use crate::events::{Event, EventHelpers};
use crate::{
    node_targets::NodeTargets,
    rpc::error::ReconcileError,
    state::{InternedId, LatestBlockInfo, NodeKey, NodeStatus, ReconcileStatus},
};

lazy_static! {
    static ref A: InternedId = InternedId::from_str("a").unwrap();
    static ref B: InternedId = InternedId::from_str("b").unwrap();
    static ref C: InternedId = InternedId::from_str("c").unwrap();
    static ref D: InternedId = InternedId::from_str("d").unwrap();
}

#[test]
fn test_unfiltered() {
    assert!(Connected {
        version: "0.0.0".to_string()
    }
    .event()
    .matches(&Unfiltered));
    assert!(HandshakeComplete.event().matches(&Unfiltered));
    assert!(Disconnected.event().matches(&Unfiltered));
    assert!(ReconcileComplete.event().matches(&Unfiltered));
    assert!(Reconcile(ReconcileStatus::empty())
        .event()
        .matches(&Unfiltered));
    assert!(ReconcileError(ReconcileError::Offline)
        .event()
        .matches(&Unfiltered));
    assert!(NodeStatus(NodeStatus::Unknown).event().matches(&Unfiltered));
    assert!(BlockInfo(LatestBlockInfo::default())
        .event()
        .matches(&Unfiltered));
}

#[test]
fn test_all_of() {
    assert!(Connected {
        version: "0.0.0".to_string()
    }
    .event()
    .matches(&AllOf(vec![EventIs(AgentConnected)])));

    let e = Event {
        created_at: Utc::now(),
        agent: Some(*A),
        node_key: Some(NodeKey::from_str("client/foo").unwrap()),
        env: Some(*B),
        transaction: None,
        cannon: None,
        content: Agent(Connected {
            version: "0.0.0".to_string(),
        }),
    };

    assert!(e.matches(&(AgentConnected & AgentIs(*A))));
    assert!(e.matches(&(AgentConnected & NodeKeyIs(NodeKey::from_str("client/foo").unwrap()))));
    assert!(e.matches(&(AgentConnected & EnvIs(*B))));
    assert!(e.matches(&(AgentIs(*A) & NodeTargetIs(NodeTargets::ALL) & EnvIs(*B))));

    assert!(!e.matches(&(AgentConnected & AgentIs(*B))));
    assert!(!e.matches(&(AgentConnected & NodeKeyIs(NodeKey::from_str("client/bar").unwrap()))));
    assert!(!e.matches(&(AgentConnected & EnvIs(*A))));
    assert!(!e.matches(&(AgentIs(*B) & NodeTargetIs(NodeTargets::ALL) & EnvIs(*B))));
}

#[test]
fn test_any_of() {
    assert!(Connected {
        version: "0.0.0".to_string()
    }
    .event()
    .matches(&AnyOf(vec![EventIs(AgentConnected)])));

    let e = Event {
        created_at: Utc::now(),
        agent: Some(*A),
        node_key: Some(NodeKey::from_str("client/foo").unwrap()),
        env: Some(*B),
        transaction: None,
        cannon: None,
        content: Agent(Connected {
            version: "0.0.0".to_string(),
        }),
    };

    assert!(e.matches(&(AgentConnected | AgentIs(*A))));
    assert!(e.matches(&(AgentConnected | NodeKeyIs(NodeKey::from_str("client/foo").unwrap()))));
    assert!(e.matches(&(AgentConnected | EnvIs(*B))));
    assert!(e.matches(&(AgentIs(*A) | NodeTargetIs(NodeTargets::ALL) | EnvIs(*B))));

    assert!(e.matches(&(AgentConnected | AgentIs(*B))));
    assert!(e.matches(&(AgentConnected | NodeKeyIs(NodeKey::from_str("client/bar").unwrap()))));
    assert!(e.matches(&(AgentConnected | EnvIs(*A))));

    assert!(e.matches(&(AgentIs(*B) | NodeTargetIs(NodeTargets::ALL) | EnvIs(*B))));

    assert!(!e.matches(&(AgentDisconnected | AgentIs(*C))));
    assert!(!e.matches(&(AgentDisconnected | NodeKeyIs(NodeKey::from_str("client/bar").unwrap()))));
}

#[test]
fn test_one_of() {
    assert!(Connected {
        version: "0.0.0".to_string()
    }
    .event()
    .matches(&OneOf(vec![EventIs(AgentConnected)])));

    let e = Event {
        created_at: Utc::now(),
        agent: Some(*A),
        node_key: Some(NodeKey::from_str("client/foo").unwrap()),
        env: Some(*B),
        transaction: None,
        cannon: None,
        content: Agent(Connected {
            version: "0.0.0".to_string(),
        }),
    };

    assert!(e.matches(&(AgentConnected ^ AgentIs(*B))));
    assert!(e.matches(&(AgentConnected & (AgentIs(*A) ^ AgentIs(*B) ^ AgentIs(*C)))));

    assert!(!e.matches(&(AgentConnected ^ AgentIs(*A))));
    assert!(e.matches(&(!(AgentConnected ^ AgentIs(*A)))));
}
