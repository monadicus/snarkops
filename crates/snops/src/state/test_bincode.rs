use std::str::FromStr;

use snops_common::{
    state::{AgentId, AgentMode, AgentPeer, AgentState, EnvId, HeightRequest, NodeKey, NodeState},
    INTERN,
};

use crate::state::AgentFlags;

macro_rules! bincode_test {
    ($name:ident, $( $others:expr),* ) => {
        #[test]
        fn $name() -> Result<(), Box<dyn std::error::Error>> {
            $(

                let value = $others;
                let encoded = bincode::serialize(&value).expect("serialize");
                let decoded = bincode::deserialize(&encoded).expect("deserialize");
                assert_eq!(value, decoded);
            )*
            Ok(())
        }
    };
}

bincode_test!(
    test_agent_state,
    AgentState::Inventory,
    AgentState::Node(
        EnvId::rand(),
        Box::new(NodeState {
            node_key: NodeKey::from_str("client/1").unwrap(),
            private_key: snops_common::state::KeyState::Literal("foo".to_owned()),
            height: (0, HeightRequest::Top),
            online: true,
            peers: vec![AgentPeer::Internal(AgentId::rand(), 0)],
            validators: vec![AgentPeer::External("127.0.0.1:0".parse().unwrap())],
            env: [("foo".to_owned(), "bar".to_owned())].into_iter().collect()
        })
    )
);

bincode_test!(
    test_agent_mode,
    AgentMode {
        validator: false,
        prover: false,
        client: false,
        compute: false
    },
    AgentMode {
        validator: true,
        prover: false,
        client: true,
        compute: false
    }
);

bincode_test!(
    test_agent_flags,
    AgentFlags {
        mode: Default::default(),
        local_pk: false,
        labels: Default::default()
    },
    AgentFlags {
        mode: Default::default(),
        local_pk: true,
        labels: [INTERN.get_or_intern("foo")].into_iter().collect()
    },
    AgentFlags {
        mode: AgentMode {
            validator: true,
            prover: true,
            client: true,
            compute: true
        },
        local_pk: true,
        labels: [INTERN.get_or_intern("foo"), INTERN.get_or_intern("bar")]
            .into_iter()
            .collect()
    }
);
