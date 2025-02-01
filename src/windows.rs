#![allow(clippy::missing_transmute_annotations)]

use std::{
    ffi::OsStr,
    mem::offset_of,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs},
    os::windows::ffi::OsStrExt,
    sync::OnceLock,
    time::Duration,
};

use windows::Win32::{
    Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE},
    Networking::WinSock::{
        FreeAddrInfoExW, GetAddrInfoExW, ADDRINFOEXW, AF_INET, AF_INET6, AF_UNSPEC, NS_ALL,
        SOCKADDR_IN, SOCKADDR_IN6, SOCKADDR_STORAGE, SOCK_STREAM, TIMEVAL, WSA_IO_PENDING,
    },
    System::{
        Threading::{CreateEventW, SetEvent, WaitForSingleObject, INFINITE},
        IO::OVERLAPPED,
    },
};
use windows_core::PCWSTR;

use crate::ToSocketAddrsTimeout;

static WSA_START: OnceLock<()> = OnceLock::new();

fn init() {
    WSA_START.get_or_init(|| {
        // HACK: dirty trick to make Rust call WASStartup
        _ = "----.--:80".to_socket_addrs();
    });
}

struct Context {
    pub query_overlapped: OVERLAPPED,
    pub complete_event: HANDLE,
    pub query_result: *mut ADDRINFOEXW,
    pub result: std::io::Result<LookupHost>,
}

impl Context {
    pub fn new() -> std::io::Result<Self> {
        let complete_event = unsafe { CreateEventW(None, true, false, None)? };

        Ok(Self {
            query_overlapped: unsafe { core::mem::zeroed() },
            complete_event,
            query_result: unsafe { core::mem::zeroed() },
            result: Err(std::io::ErrorKind::Other.into()),
        })
    }

    #[inline(always)]
    pub fn set_event(&mut self) {
        unsafe { SetEvent(self.complete_event) }.unwrap();
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.complete_event) }.unwrap();
    }
}

unsafe extern "system" fn query_complete_callback(
    error: u32,
    _bytes: u32,
    overlapped: *const OVERLAPPED,
) {
    let ctx = &mut *(overlapped
        .cast::<u8>()
        .sub(offset_of!(Context, query_overlapped)) as *mut Context);

    let lh = LookupHost {
        original: ctx.query_result,
        cur: ctx.query_result,
        port: 0,
    };
    ctx.query_result = core::ptr::null_mut();
    ctx.result = if error == ERROR_SUCCESS.0 {
        Ok(lh)
    } else {
        drop(lh);
        Err(std::io::Error::from_raw_os_error(error as _))
    };

    ctx.set_event();
}

fn getaddrinfo_timeout(name: &[u16], timeout: Duration) -> std::io::Result<LookupHost> {
    init();

    let mut hints: ADDRINFOEXW = unsafe { core::mem::zeroed() };
    hints.ai_family = AF_UNSPEC.0 as _;
    hints.ai_socktype = SOCK_STREAM.0 as _;

    let tv = TIMEVAL {
        tv_sec: timeout.as_secs() as _,
        tv_usec: timeout.subsec_micros() as _,
    };

    let mut ctx = Context::new()?;

    let ret = unsafe {
        GetAddrInfoExW(
            PCWSTR(name.as_ptr().cast()),
            None,
            NS_ALL,
            None,
            Some(&hints),
            &mut ctx.query_result,
            Some(&tv),
            Some(&ctx.query_overlapped),
            Some(Some(query_complete_callback)),
            None,
        )
    };

    if ret != WSA_IO_PENDING.0 {
        unsafe { query_complete_callback(ret as _, 0, &ctx.query_overlapped) };
    }

    assert_eq!(
        unsafe { WaitForSingleObject(ctx.complete_event, INFINITE).0 },
        0
    );

    let mut result = Err(std::io::ErrorKind::Other.into());
    core::mem::swap(&mut ctx.result, &mut result);
    result
}

struct LookupHost {
    original: *mut ADDRINFOEXW,
    cur: *mut ADDRINFOEXW,
    port: u16,
}

impl LookupHost {
    pub fn port(&self) -> u16 {
        self.port
    }
}

fn sockaddr_to_addr(storage: &SOCKADDR_STORAGE, len: usize) -> std::io::Result<SocketAddr> {
    match storage.ss_family as _ {
        AF_INET => {
            assert!(len >= core::mem::size_of::<SOCKADDR_IN>());
            let addr = unsafe { core::mem::transmute::<_, &SOCKADDR_IN>(storage) };
            Ok(SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::from(unsafe { addr.sin_addr.S_un.S_addr.to_ne_bytes() }),
                u16::from_be(addr.sin_port),
            )))
        }
        AF_INET6 => {
            assert!(len >= core::mem::size_of::<SOCKADDR_IN6>());
            let addr = unsafe { core::mem::transmute::<_, &SOCKADDR_IN6>(storage) };
            Ok(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::from(unsafe { addr.sin6_addr.u.Byte }),
                u16::from_be(addr.sin6_port),
                addr.sin6_flowinfo,
                0,
            )))
        }
        _ => Err(std::io::ErrorKind::InvalidInput.into()),
    }
}

impl Iterator for LookupHost {
    type Item = SocketAddr;

    fn next(&mut self) -> Option<SocketAddr> {
        loop {
            let cur = unsafe { self.cur.as_ref()? };
            self.cur = cur.ai_next;
            match sockaddr_to_addr(
                unsafe { &*(cur.ai_addr as *const SOCKADDR_STORAGE) },
                cur.ai_addrlen,
            ) {
                Ok(addr) => return Some(addr),
                Err(_) => continue,
            }
        }
    }
}

impl Drop for LookupHost {
    fn drop(&mut self) {
        unsafe { FreeAddrInfoExW(Some(self.original)) };
    }
}

unsafe impl Sync for LookupHost {}
unsafe impl Send for LookupHost {}

fn to_wide<T: AsRef<OsStr>>(s: T) -> std::io::Result<Vec<u16>> {
    if s.as_ref().as_encoded_bytes().contains(&b'\0') {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "host name contained an unexpected NUL byte",
        ))
    } else {
        Ok(s.as_ref().encode_wide().chain(Some(0)).collect())
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
        let hostname = to_wide(hostname)?;
        let mut me = getaddrinfo_timeout(&hostname, timeout)?;
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
