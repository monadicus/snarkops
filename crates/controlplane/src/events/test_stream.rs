use std::str::FromStr;

use lazy_static::lazy_static;
use snops_common::state::InternedId;

use super::EventFilter::*;
use super::EventKind as EK;
use super::EventKindFilter::*;
use super::Events;

lazy_static! {
    static ref A: InternedId = InternedId::from_str("a").unwrap();
    static ref B: InternedId = InternedId::from_str("b").unwrap();
    static ref C: InternedId = InternedId::from_str("c").unwrap();
    static ref D: InternedId = InternedId::from_str("d").unwrap();
}

#[test]
fn test_stream_filtering() {
    let events = Events::new();

    let mut sub_all = events.subscribe();
    let mut sub_a = events.subscribe_on(AgentIs(*A));
    let mut sub_b = events.subscribe_on(AgentIs(*B));
    let mut sub_connected = events.subscribe_on(AgentConnected);

    assert_eq!(sub_all.collect_many().len(), 0);
    assert_eq!(sub_a.collect_many().len(), 0);
    assert_eq!(sub_b.collect_many().len(), 0);
    assert_eq!(sub_connected.collect_many().len(), 0);

    events.emit(EK::AgentConnected.with_agent_id(*A));
    events.emit(EK::AgentDisconnected.with_agent_id(*A));
    events.emit(EK::Block(Default::default()).with_agent_id(*B));

    assert_eq!(sub_all.collect_many().len(), 3);
    assert_eq!(sub_a.collect_many().len(), 2);
    assert_eq!(sub_b.collect_many().len(), 1);
    assert_eq!(sub_connected.collect_many().len(), 1);
}