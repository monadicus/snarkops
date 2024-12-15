use fixedbitset::FixedBitSet;
use indexmap::IndexSet;
use serde::{Deserialize, Serialize};
use snops_common::{
    lasso::Spur,
    set::{MaskBit, MASK_PREFIX_LEN},
    state::AgentModeOptions,
    INTERN,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentFlags {
    #[serde(deserialize_with = "deser_mode", serialize_with = "ser_mode")]
    pub mode: AgentModeOptions,
    #[serde(
        default,
        deserialize_with = "deser_labels",
        serialize_with = "ser_labels"
    )]
    pub labels: IndexSet<Spur>,
    #[serde(default, deserialize_with = "deser_pk", serialize_with = "ser_pk")]
    pub local_pk: bool,
}

fn deser_mode<'de, D>(deser: D) -> Result<AgentModeOptions, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // axum's querystring visitor marks all values as string
    let byte: u8 = String::deserialize(deser)?
        .parse()
        .map_err(|e| serde::de::Error::custom(format!("error parsing u8: {e}")))?;
    Ok(AgentModeOptions::from(byte))
}

fn ser_mode<S>(mode: &AgentModeOptions, ser: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    ser.serialize_str(&u8::from(*mode).to_string())
}

fn deser_labels<'de, D>(deser: D) -> Result<IndexSet<Spur>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deser)?
        .map(|s| {
            s.split(',')
                .filter(|s| !s.is_empty())
                .map(|s| INTERN.get_or_intern(s))
                .collect()
        })
        .unwrap_or_default())
}

fn ser_labels<S>(labels: &IndexSet<Spur>, ser: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if labels.is_empty() {
        return ser.serialize_none();
    }
    ser.serialize_some(
        &labels
            .iter()
            .map(|s| INTERN.resolve(s))
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn deser_pk<'de, D>(deser: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // axum's querystring visitor marks all values as string
    Ok(Option::<String>::deserialize(deser)?
        .map(|s| s == "true")
        .unwrap_or(false))
}

fn ser_pk<S>(pk: &bool, ser: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if *pk {
        ser.serialize_some("true")
    } else {
        ser.serialize_none()
    }
}

impl AgentFlags {
    pub fn mask(&self, labels: &[Spur]) -> FixedBitSet {
        let mut mask = FixedBitSet::with_capacity(labels.len() + MASK_PREFIX_LEN);
        if self.mode.validator {
            mask.insert(MaskBit::Validator as usize);
        }
        if self.mode.prover {
            mask.insert(MaskBit::Prover as usize);
        }
        if self.mode.client {
            mask.insert(MaskBit::Client as usize);
        }
        if self.mode.compute {
            mask.insert(MaskBit::Compute as usize);
        }
        if self.local_pk {
            mask.insert(MaskBit::LocalPrivateKey as usize);
        }

        for (i, label) in labels.iter().enumerate() {
            if self.labels.contains(label) {
                mask.insert(i + MASK_PREFIX_LEN);
            }
        }
        mask
    }
}
