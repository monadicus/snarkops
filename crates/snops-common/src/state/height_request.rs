/// for some reason bincode does not allow deserialize_any so if i want to allow
/// end users to type "top", 42, or "persist" i need to do have to copies of
/// this where one is not untagged.
///
/// bincode. please.
#[derive(Debug, Copy, Default, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase", untagged)]
pub enum DocHeightRequest {
    #[default]
    /// Use the latest height for the ledger
    #[serde(with = "super::strings::top")]
    Top,
    /// Set the height to the given block (there must be a checkpoint at this
    /// height) Setting to 0 will reset the height to the genesis block
    Absolute(u32),
    /// Use the next checkpoint that matches this checkpoint span
    Checkpoint(checkpoint::RetentionSpan),
    // the control plane doesn't know the heights the nodes are at
    // TruncateHeight(u32),
    // TruncateTime(i64),
}

#[derive(Debug, Default, Copy, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HeightRequest {
    #[default]
    /// Use the latest height for the ledger
    Top,
    /// Set the height to the given block (there must be a checkpoint at this
    /// height) Setting to 0 will reset the height to the genesis block
    Absolute(u32),
    /// Use the next checkpoint that matches this checkpoint span
    Checkpoint(checkpoint::RetentionSpan),
    // the control plane doesn't know the heights the nodes are at
    // TruncateHeight(u32),
    // TruncateTime(i64),
}

impl HeightRequest {
    pub fn is_top(&self) -> bool {
        *self == Self::Top
    }

    pub fn reset(&self) -> bool {
        *self == Self::Absolute(0)
    }
}

impl From<DocHeightRequest> for HeightRequest {
    fn from(req: DocHeightRequest) -> Self {
        match req {
            DocHeightRequest::Top => Self::Top,
            DocHeightRequest::Absolute(h) => Self::Absolute(h),
            DocHeightRequest::Checkpoint(c) => Self::Checkpoint(c),
        }
    }
}
