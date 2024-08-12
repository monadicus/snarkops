// https://github.com/serde-rs/serde/issues/1560#issuecomment-506915291
macro_rules! named_unit_variant {
    ($variant:ident) => {
        pub mod $variant {
            pub fn serialize<S>(serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(stringify!($variant))
            }

            pub fn deserialize<'de, D>(deserializer: D) -> Result<(), D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct V;
                impl<'de> serde::de::Visitor<'de> for V {
                    type Value = ();
                    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                        f.write_str(concat!("\"", stringify!($variant), "\""))
                    }
                    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                        if value == stringify!($variant) {
                            Ok(())
                        } else {
                            Err(E::invalid_value(serde::de::Unexpected::Str(value), &self))
                        }
                    }
                }
                deserializer.deserialize_str(V)
            }
        }
    };
}

named_unit_variant!(top);
named_unit_variant!(auto);
