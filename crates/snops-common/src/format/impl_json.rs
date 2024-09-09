use super::{DataFormat, PackedUint};

impl DataFormat for serde_json::Value {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: std::io::prelude::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        let bytes = serde_json::to_vec(self)
            .map_err(|e| crate::format::DataWriteError::Custom(format!("json to bytes: {e:?}")))?;

        Ok(PackedUint::from(bytes.len()).write_data(writer)? + writer.write(&bytes)?)
    }

    fn read_data<R: std::io::prelude::Read>(
        reader: &mut R,
        _header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        let len = usize::from(PackedUint::read_data(reader, &())?);
        let mut buf = vec![0; len];
        reader.read_exact(&mut buf)?;
        serde_json::from_slice(&buf)
            .map_err(|e| crate::format::DataReadError::Custom(format!("json from bytes: {e:?}")))
    }
}

#[cfg(test)]
#[rustfmt::skip]
mod test {

    use serde_json::json;

    use crate::format::DataFormat;
    use crate::format::PackedUint;

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr) => {
            #[test]
            fn $name() {
                let mut data = Vec::new();
                let vec_raw = serde_json::to_vec(&$a).unwrap();
                let mut vec = PackedUint::from(vec_raw.len()).to_byte_vec().unwrap();
                vec.extend(vec_raw);

                $a.write_data(&mut data).unwrap();
                assert_eq!(data, vec);

                let mut reader = &data[..];
                let read_value = <$ty>::read_data(&mut reader, &()).unwrap();
                assert_eq!(read_value, $a);
            }

        };
    }

    case!(test_json_null, serde_json::Value, serde_json::Value::Null);
    case!(test_json_bool, serde_json::Value, serde_json::Value::Bool(true));
    case!(test_json_number, serde_json::Value, serde_json::Value::Number(serde_json::Number::from_f64(1.23).unwrap()));
    case!(test_json_string, serde_json::Value, serde_json::Value::String("hello".to_string()));
    case!(test_json_array, serde_json::Value, serde_json::Value::Array(vec![
        serde_json::Value::Null,
        serde_json::Value::Bool(true),
        serde_json::Value::Number(serde_json::Number::from_f64(1.23).unwrap()),
        serde_json::Value::String("hello".to_string()),
    ]));
    case!(test_json_object, serde_json::Value, serde_json::Value::Object({
        let mut map = serde_json::Map::new();
        map.insert("null".to_string(), serde_json::Value::Null);
        map.insert("bool".to_string(), serde_json::Value::Bool(true));
        map.insert("number".to_string(), serde_json::Value::Number(serde_json::Number::from_f64(1.23).unwrap()));
        map.insert("string".to_string(), serde_json::Value::String("hello".to_string()));
        map
    }));
    case!(test_json_complex, serde_json::Value, json!({
        "null": null,
        "bool": true,
        "number": 1.23,
        "string": "hello",
        "array": [null, true, 1.23, "hello"],
        "object": {
            "null": null,
            "bool": true,
            "number": 1.23,
            "string": "hello"
        }
    }));
}
