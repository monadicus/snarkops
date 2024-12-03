// rmp_serde and bincode have various limitations and are troublesome to debug.
// the overhead of JSON for messages is not a concern for the RPC layer.

pub fn encode<T: serde::Serialize>(msg: &T) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(msg)
}

pub fn decode<'de, T: serde::Deserialize<'de>>(msg: &'de [u8]) -> serde_json::Result<T> {
    serde_json::from_slice(msg)
}

// pub fn encode<T: serde::Serialize>(msg: &T) -> Result<Vec<u8>,
// rmp_serde::encode::Error> {     rmp_serde::to_vec(msg)
// }

// pub fn decode<'de, T: serde::Deserialize<'de>>(
//     msg: &'de [u8],
// ) -> Result<T, rmp_serde::decode::Error> {
//     rmp_serde::from_slice(msg)
// }
