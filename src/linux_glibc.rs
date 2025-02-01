use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    time::{Duration, Instant},
};

use crate::ToSocketAddrsTimeout;

#[repr(C)]
#[allow(non_camel_case_types)]
struct gaicb {
    pub name: *const ::core::ffi::c_char,
    pub service: *const ::core::ffi::c_char,
    pub request: *const libc::addrinfo,
    pub addrinfo: *mut libc::addrinfo,
    __return: ::core::ffi::c_int,
    __glibc_reserved: [::core::ffi::c_int; 5],
}

impl gaicb {
    pub unsafe fn new(
        name: &::core::ffi::CStr,
        service: Option<&::core::ffi::CStr>,
        request: Option<&libc::addrinfo>,
    ) -> Self {
        Self {
            name: name.as_ptr(),
            service: service
                .map(::core::ffi::CStr::as_ptr)
                .unwrap_or_else(core::ptr::null),
            request: request
                .map(|x| x as *const libc::addrinfo)
                .unwrap_or_else(core::ptr::null),
            addrinfo: unsafe { core::mem::zeroed() },
            __return: unsafe { core::mem::zeroed() },
            __glibc_reserved: unsafe { core::mem::zeroed() },
        }
    }
}

#[repr(transparent)]
#[must_use]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct AddressInfoError(::core::ffi::c_int);

impl core::fmt::Display for AddressInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[link(name = "c")]
        extern "C" {
            fn gai_strerror(errcode: ::core::ffi::c_int) -> *const core::ffi::c_char;
        }

        unsafe {
            let ptr = gai_strerror(self.0);
            if ptr.is_null() {
                f.write_str("unknown")
            } else {
                core::fmt::Display::fmt(&::core::ffi::CStr::from_ptr(ptr).to_string_lossy(), f)
            }
        }
    }
}

#[allow(non_upper_case_globals)]
impl AddressInfoError {
    /// Invalid value for `ai_flags' field.
    pub const BadFlags: Self = Self(-1);
    /// NAME or SERVICE is unknown.
    pub const NoName: Self = Self(-2);
    /// Temporary failure in name resolution.
    pub const Again: Self = Self(-3);
    /// Non-recoverable failure in name res.
    pub const Fail: Self = Self(-4);
    /// `ai_family' not supported.
    pub const Family: Self = Self(-6);
    /// `ai_socktype' not supported.
    pub const Socktype: Self = Self(-7);
    /// SERVICE not supported for `ai_socktype'
    pub const Service: Self = Self(-8);
    /// Memory allocation failure.
    pub const Memory: Self = Self(-10);
    /// System error returned in `errno'
    pub const System: Self = Self(-11);
    /// Argument buffer overflow.
    pub const Overflow: Self = Self(-12);
    /// No address associated with NAME.
    pub const NoData: Self = Self(-5);
    /// Address family for NAME not supported.
    pub const AddrFamily: Self = Self(-9);
    /// Processing request in progress.
    pub const InProgress: Self = Self(-100);
    /// Request canceled.
    pub const Canceled: Self = Self(-101);
    /// Request not canceled.
    pub const NotCanceled: Self = Self(-102);
    /// All requests done.
    pub const AllDone: Self = Self(-103);
    /// Interrupted by a signal.
    pub const Interrupted: Self = Self(-104);
    /// IDN encoding failed.
    pub const IdnEncode: Self = Self(-105);
}

