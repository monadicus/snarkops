use std::sync::Arc;

use super::{
    filter_parse::EventFilterParseError,
    EventFilter::{self, *},
    EventKindFilter::*,
};
use crate::events::filter_parse::EventFilterParsable;
use crate::{node_targets::NodeTargets, state::InternedId};

macro_rules! eq {
    ($s:expr, $f:expr) => {
        assert_eq!($s.parse::<EventFilter>()?, $f);
    };
}

macro_rules! err {
    ($s:expr, $pattern:pat $(if $guard:expr)?) => {
        assert!(match $s.parse::<EventFilter>() {
            $pattern $(if $guard)? => true,
            other => {
                eprintln!("Received {other:?}");
                false
            }
        })
    };
}

#[test]
fn test_each_filter() -> Result<(), EventFilterParseError> {
    eq!("unfiltered", Unfiltered);
    eq!("all-of(unfiltered)", AllOf(vec![Unfiltered]));
    eq!("any-of(unfiltered)", AnyOf(vec![Unfiltered]));
    eq!("one-of(unfiltered)", OneOf(vec![Unfiltered]));
    eq!("not(unfiltered)", Not(Box::new(Unfiltered)));
    eq!("agent-is(default)", AgentIs(InternedId::default()));
    eq!("env-is(default)", EnvIs(InternedId::default()));
    eq!(
        "transaction-is(foo)",
        TransactionIs(Arc::new(String::from("foo")))
    );
    eq!("cannon-is(default)", CannonIs(InternedId::default()));
    eq!("event-is(agent-connected)", EventIs(AgentConnected));
    eq!(
        "node-key-is(client/foo)",
        NodeKeyIs("client/foo".parse().unwrap())
    );
    eq!(
        "node-target-is(client/any)",
        NodeTargetIs(NodeTargets::One("client/any".parse().unwrap()))
    );

    Ok(())
}

#[test]
fn test_array() -> Result<(), EventFilterParseError> {
    eq!(
        "all-of(unfiltered, unfiltered)",
        AllOf(vec![Unfiltered, Unfiltered])
    );
    eq!(
        "any-of(unfiltered, unfiltered)",
        AnyOf(vec![Unfiltered, Unfiltered])
    );
    eq!(
        "one-of(unfiltered, unfiltered)",
        OneOf(vec![Unfiltered, Unfiltered])
    );

    eq!(
        "any-of(
        unfiltered,
        all-of(unfiltered),
        any-of(unfiltered),
        one-of(unfiltered),
        not(unfiltered),
        agent-is(default),
        env-is(default),
        transaction-is(foo),
        cannon-is(default),
        event-is(agent-connected),
        node-key-is(client/foo),
        node-target-is(client/any)
    )",
        AnyOf(vec![
            Unfiltered,
            AllOf(vec![Unfiltered]),
            AnyOf(vec![Unfiltered]),
            OneOf(vec![Unfiltered]),
            Not(Box::new(Unfiltered)),
            AgentIs(InternedId::default()),
            EnvIs(InternedId::default()),
            TransactionIs(Arc::new(String::from("foo"))),
            CannonIs(InternedId::default()),
            EventIs(AgentConnected),
            NodeKeyIs("client/foo".parse().unwrap()),
            NodeTargetIs(NodeTargets::One("client/any".parse().unwrap())),
        ])
    );

    eq!(
        "node-target-is(client/any,validator/any)",
        NodeTargetIs(NodeTargets::Many(vec![
            "client/any".parse().unwrap(),
            "validator/any".parse().unwrap(),
        ]))
    );

    Ok(())
}

#[test]
fn test_whitespace_ignore() -> Result<(), EventFilterParseError> {
    eq!(
        " all-of ( unfiltered , unfiltered ) ",
        AllOf(vec![Unfiltered, Unfiltered])
    );
    Ok(())
}

#[test]
fn test_trailing_commas() -> Result<(), EventFilterParseError> {
    eq!("all-of(unfiltered,)", AllOf(vec![Unfiltered]));
    Ok(())
}

#[test]
fn test_deep_nesting() -> Result<(), EventFilterParseError> {
    eq!(
        "all-of(all-of(all-of(all-of(all-of(all-of(unfiltered))))))",
        AllOf(vec![AllOf(vec![AllOf(vec![AllOf(vec![AllOf(vec![
            AllOf(vec![Unfiltered])
        ])])])])])
    );

    // not
    eq!("not(not(not(not(not(not(unfiltered))))))", !!!!!!Unfiltered);

    Ok(())
}

#[test]
fn test_invalid() {
    err!(
        "invalid",
        Err(EventFilterParseError::InvalidFilter(e)) if e == "invalid"
    );
}

#[test]
fn test_expected_parens() {
    use EventFilterParsable::*;

    err!(
        "all-of",
        Err(EventFilterParseError::ExpectedToken(a, b)) if a == OpenParen && b == "EOF"
    );
    err!(
        "all-of(",
        Err(EventFilterParseError::ExpectedToken(a, b)) if a == CloseParen && b == "EOF"
    );
    err!(
        "all-of(unfiltered",
        Err(EventFilterParseError::ExpectedToken(a, b)) if a == CommaOrCloseParen && b == "EOF"
    );
}

#[test]
fn test_failed_agent_parse() {
    err!(
        "agent-is(|)",
        Err(EventFilterParseError::ParseError(EventFilterParsable::AgentId, e))
            if e.starts_with("invalid InternedId expected pattern")
    );
}

#[test]
fn test_str() {
    macro_rules! test {
        ($s:expr) => {
            assert_eq!($s.parse::<EventFilter>().unwrap().to_string(), $s);
        };
    }

    test!("unfiltered");
    test!("any-of(unfiltered)");
    test!("all-of(unfiltered)");
    test!("one-of(unfiltered)");
    test!("not(unfiltered)");
    test!("agent-is(default)");
    test!("env-is(default)");
    test!("transaction-is(foo)");
    test!("cannon-is(default)");
    test!("event-is(agent-connected)");
    test!("node-key-is(client/foo)");
    test!("node-target-is(client/any)");
    test!("node-target-is(client/any, validator/any)");

    test!("any-of(unfiltered, unfiltered)");
    test!("any-of(agent-is(foo), cannon-is(bar))");
}
