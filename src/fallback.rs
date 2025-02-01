use std::{
    mem::MaybeUninit,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs},
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::Duration,
};

use crate::ToSocketAddrsTimeout;

fn resolve_timeout(
    v: &str,
    port: u16,
    timeout: Duration,
) -> std::io::Result<std::vec::IntoIter<SocketAddr>> {
    if v.len() > 253 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid socket address",
        ));
    }
    let (tx, rx) = mpsc::sync_channel(1);
    {
        let mut buffer = MaybeUninit::<[u8; 253]>::uninit();
        let len = v.len();
        let buffer = unsafe {
            (*buffer.as_mut_ptr())
                .get_unchecked_mut(..len)
                .copy_from_slice(v.as_bytes());
            buffer.assume_init()
        };
        thread::spawn(move || {
            let v = unsafe { std::str::from_utf8_unchecked(buffer.get_unchecked(..len)) };
            tx.send((v, port).to_socket_addrs())
        });
    }
    match rx.recv_timeout(timeout) {
        Ok(v) => v,
        Err(c) => match c {
            RecvTimeoutError::Timeout => Err(std::io::ErrorKind::TimedOut.into()),
            RecvTimeoutError::Disconnected => unreachable!(),
        },
    }
}

impl ToSocketAddrsTimeout for str {
    type Iter = std::vec::IntoIter<SocketAddr>;

    fn to_socket_addrs_timeout(
        &self,
        timeout: Duration,
    ) -> std::io::Result<std::vec::IntoIter<SocketAddr>> {
        if let Ok(addr) = self.parse() {
            return Ok(vec![addr].into_iter());
        }

        let (host, port_str) = self.rsplit_once(':').ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid socket address")
        })?;
        let port: u16 = port_str.parse().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid port value")
        })?;

        resolve_timeout(host, port, timeout)
    }
}

impl ToSocketAddrsTimeout for (&str, u16) {
    type Iter = std::vec::IntoIter<SocketAddr>;

    fn to_socket_addrs_timeout(
        &self,
        timeout: Duration,
    ) -> ::std::io::Result<std::vec::IntoIter<SocketAddr>> {
        let (host, port) = *self;

        if let Ok(addr) = host.parse::<Ipv4Addr>() {
            let addr = SocketAddrV4::new(addr, port);
            return Ok(vec![SocketAddr::V4(addr)].into_iter());
        }
        if let Ok(addr) = host.parse::<Ipv6Addr>() {
            let addr = SocketAddrV6::new(addr, port, 0, 0);
            return Ok(vec![SocketAddr::V6(addr)].into_iter());
        }

        resolve_timeout(host, port, timeout)
    }
}
