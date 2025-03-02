use serde::{Deserialize, Deserializer, Serialize, de::Visitor};

#[derive(Debug, Clone, Serialize)]
pub struct Accounts {
    pub count: u16,
    #[serde(default)]
    pub seed: Option<u64>,
}

impl<'de> Deserialize<'de> for Accounts {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AccountsVisitor;

        impl<'de> Visitor<'de> for AccountsVisitor {
            type Value = Accounts;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or an object with a count and seed")
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Accounts {
                    count: v.min(u16::MAX as u64) as u16,
                    seed: None,
                })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut count = None;
                let mut seed = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        "count" => {
                            if count.is_some() {
                                return Err(serde::de::Error::duplicate_field("count"));
                            }
                            count = Some(map.next_value()?);
                        }
                        "seed" => {
                            if seed.is_some() {
                                return Err(serde::de::Error::duplicate_field("seed"));
                            }
                            seed = Some(map.next_value()?);
                        }
                        _ => return Err(serde::de::Error::unknown_field(key, &["count", "seed"])),
                    }
                }

                Ok(Accounts {
                    count: count.ok_or_else(|| serde::de::Error::missing_field("count"))?,
                    seed,
                })
            }
        }

        deserializer.deserialize_any(AccountsVisitor)
    }
}
