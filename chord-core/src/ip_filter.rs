//! The address filter for a fetch that a remote entity can steer: a URL from a space
//! owner, or from a link in a message. A connection goes to a public address only. The
//! desktop link preview uses the same filter.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || a == 0 // 0.0.0.0/8, "this network"
        || (a == 100 && (64..=127).contains(&b)) // 100.64.0.0/10, CGNAT
        || (a == 192 && b == 0 && c == 0) // 192.0.0.0/24, IETF protocol assignments
        || (a == 192 && b == 0 && c == 2) // 192.0.2.0/24, documentation
        || (a == 198 && b == 51 && c == 100) // 198.51.100.0/24, documentation
        || (a == 203 && b == 0 && c == 113) // 203.0.113.0/24, documentation
        || (a == 198 && (b == 18 || b == 19)) // 198.18.0.0/15, benchmarking
        || (a == 192 && b == 88 && c == 99) // 192.88.99.0/24, old 6to4 relay anycast
        || (a == 192 && b == 31 && c == 196) // 192.31.196.0/24, AS112-v4
        || a >= 240) // 240.0.0.0/4, reserved
}

pub fn is_public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    // ::ffff:a.b.c.d and the NAT64 prefix 64:ff9b::/96 carry an IPv4 address.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    if s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        let [hi, lo] = [s[6].to_be_bytes(), s[7].to_be_bytes()];
        return is_public_v4(Ipv4Addr::new(hi[0], hi[1], lo[0], lo[1]));
    }
    // The deprecated IPv4-compatible form ::a.b.c.d (but not :: and ::1, checked below).
    if s[..6] == [0, 0, 0, 0, 0, 0] && (s[6] != 0 || s[7] > 1) {
        let [hi, lo] = [s[6].to_be_bytes(), s[7].to_be_bytes()];
        return is_public_v4(Ipv4Addr::new(hi[0], hi[1], lo[0], lo[1]));
    }
    // 6to4, 2002:a.b.c.d::/48: the IPv4 address is in the second and third segments.
    if s[0] == 0x2002 {
        let [hi, lo] = [s[1].to_be_bytes(), s[2].to_be_bytes()];
        if !is_public_v4(Ipv4Addr::new(hi[0], hi[1], lo[0], lo[1])) {
            return false;
        }
    }
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00 // fc00::/7, unique local
        || (s[0] & 0xffc0) == 0xfe80 // fe80::/10, link local
        || (s[0] & 0xffc0) == 0xfec0 // fec0::/10, site local (deprecated)
        || (s[0] == 0x2001 && s[1] == 0x0db8) // 2001:db8::/32, documentation
        || (s[0] == 0x2001 && s[1] == 0) // 2001::/32, Teredo: it holds an IPv4 address
        || (s[0] == 0x64 && s[1] == 0xff9b && s[2] == 1) // 64:ff9b:1::/48, NAT64 local use
        || (s[..4] == [0x100, 0, 0, 0])) // 100::/64, discard only
}

/// True if a connection to `ip` is allowed: a public address only.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

/// Look up a host name and keep the public addresses. An error when none is left. The
/// check runs on the answer of the DNS, so a name that points to a private address (or
/// a DNS rebinding trick) cannot bypass it.
#[cfg(feature = "native-session")]
pub async fn resolve_public(host: &str) -> std::io::Result<Vec<std::net::SocketAddr>> {
    let found = tokio::net::lookup_host((host, 0)).await?;
    let public: Vec<std::net::SocketAddr> = found.filter(|a| is_public_ip(a.ip())).collect();
    if public.is_empty() {
        return Err(std::io::Error::other("the host has no public address"));
    }
    Ok(public)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn public_addresses_pass() {
        for s in [
            "8.8.8.8",
            "93.184.216.34",
            "100.63.255.255",
            "172.32.0.1",
            "192.88.98.1",
            "192.31.195.1",
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
            "2002:0808:0808::1",
        ] {
            assert!(is_public_ip(ip(s)), "{s} must pass");
        }
    }

    #[test]
    fn private_ranges_fail() {
        for s in [
            "127.0.0.1",
            "0.0.0.0",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "192.0.2.1",
            "198.18.0.1",
            "240.0.0.1",
            "::",
            "::1",
            "fc00::1",
            "fe80::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:7f00:1",
            "64:ff9b::7f00:1",
            "::127.0.0.1",
            "2002:c0a8:0101::1",
        ] {
            assert!(!is_public_ip(ip(s)), "{s} must fail");
        }
    }

    #[test]
    fn the_missing_ranges_fail() {
        for s in [
            // Teredo, 2001::/32.
            "2001::1",
            "2001:0:4136:e378:8000:63bf:3fff:fdd2",
            "2001:0:ffff:ffff:ffff:ffff:ffff:ffff",
            // NAT64 local use, 64:ff9b:1::/48.
            "64:ff9b:1::1",
            "64:ff9b:1:ffff:ffff:ffff:ffff:ffff",
            // The discard prefix, 100::/64.
            "100::1",
            "100::ffff:ffff:ffff:ffff",
            // Old 6to4 relay anycast and AS112-v4.
            "192.88.99.1",
            "192.88.99.255",
            "192.31.196.1",
            "192.31.196.255",
        ] {
            assert!(!is_public_ip(ip(s)), "{s} must fail");
        }
        // The next prefixes stay public.
        assert!(is_public_ip(ip("2001:1::1")));
        assert!(is_public_ip(ip("64:ff9b:2::1")));
        assert!(is_public_ip(ip("100:0:0:1::1")));
    }

    #[cfg(feature = "native-session")]
    #[tokio::test]
    async fn a_host_name_for_the_loopback_has_no_public_address() {
        for host in ["localhost", "localhost.", "127.0.0.1", "::1"] {
            assert!(resolve_public(host).await.is_err(), "{host}");
        }
    }
}
