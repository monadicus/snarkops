use std::sync::OnceLock;

use semver::{Comparator, Prerelease, Version, VersionReq};

/// A version requirement that matches the current controlplane version against
/// an agent version
fn cp_version() -> &'static VersionReq {
    static CP_VERSION: OnceLock<VersionReq> = OnceLock::new();

    CP_VERSION.get_or_init(|| {
        let version = Version::parse(env!("CARGO_PKG_VERSION"))
            .expect("Failed to parse controlplane version");

        VersionReq {
            comparators: vec![
                Comparator {
                    op: semver::Op::GreaterEq,
                    major: version.major,
                    minor: Some(version.minor),
                    patch: Some(0),
                    pre: Prerelease::EMPTY,
                },
                Comparator {
                    op: semver::Op::Less,
                    major: version.major,
                    minor: Some(version.minor + 1),
                    patch: None,
                    pre: Prerelease::EMPTY,
                },
            ],
        }
    })
}

pub fn agent_version_ok(agent_version: &Version) -> bool {
    cp_version().matches(agent_version)
}