impl core::fmt::Debug for AddressInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::BadFlags => f.write_str("AddressInfoError::BadFlags"),
            Self::NoName => f.write_str("AddressInfoError::NoName"),
            Self::Again => f.write_str("AddressInfoError::Again"),
            Self::Fail => f.write_str("AddressInfoError::Fail"),
            Self::Family => f.write_str("AddressInfoError::Family"),
            Self::Socktype => f.write_str("AddressInfoError::Socktype"),
            Self::Service => f.write_str("AddressInfoError::Service"),
            Self::Memory => f.write_str("AddressInfoError::Memory"),
            Self::System => f.write_str("AddressInfoError::System"),
            Self::Overflow => f.write_str("AddressInfoError::Overflow"),
            Self::NoData => f.write_str("AddressInfoError::NoData"),
            Self::AddrFamily => f.write_str("AddressInfoError::AddrFamily"),
            Self::InProgress => f.write_str("AddressInfoError::InProgress"),
            Self::Canceled => f.write_str("AddressInfoError::Canceled"),
            Self::NotCanceled => f.write_str("AddressInfoError::NotCanceled"),
            Self::AllDone => f.write_str("AddressInfoError::AllDone"),
            Self::Interrupted => f.write_str("AddressInfoError::Interrupted"),
            Self::IdnEncode => f.write_str("AddressInfoError::IdnEncode"),
            _ => f.write_str("AddressInfoError::Unknown"),
        }
    }
}

impl std::error::Error for AddressInfoError {}

impl From<AddressInfoError> for std::io::Error {
    fn from(value: AddressInfoError) -> Self {
        if value == AddressInfoError::System {
            std::io::Error::from_raw_os_error(unsafe { *libc::__errno_location() })
        } else {
            std::io::Error::new(std::io::ErrorKind::Other, value)
        }
    }
}

struct LookupHost {
    original: *mut libc::addrinfo,
    cur: *mut libc::addrinfo,
    port: u16,
}

impl LookupHost {
    pub fn port(&self) -> u16 {
        self.port
    }
}

pub fn sockaddr_to_addr(
    storage: &libc::sockaddr_storage,
    len: usize,
) -> std::io::Result<SocketAddr> {
    match storage.ss_family as ::core::ffi::c_int {
        libc::AF_INET => {
            assert!(len >= core::mem::size_of::<libc::sockaddr_in>());
            let addr = unsafe {
                core::mem::transmute::<&libc::sockaddr_storage, &libc::sockaddr_in>(storage)
            };
            Ok(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::from(addr.sin_addr.s_addr.to_ne_bytes()),
                u16::from_be(addr.sin_port),
            )))
        }
        libc::AF_INET6 => {
            assert!(len >= core::mem::size_of::<libc::sockaddr_in6>());
            let addr = unsafe {
                core::mem::transmute::<&libc::sockaddr_storage, &libc::sockaddr_in6>(storage)
            };
            Ok(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(addr.sin6_addr.s6_addr),
                u16::from_be(addr.sin6_port),
                addr.sin6_flowinfo,
                addr.sin6_scope_id,
            )))
        }
        _ => Err(std::io::ErrorKind::InvalidInput.into()),
    }
}

impl Iterator for LookupHost {
    type Item = SocketAddr;

    fn next(&mut self) -> Option<SocketAddr> {
        loop {
            unsafe {
                let cur = self.cur.as_ref()?;
                self.cur = cur.ai_next;
                match sockaddr_to_addr(
                    &*(cur.ai_addr as *const libc::sockaddr_storage),
                    cur.ai_addrlen as usize,
                ) {
                    Ok(addr) => return Some(addr),
                    Err(_) => continue,
                }
            }
        }
    }
}

impl Drop for LookupHost {
    fn drop(&mut self) {
        unsafe { libc::freeaddrinfo(self.original) };
    }
}

unsafe impl Sync for LookupHost {}
unsafe impl Send for LookupHost {}

fn d2ts(duration: Duration) -> libc::timespec {
    libc::timespec {
        tv_sec: duration.as_secs() as _,
        tv_nsec: duration.subsec_nanos() as _,
    }
}

