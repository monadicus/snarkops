use snops_common::state::DocHeightRequest;

use crate::schema::{
    timeline::{Reconfig, TimelineEvent},
    ItemDocument,
};

#[test]
fn test_timeline_item_doc_serde() {
    let yaml = r#"
    version: timeline.snarkos.testing.monadic.us/v1
    name: test
    description: |-
        baz test description
        - baq
    timeline:
        - online: "*/*"
        - duration: 1m
        - config:
            validator/foo-1:
                height: 42
            validator/bar-2:
                height: 1m
    "#;

    assert!(dbg!(serde_yaml::from_str::<ItemDocument>(yaml)).is_ok());
}

#[test]
fn test_timeline_config_serde() {
    let yaml = r#"
        config:
            validator/1:
                height: 42
            validator/2:
                height: 1m
    "#;

    assert!(serde_yaml::from_str::<TimelineEvent>(yaml).is_ok());
}

#[test]
fn test_reconfig_serde() {
    let reconfig = Reconfig {
        height: Some(DocHeightRequest::Absolute(42)),
        peers: None,
        validators: None,
    };

    let yaml = serde_yaml::to_string(&reconfig).unwrap();
    let de: Reconfig = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(de.height, reconfig.height);
    assert_eq!(de.peers, reconfig.peers);
    assert_eq!(de.validators, reconfig.validators);
}
