//! This machine's own addresses — `getipv4addr` and `getipv6addr`.
//!
//! Not a transport, and here anyway: it is the network stack, this is the
//! crate that owns the socket layer and the `libc` dependency, and the
//! alternative was `tt-macro` growing one for a single command.
//!
//! Upstream asks Winsock two different questions and gets two differently
//! shaped answers, which is why this is two filters rather than one with a
//! family argument. See [`local_ip_addresses`].

/// Every local address of the requested family, rendered the way a macro
/// expects to read it.
///
/// `None` is "could not ask", which the macro reads as `result` -1. Upstream
/// answers that when `WSAStartup` fails, or when `GetAdaptersAddresses` is not
/// in `iphlpapi.dll` at all (`ttl.cpp:2534`) — an OS old enough that the whole
/// question is moot. An **empty list is not a failure**: a machine with IPv6
/// switched off has no addresses and no error.
///
/// Two things about the filtering are upstream's rather than obvious, and the
/// two halves do not match each other:
///
/// - **IPv4 is one address per interface, and skips anything down or
///   loopback.** `SIO_GET_INTERFACE_LIST` answers with one `INTERFACE_INFO`
///   per interface, so an interface holding a second address never shows it,
///   and `ttl.cpp:2472` drops the rest on `IFF_UP` and `IFF_LOOPBACK`. Here
///   `getifaddrs` reports *every* address, so the first per interface is taken
///   to keep the shapes the same.
/// - **IPv6 filters on none of that.** It takes every unicast address of every
///   adapter, up or down, loopback included, and asks only that Windows call
///   it `IP_ADAPTER_ADDRESS_DNS_ELIGIBLE` — which has no Linux equivalent:
///   the flag is Windows' own judgement about what may be published in DNS,
///   and `getifaddrs` has nothing that means it. So Linux lists a link-local
///   `fe80::` address Windows might withhold; Windows uses the native flag.
#[cfg(unix)]
pub fn local_ip_addresses(v6: bool) -> Option<Vec<String>> {
    use std::ffi::CStr;

    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` writes a list we own and free below; every
    // dereference past this point is guarded on the pointer it came from.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut p = head;
    while !p.is_null() {
        let ifa = unsafe { &*p };
        p = ifa.ifa_next;
        if ifa.ifa_addr.is_null() {
            continue;
        }
        let family = i32::from(unsafe { (*ifa.ifa_addr).sa_family });
        if v6 {
            if family != libc::AF_INET6 {
                continue;
            }
            let sa = ifa.ifa_addr as *const libc::sockaddr_in6;
            out.push(render_v6(&unsafe { (*sa).sin6_addr }.s6_addr));
        } else {
            if family != libc::AF_INET
                || ifa.ifa_flags & libc::IFF_UP as u32 == 0
                || ifa.ifa_flags & libc::IFF_LOOPBACK as u32 != 0
            {
                continue;
            }
            // One per interface, which is all Winsock's answer can hold.
            let name = unsafe { CStr::from_ptr(ifa.ifa_name) }.to_bytes().to_vec();
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            let sa = ifa.ifa_addr as *const libc::sockaddr_in;
            let addr = u32::from_be(unsafe { (*sa).sin_addr }.s_addr);
            out.push(std::net::Ipv4Addr::from(addr).to_string());
        }
    }
    // SAFETY: `head` is what `getifaddrs` handed over and has not been moved;
    // `p` walked a copy.
    unsafe { libc::freeifaddrs(head) };
    Some(out)
}

#[cfg(windows)]
pub fn local_ip_addresses(v6: bool) -> Option<Vec<String>> {
    if v6 {
        windows_ipv6()
    } else {
        windows_ipv4()
    }
}

#[cfg(not(any(unix, windows)))]
pub fn local_ip_addresses(_v6: bool) -> Option<Vec<String>> {
    None
}

#[cfg(windows)]
fn windows_ipv4() -> Option<Vec<String>> {
    use windows_sys::Win32::Networking::WinSock::{
        WSAIoctl, WSASocketW, WSAStartup, AF_INET, IFF_LOOPBACK, IFF_UP, INTERFACE_INFO,
        INVALID_SOCKET, IPPROTO_UDP, SOCKET_ERROR, SOCK_DGRAM, WSADATA,
    };

    // SIO_GET_INTERFACE_LIST is `_IOR('t', 127, u_long)`. windows-sys omits
    // the macro-generated constant, so retain the SDK value beside its use.
    const SIO_GET_INTERFACE_LIST: u32 = 0x4004_747f;
    const MAX_IPADDR: usize = 30;

    let mut data = WSADATA::default();
    // SAFETY: `data` is a live output structure. Like upstream, failure to
    // initialise Winsock is the one condition reported as "cannot retrieve".
    if unsafe { WSAStartup(0x0202, &mut data) } != 0 {
        return None;
    }
    let _winsock = WinsockCleanup;

    // SAFETY: no protocol-info structure is supplied and no overlapped mode
    // is requested. This is the same datagram socket upstream uses only for
    // the interface-list ioctl.
    let socket = unsafe {
        WSASocketW(
            i32::from(AF_INET),
            SOCK_DGRAM,
            IPPROTO_UDP,
            std::ptr::null(),
            0,
            0,
        )
    };
    if socket == INVALID_SOCKET {
        return Some(Vec::new());
    }
    let socket = WinsockSocket(socket);

    let mut interfaces = [INTERFACE_INFO::default(); MAX_IPADDR];
    let mut bytes = 0;
    // SAFETY: the input pointers are null with zero length, and `interfaces`
    // is a writable output buffer whose exact byte length is supplied.
    let result = unsafe {
        WSAIoctl(
            socket.0,
            SIO_GET_INTERFACE_LIST,
            std::ptr::null(),
            0,
            interfaces.as_mut_ptr().cast(),
            std::mem::size_of_val(&interfaces) as u32,
            &mut bytes,
            std::ptr::null_mut(),
            None,
        )
    };
    if result == SOCKET_ERROR {
        return Some(Vec::new());
    }

    let count = (bytes as usize / std::mem::size_of::<INTERFACE_INFO>()).min(MAX_IPADDR);
    let mut out = Vec::new();
    for interface in &interfaces[..count] {
        if interface.iiFlags & IFF_UP == 0 || interface.iiFlags & IFF_LOOPBACK != 0 {
            continue;
        }
        // SAFETY: SIO_GET_INTERFACE_LIST filled this union as `AddressIn` for
        // the AF_INET socket. The byte fields avoid host-endian ambiguity.
        let addr = unsafe { interface.iiAddress.AddressIn.sin_addr.S_un.S_un_b };
        out.push(std::net::Ipv4Addr::new(addr.s_b1, addr.s_b2, addr.s_b3, addr.s_b4).to_string());
    }
    Some(out)
}

#[cfg(windows)]
fn windows_ipv6() -> Option<Vec<String>> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH, IP_ADAPTER_ADDRESS_DNS_ELIGIBLE,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET6, SOCKADDR_IN6};

    // Upstream hands GetAdaptersAddresses a fixed 256-entry buffer and treats
    // every API error as a successful empty result. Keep both details rather
    // than turning the usual size-probe/retry idiom into different behaviour.
    let mut adapters = vec![IP_ADAPTER_ADDRESSES_LH::default(); 256];
    let mut bytes = std::mem::size_of_val(adapters.as_slice()) as u32;
    // SAFETY: `adapters` is a writable buffer of `bytes` bytes; the reserved
    // pointer is required to be null.
    if unsafe {
        GetAdaptersAddresses(
            u32::from(AF_INET6),
            0,
            std::ptr::null(),
            adapters.as_mut_ptr(),
            &mut bytes,
        )
    } != ERROR_SUCCESS
    {
        return Some(Vec::new());
    }

    let mut out = Vec::new();
    let mut adapter = adapters.as_mut_ptr();
    // SAFETY: a successful GetAdaptersAddresses call writes two linked lists
    // whose nodes and socket addresses live inside `adapters` until return.
    unsafe {
        while !adapter.is_null() {
            let mut unicast = (*adapter).FirstUnicastAddress;
            while !unicast.is_null() {
                let flags = (*unicast).Anonymous.Anonymous.Flags;
                let address = (*unicast).Address;
                if flags & IP_ADAPTER_ADDRESS_DNS_ELIGIBLE != 0
                    && !address.lpSockaddr.is_null()
                    && address.iSockaddrLength >= std::mem::size_of::<SOCKADDR_IN6>() as i32
                {
                    let socket = &*(address.lpSockaddr.cast::<SOCKADDR_IN6>());
                    if socket.sin6_family == AF_INET6 {
                        out.push(render_v6(&socket.sin6_addr.u.Byte));
                    }
                }
                unicast = (*unicast).Next;
            }
            adapter = (*adapter).Next;
        }
    }
    Some(out)
}

#[cfg(windows)]
struct WinsockCleanup;

#[cfg(windows)]
impl Drop for WinsockCleanup {
    fn drop(&mut self) {
        // SAFETY: constructed only after a successful WSAStartup call.
        let _ = unsafe { windows_sys::Win32::Networking::WinSock::WSACleanup() };
    }
}

#[cfg(windows)]
struct WinsockSocket(windows_sys::Win32::Networking::WinSock::SOCKET);

#[cfg(windows)]
impl Drop for WinsockSocket {
    fn drop(&mut self) {
        // SAFETY: the constructor excludes INVALID_SOCKET and owns this
        // temporary socket until the interface query is complete.
        let _ = unsafe { windows_sys::Win32::Networking::WinSock::closesocket(self.0) };
    }
}

/// `myInetNtop` (`ttl.cpp:2499`), which is **not** RFC 5952 and not
/// `Ipv6Addr::to_string`.
///
/// All sixteen bytes as `%02x` with a colon after every second one, so `::1`
/// comes out as `0000:0000:0000:0000:0000:0000:0000:0001` — no zero
/// compression, no elision, always 39 characters. Upstream wrote its own
/// because `InetNtop` was not available on the oldest Windows it supported,
/// and a script that has been comparing against that string for a decade is
/// the reason to keep it.
fn render_v6(bytes: &[u8; 16]) -> String {
    let mut s = String::with_capacity(39);
    for (i, b) in bytes.iter().enumerate() {
        s.push_str(&format!("{b:02x}"));
        if i != 15 && i % 2 == 1 {
            s.push(':');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_v6_rendering_is_upstreams_and_not_the_short_form() {
        let mut loopback = [0u8; 16];
        loopback[15] = 1;
        assert_eq!(
            render_v6(&loopback),
            "0000:0000:0000:0000:0000:0000:0000:0001"
        );
        assert_eq!(
            render_v6(&[0xff; 16]),
            "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff"
        );
        // A real one, to show the case and the byte order: 2001:db8::dead:beef.
        let mut a = [0u8; 16];
        a[0..2].copy_from_slice(&[0x20, 0x01]);
        a[2..4].copy_from_slice(&[0x0d, 0xb8]);
        a[12..16].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(render_v6(&a), "2001:0db8:0000:0000:0000:0000:dead:beef");
        // Always 39 characters, which is what makes it fit upstream's buffer.
        assert_eq!(render_v6(&a).len(), 39);
    }

    /// The enumeration itself, asserted on properties rather than on this
    /// machine's addresses — which belong to the current environment and
    /// change.
    #[test]
    fn the_addresses_are_this_machines_own() {
        let v4 = local_ip_addresses(false).expect("the platform interface API should answer");
        for a in &v4 {
            let parsed: std::net::Ipv4Addr = a.parse().expect(a);
            assert!(!parsed.is_loopback(), "loopback is filtered out: {a}");
        }
        let mut sorted = v4.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), v4.len(), "one address per interface: {v4:?}");

        // The v6 form is long-hand, and the check is that it still parses:
        // an expanded address is valid input even though it is not what
        // `to_string` would produce.
        for a in local_ip_addresses(true).expect("the platform interface API should answer") {
            assert_eq!(a.len(), 39, "{a}");
            a.parse::<std::net::Ipv6Addr>().expect(&a);
        }
    }
}
