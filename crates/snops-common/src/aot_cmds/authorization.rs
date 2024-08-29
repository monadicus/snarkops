use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::format::DataFormat;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Authorization {
    Program {
        auth: Value,
        fee_auth: Option<Value>,
    },
    Deploy {
        owner: Value,
        deployment: Value,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fee_auth: Option<Value>,
    },
}

impl FromStr for Authorization {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl DataFormat for Authorization {
    type Header = u8;
    const LATEST_HEADER: Self::Header = 1u8;

    fn write_data<W: std::io::Write>(
        &self,
        writer: &mut W,
    ) -> Result<usize, crate::format::DataWriteError> {
        match self {
            Authorization::Program { auth, fee_auth } => {
                let mut written = 0;
                written += 0u8.write_data(writer)?;
                written += auth.write_data(writer)?;
                written += fee_auth.write_data(writer)?;
                Ok(written)
            }
            Authorization::Deploy {
                owner,
                deployment,
                fee_auth,
            } => {
                let mut written = 0;
                written += 1u8.write_data(writer)?;
                written += owner.write_data(writer)?;
                written += deployment.write_data(writer)?;
                written += fee_auth.write_data(writer)?;
                Ok(written)
            }
        }
    }

    fn read_data<R: std::io::Read>(
        reader: &mut R,
        header: &Self::Header,
    ) -> Result<Self, crate::format::DataReadError> {
        if *header != Self::LATEST_HEADER {
            return Err(crate::format::DataReadError::unsupported(
                "Authorization",
                Self::LATEST_HEADER,
                *header,
            ));
        }

        let tag = u8::read_data(reader, &())?;
        match tag {
            0 => {
                let auth = Value::read_data(reader, &())?;
                let fee_auth = Option::<Value>::read_data(reader, &())?;
                Ok(Authorization::Program { auth, fee_auth })
            }
            1 => {
                let owner = Value::read_data(reader, &())?;
                let deployment = Value::read_data(reader, &())?;
                let fee_auth = Option::<Value>::read_data(reader, &())?;
                Ok(Authorization::Deploy {
                    owner,
                    deployment,
                    fee_auth,
                })
            }
            _ => Err(crate::format::DataReadError::custom(
                "invalid Authorization tag",
            )),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::format::DataFormat;

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr) => {
            #[test]
            fn $name() {
                let mut data = Vec::new();
                let value: $ty = $a;
                value.write_data(&mut data).unwrap();
                let mut reader = &data[..];
                let read_value =
                    <$ty>::read_data(&mut reader, &<$ty as DataFormat>::LATEST_HEADER).unwrap();

                match (value, read_value) {
                    (Authorization::Program { auth, fee_auth }, Authorization::Program { auth: read_auth, fee_auth: read_fee_auth }) => {
                        assert_eq!(auth, read_auth);
                        assert_eq!(fee_auth, read_fee_auth);
                    }
                    (Authorization::Deploy { owner, deployment, fee_auth }, Authorization::Deploy { owner: read_owner, deployment: read_deployment, fee_auth: read_fee_auth }) => {
                        assert_eq!(owner, read_owner);
                        assert_eq!(deployment, read_deployment);
                        assert_eq!(fee_auth, read_fee_auth);
                    }
                    _ => panic!("Authorization types do not match"),
                }
            }
        };
    }

    case!(
        test_program,
        Authorization,
        Authorization::Program {
            auth: Value::String("auth".to_string()),
            fee_auth: Some(Value::String("fee_auth".to_string()))
        }
    );

    case!(
        test_deploy,
        Authorization,
        Authorization::Deploy {
            owner: Value::String("owner".to_string()),
            deployment: Value::String("deployment".to_string()),
            fee_auth: Some(Value::String("fee_auth".to_string()))
        }
    );

    case!(
        test_deploy_no_fee,
        Authorization,
        Authorization::Deploy {
            owner: Value::String("owner".to_string()),
            deployment: Value::String("deployment".to_string()),
            fee_auth: None
        }
    );

    case!(
        test_program_no_fee,
        Authorization,
        Authorization::Program {
            auth: Value::String("auth".to_string()),
            fee_auth: None
        }
    );
}