/// BUG: this is a bad implementation beacuse it does not handle `AddressInfoError::NotCanceled`
fn getaddrinfo_timeout(
    hostname: &::core::ffi::CStr,
    service: Option<&::core::ffi::CStr>,
    hints: Option<&libc::addrinfo>,
    timeout: Duration,
) -> std::io::Result<LookupHost> {
    struct GaicbGuard(*mut gaicb);
    impl GaicbGuard {
        pub fn run(self) -> AddressInfoError {
            let list = self.0;
            core::mem::forget(self);
            unsafe { gai_cancel(list) }
        }
    }
    impl Drop for GaicbGuard {
        fn drop(&mut self) {
            _ = unsafe { gai_cancel(self.0) };
        }
    }

    #[link(name = "c")]
    extern "C" {
        fn getaddrinfo_a(
            mode: ::core::ffi::c_int,
            list: *mut *mut gaicb,
            n: core::ffi::c_int,
            sevp: *mut libc::sigevent,
        ) -> AddressInfoError;

        fn gai_cancel(req: *mut gaicb) -> AddressInfoError;

        fn gai_error(req: *mut gaicb) -> AddressInfoError;

        fn gai_suspend(
            req: *const *const gaicb,
            n: ::core::ffi::c_int,
            timeout: *const libc::timespec,
        ) -> AddressInfoError;
    }
    const GAI_NOWAIT: ::core::ffi::c_int = 1;

    let mut host = unsafe { gaicb::new(hostname, service, hints) };
    let mut list = [&mut host as *mut gaicb];

    let mut handler: libc::sigevent = unsafe { core::mem::zeroed() };
    handler.sigev_notify = libc::SIGEV_NONE;

    let ret = unsafe { getaddrinfo_a(GAI_NOWAIT, list.as_mut_ptr(), 1, &mut handler) };
    if ret.0 != 0 {
        return Err(ret.into());
    }
    let guard = GaicbGuard(&mut host);

    let end = Instant::now() + timeout;
    loop {
        let Some(timeout) = end.checked_duration_since(Instant::now()) else {
            return Err(std::io::ErrorKind::TimedOut.into());
        };
        let ret = unsafe { gai_suspend(list.as_ptr().cast(), 1, &d2ts(timeout)) };
        if ret.0 == 0 {
            if unsafe { gai_error(&mut host) }.0 == 0 {
                return Ok(LookupHost {
                    original: host.addrinfo,
                    cur: host.addrinfo,
                    port: 0,
                });
            }
            continue;
        }
        if ret == AddressInfoError::System && unsafe { *libc::__errno_location() } == libc::EINTR {
            continue;
        }
        return if guard.run() == AddressInfoError::AllDone {
            Ok(LookupHost {
                original: host.addrinfo,
                cur: host.addrinfo,
                port: 0,
            })
        } else {
            Err(ret.into())
        };
    }
}

impl TryFrom<(&str, Duration)> for LookupHost {
    type Error = std::io::Error;

    fn try_from((s, timeout): (&str, Duration)) -> Result<Self, Self::Error> {
        let (host, port_str) = s.rsplit_once(':').ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid socket address")
        })?;
        let port: u16 = port_str.parse().map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid port value")
        })?;
        (host, port, timeout).try_into()
    }
}

impl TryFrom<(&str, u16, Duration)> for LookupHost {
    type Error = std::io::Error;

    fn try_from((hostname, port, timeout): (&str, u16, Duration)) -> Result<Self, Self::Error> {
        let hostname = ::std::ffi::CString::new(hostname).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "host name contained an unexpected NUL byte",
            )
        })?;

        let mut hints: libc::addrinfo = unsafe { core::mem::zeroed() };
        hints.ai_socktype = libc::SOCK_STREAM;

        let mut me = getaddrinfo_timeout(&hostname, None, Some(&hints), timeout)?;
        me.port = port;
        Ok(me)
    }
}

fn resolve_socket_addr(lh: LookupHost) -> std::io::Result<std::vec::IntoIter<SocketAddr>> {
    let p = lh.port();
    let v: Vec<_> = lh
        .map(|mut a| {
            a.set_port(p);
            a
        })
        .collect();
    Ok(v.into_iter())
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

        resolve_socket_addr((self, timeout).try_into()?)
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

        resolve_socket_addr((host, port, timeout).try_into()?)
    }
}
