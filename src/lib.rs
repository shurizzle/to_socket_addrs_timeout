// TODO:
// - macos https://developer.apple.com/documentation/dnssd/dnsservicegetaddrinfo(_:_:_:_:_:_:_:) - https://eggerapps.at/blog/2014/hostname-lookups.html

use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    option,
    time::Duration,
};

#[cfg(not(windows))]
mod fallback;
#[cfg(windows)]
mod windows;

pub trait ToSocketAddrsTimeout {
    type Iter: Iterator<Item = SocketAddr>;

    fn to_socket_addrs_timeout(&self, timeout: Duration) -> io::Result<Self::Iter>;
}

impl<'a> ToSocketAddrsTimeout for &'a [SocketAddr] {
    type Iter = std::iter::Cloned<std::slice::Iter<'a, SocketAddr>>;

    fn to_socket_addrs_timeout(&self, _timeout: Duration) -> io::Result<Self::Iter> {
        Ok(self.iter().cloned())
    }
}

impl<T: ToSocketAddrsTimeout + ?Sized> ToSocketAddrsTimeout for &T {
    type Iter = T::Iter;

    fn to_socket_addrs_timeout(&self, timeout: Duration) -> io::Result<T::Iter> {
        (**self).to_socket_addrs_timeout(timeout)
    }
}

impl ToSocketAddrsTimeout for SocketAddr {
    type Iter = option::IntoIter<SocketAddr>;

    fn to_socket_addrs_timeout(
        &self,
        _timeout: Duration,
    ) -> io::Result<std::option::IntoIter<SocketAddr>> {
        Ok(Some(*self).into_iter())
    }
}

impl ToSocketAddrsTimeout for SocketAddrV4 {
    type Iter = option::IntoIter<SocketAddr>;

    fn to_socket_addrs_timeout(
        &self,
        timeout: Duration,
    ) -> io::Result<option::IntoIter<SocketAddr>> {
        SocketAddr::V4(*self).to_socket_addrs_timeout(timeout)
    }
}

impl ToSocketAddrsTimeout for SocketAddrV6 {
    type Iter = option::IntoIter<SocketAddr>;

    fn to_socket_addrs_timeout(
        &self,
        timeout: Duration,
    ) -> io::Result<option::IntoIter<SocketAddr>> {
        SocketAddr::V6(*self).to_socket_addrs_timeout(timeout)
    }
}

impl ToSocketAddrsTimeout for (IpAddr, u16) {
    type Iter = option::IntoIter<SocketAddr>;
    fn to_socket_addrs_timeout(
        &self,
        timeout: Duration,
    ) -> io::Result<option::IntoIter<SocketAddr>> {
        let (ip, port) = *self;
        match ip {
            IpAddr::V4(ref a) => (*a, port).to_socket_addrs_timeout(timeout),
            IpAddr::V6(ref a) => (*a, port).to_socket_addrs_timeout(timeout),
        }
    }
}

impl ToSocketAddrsTimeout for (Ipv4Addr, u16) {
    type Iter = option::IntoIter<SocketAddr>;
    fn to_socket_addrs_timeout(
        &self,
        timeout: Duration,
    ) -> io::Result<option::IntoIter<SocketAddr>> {
        let (ip, port) = *self;
        SocketAddrV4::new(ip, port).to_socket_addrs_timeout(timeout)
    }
}

impl ToSocketAddrsTimeout for (Ipv6Addr, u16) {
    type Iter = option::IntoIter<SocketAddr>;
    fn to_socket_addrs_timeout(
        &self,
        timeout: Duration,
    ) -> io::Result<option::IntoIter<SocketAddr>> {
        let (ip, port) = *self;
        SocketAddrV6::new(ip, port, 0, 0).to_socket_addrs_timeout(timeout)
    }
}

impl ToSocketAddrsTimeout for String {
    type Iter = std::vec::IntoIter<SocketAddr>;

    #[inline]
    fn to_socket_addrs_timeout(&self, timeout: Duration) -> ::std::io::Result<Self::Iter> {
        (**self).to_socket_addrs_timeout(timeout)
    }
}

impl ToSocketAddrsTimeout for (String, u16) {
    type Iter = std::vec::IntoIter<SocketAddr>;

    fn to_socket_addrs_timeout(
        &self,
        timeout: Duration,
    ) -> std::io::Result<std::vec::IntoIter<SocketAddr>> {
        (&*self.0, self.1).to_socket_addrs_timeout(timeout)
    }
}
