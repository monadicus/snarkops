use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
};

use super::{DataFormat, DataFormatReader, DataReadError, DataWriteError};

impl DataFormat for Ipv4Addr {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(writer.write(&self.octets())?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let mut octets = [0u8; 4];
        reader.read_exact(&mut octets)?;
        Ok(Ipv4Addr::from(octets))
    }
}

impl DataFormat for Ipv6Addr {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(writer.write(&self.octets())?)
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        let mut octets = [0u8; 16];
        reader.read_exact(&mut octets)?;
        Ok(Ipv6Addr::from(octets))
    }
}

impl DataFormat for IpAddr {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        match self {
            IpAddr::V4(addr) => Ok(0u8.write_data(writer)? + addr.write_data(writer)?),
            IpAddr::V6(addr) => Ok(1u8.write_data(writer)? + addr.write_data(writer)?),
        }
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        match reader.read_data(&())? {
            0u8 => Ok(IpAddr::V4(reader.read_data(&())?)),
            1u8 => Ok(IpAddr::V6(reader.read_data(&())?)),
            n => Err(DataReadError::Custom(format!(
                "invalid IpAddr discriminant: {n}"
            ))),
        }
    }
}

impl DataFormat for SocketAddrV4 {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.ip().write_data(writer)? + self.port().write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        Ok(SocketAddrV4::new(
            reader.read_data(header)?,
            reader.read_data(header)?,
        ))
    }
}

impl DataFormat for SocketAddrV6 {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        Ok(self.ip().write_data(writer)? + self.port().write_data(writer)?)
    }

    fn read_data<R: Read>(reader: &mut R, header: &Self::Header) -> Result<Self, DataReadError> {
        Ok(SocketAddrV6::new(
            reader.read_data(header)?,
            reader.read_data(header)?,
            0,
            0,
        ))
    }
}

impl DataFormat for SocketAddr {
    type Header = ();
    const LATEST_HEADER: Self::Header = ();

    fn write_data<W: Write>(&self, writer: &mut W) -> Result<usize, DataWriteError> {
        match self {
            SocketAddr::V4(addr) => Ok(0u8.write_data(writer)? + addr.write_data(writer)?),
            SocketAddr::V6(addr) => Ok(1u8.write_data(writer)? + addr.write_data(writer)?),
        }
    }

    fn read_data<R: Read>(reader: &mut R, _header: &Self::Header) -> Result<Self, DataReadError> {
        match reader.read_data(&())? {
            0u8 => Ok(SocketAddr::V4(reader.read_data(&())?)),
            1u8 => Ok(SocketAddr::V6(reader.read_data(&())?)),
            n => Err(DataReadError::Custom(format!(
                "invalid SocketAddr discriminant: {n}"
            ))),
        }
    }
}

#[cfg(test)]
#[rustfmt::skip]
mod test {
    use crate::format::DataFormat;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

    macro_rules! case {
        ($name:ident, $ty:ty, $a:expr_2021, $b:expr_2021) => {
            #[test]
            fn $name() {
                let mut data = Vec::new();
                $a.write_data(&mut data).unwrap();
                assert_eq!(data, &$b);

                let mut reader = &data[..];
                let read_value = <$ty>::read_data(&mut reader, &()).unwrap();
                assert_eq!(read_value, $a);

            }

        };
    }

    case!(ip_localhost, Ipv4Addr, Ipv4Addr::LOCALHOST, [127, 0, 0, 1]);
    case!(ip_v6_localhost, Ipv6Addr, "::1".parse::<Ipv6Addr>().unwrap(), [
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 1,
    ]);
    case!(ip_v4, IpAddr, IpAddr::V4(Ipv4Addr::LOCALHOST), [0, 127, 0, 0, 1]);
    case!(ip_v6, IpAddr, IpAddr::V6("::1".parse::<Ipv6Addr>().unwrap()), [
        1,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 1,
    ]);

    case!(socket_v4, SocketAddrV4, SocketAddrV4::new(Ipv4Addr::LOCALHOST, 80), [
        127, 0, 0, 1,
        80, 0,
    ]);
    case!(socket_v6, SocketAddrV6, SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8080, 0, 0), [
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 1,
        144, 31,
    ]);

    case!(socket_v4_socket, SocketAddr, SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8080)), [
        0,
        127, 0, 0, 1,
        144, 31,
    ]);

    case!(socket_v6_socket, SocketAddr, SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8080, 0, 0)), [
        1,
        0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 1,
        144, 31,
    ]);
}
