use std::str::FromStr;

use lazy_static::lazy_static;
use snops_common::state::InternedId;

use super::EventFilter::*;
use super::EventKindFilter::*;

lazy_static! {
    static ref A: InternedId = InternedId::from_str("a").unwrap();
    static ref B: InternedId = InternedId::from_str("b").unwrap();
    static ref C: InternedId = InternedId::from_str("c").unwrap();
    static ref D: InternedId = InternedId::from_str("d").unwrap();
}

#[test]
fn test_filter_bitand() {
    assert_eq!(Unfiltered & Unfiltered, Unfiltered);
    assert_eq!(AgentBlockInfo & Unfiltered, EventIs(AgentBlockInfo));
    assert_eq!(
        AgentBlockInfo & AgentIs(*A),
        AllOf(vec![EventIs(AgentBlockInfo), AgentIs(*A)])
    );
    assert_eq!(
        AgentIs(*A) & AgentIs(*B),
        AllOf(vec![AgentIs(*A), AgentIs(*B)])
    );
    assert_eq!(
        AgentIs(*A) & AgentIs(*B) & AgentIs(*C),
        AllOf(vec![AgentIs(*A), AgentIs(*B), AgentIs(*C)])
    );
}

#[test]
fn test_filter_bitor() {
    assert_eq!(Unfiltered | Unfiltered, Unfiltered);
    assert_eq!(AgentBlockInfo | Unfiltered, Unfiltered);
    assert_eq!(
        AgentBlockInfo | AgentIs(*A),
        AnyOf(vec![EventIs(AgentBlockInfo), AgentIs(*A)])
    );
    assert_eq!(
        AgentIs(*A) | AgentIs(*B),
        AnyOf(vec![AgentIs(*A), AgentIs(*B)])
    );
    assert_eq!(
        AgentIs(*A) | AgentIs(*B) | AgentIs(*C),
        AnyOf(vec![AgentIs(*A), AgentIs(*B), AgentIs(*C)])
    );
}

#[test]
fn test_filter_bitxor() {
    assert_eq!(Unfiltered ^ Unfiltered, Unfiltered);
    assert_eq!(AgentBlockInfo ^ Unfiltered, EventIs(AgentBlockInfo));
    assert_eq!(
        AgentBlockInfo ^ AgentIs(*A),
        OneOf(vec![EventIs(AgentBlockInfo), AgentIs(*A)])
    );
    assert_eq!(
        AgentIs(*A) ^ AgentIs(*B),
        OneOf(vec![AgentIs(*A), AgentIs(*B)])
    );
    assert_eq!(
        AgentIs(*A) ^ AgentIs(*B) ^ AgentIs(*C),
        OneOf(vec![AgentIs(*A), AgentIs(*B), AgentIs(*C)])
    );
}

#[test]
fn test_filter_not() {
    assert_eq!(!Unfiltered, Not(Box::new(Unfiltered)));
    assert_eq!(!AgentBlockInfo, Not(Box::new(EventIs(AgentBlockInfo))));
    assert_eq!(!AgentIs(*A), Not(Box::new(AgentIs(*A))));
    assert_eq!(
        !AgentIs(*A) & AgentIs(*B),
        AllOf(vec![Not(Box::new(AgentIs(*A))), AgentIs(*B)])
    );
}
