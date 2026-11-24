use rust_i18n::t;
use windows::Win32::Foundation::ERROR_BUFFER_OVERFLOW;
use windows::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_INCLUDE_PREFIX, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
    GAA_FLAG_SKIP_MULTICAST, GET_ADAPTERS_ADDRESSES_FLAGS, GetAdaptersAddresses,
    IP_ADAPTER_ADDRESSES_LH, IP_ADAPTER_UNICAST_ADDRESS_LH,
};
use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpInfo {

    pub ip: String,

    pub prefix: u8,
}

impl IpInfo {

    #[cfg(test)]
    pub fn cidr(&self) -> String {
        network_cidr(self)
    }
}

#[derive(Clone, Debug)]
pub struct AdapterInfo {

    pub guid: String,

    pub friendly: String,

    pub description: String,

    pub ips: Vec<IpInfo>,
}

impl AdapterInfo {

    pub fn label(&self) -> String {
        if self.description.is_empty() {
            self.friendly.clone()
        } else {
            t!("netinfo.adapter_label", name = self.friendly, description = self.description).to_string()
        }
    }
}

fn mask_bits(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32)) as u32
    }
}

pub fn network_cidr(ip: &IpInfo) -> String {
    match parse_ipv4(&ip.ip) {
        Some(addr) => format!(
            "{}/{}",
            format_ipv4((addr & mask_bits(ip.prefix)).to_be_bytes()),
            ip.prefix
        ),

        None => format!("{}/{}", ip.ip, ip.prefix),
    }
}

fn format_ipv4(b: [u8; 4]) -> String {
    format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
}

pub fn next_ip(ip: &str) -> Option<String> {
    let addr = parse_ipv4(ip)?;
    let mut bytes = addr.to_be_bytes();
    if bytes[3] >= 254 {
        return None;
    }
    bytes[3] += 1;
    Some(format_ipv4(bytes))
}

fn parse_ipv4(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut bytes = [0u8; 4];
    for (b, p) in bytes.iter_mut().zip(&parts) {
        *b = p.parse().ok()?;
    }
    Some(u32::from_be_bytes(bytes))
}

pub fn subnet_labels(ips: &[IpInfo]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for ip in ips {
        let cidr = network_cidr(ip);
        if !out.contains(&cidr) {
            out.push(cidr);
        }
    }
    out
}

/// Whether the adapter named `friendly` has the IPv4 address `ip` right now.
pub fn adapter_has_ip(friendly: &str, ip: &str) -> bool {
    list_adapters()
        .iter()
        .any(|a| a.friendly == friendly && a.ips.iter().any(|i| i.ip == ip))
}

pub fn list_adapters() -> Vec<AdapterInfo> {
    let mut adapters = Vec::new();
    let flags = GET_ADAPTERS_ADDRESSES_FLAGS(
        GAA_FLAG_SKIP_ANYCAST.0
            | GAA_FLAG_SKIP_MULTICAST.0
            | GAA_FLAG_SKIP_DNS_SERVER.0
            | GAA_FLAG_INCLUDE_PREFIX.0,
    );
    let family = AF_INET.0 as u32;

    let mut ok = false;
    let mut size: u32 = 0;
    let mut buf: Vec<u8> = Vec::new();
    for _ in 0..4 {
        if size == 0 {
            let rc = unsafe { GetAdaptersAddresses(family, flags, None, None, &mut size) };
            if rc != ERROR_BUFFER_OVERFLOW.0 {
                return adapters;
            }
        }
        buf.resize(size as usize, 0);
        let rc = unsafe {
            GetAdaptersAddresses(
                family,
                flags,
                None,
                Some(buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>()),
                &mut size,
            )
        };
        if rc == 0 {
            ok = true;
            break;
        }
        if rc != ERROR_BUFFER_OVERFLOW.0 {
            return adapters;
        }

        size = 0;
    }
    if !ok {
        return adapters;
    }

    let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !cur.is_null() {
        let aa = unsafe { &*cur };
        cur = aa.Next;

        if aa.IfType == 24 || aa.OperStatus.0 != 1 {
            continue;
        }
        let mut ips = Vec::new();
        let mut unicast = aa.FirstUnicastAddress;
        while !unicast.is_null() {
            let ua: &IP_ADAPTER_UNICAST_ADDRESS_LH = unsafe { &*unicast };
            unicast = ua.Next;
            if let Some(info) = unicast_ipv4(ua) {
                ips.push(info);
            }
        }
        adapters.push(AdapterInfo {
            guid: pstr_to_string(aa.AdapterName),
            friendly: pwstr_to_string(aa.FriendlyName),
            description: pwstr_to_string(aa.Description),
            ips,
        });
    }

    adapters.sort_by(|a, b| a.friendly.to_lowercase().cmp(&b.friendly.to_lowercase()));
    adapters
}

fn unicast_ipv4(ua: &IP_ADAPTER_UNICAST_ADDRESS_LH) -> Option<IpInfo> {
    let sa = ua.Address.lpSockaddr;
    if sa.is_null() {
        return None;
    }
    unsafe {
        if (*sa).sa_family != AF_INET {
            return None;
        }
        let sin = &*(sa as *const SOCKADDR_IN);

        let addr = u32::from_be(sin.sin_addr.S_un.S_addr);
        Some(IpInfo {
            ip: format_ipv4(addr.to_be_bytes()),
            prefix: ua.OnLinkPrefixLength,
        })
    }
}

fn pwstr_to_string(p: windows::core::PWSTR) -> String {
    if p.0.is_null() {
        return String::new();
    }
    unsafe {
        let mut len = 0usize;
        while *p.0.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p.0, len))
    }
}

fn pstr_to_string(p: windows::core::PSTR) -> String {
    if p.0.is_null() {
        return String::new();
    }
    unsafe {
        std::ffi::CStr::from_ptr(p.0.cast())
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_covers_prefix_edges() {

        assert_eq!(
            network_cidr(&IpInfo {
                ip: "192.168.100.114".into(),
                prefix: 0
            }),
            "0.0.0.0/0"
        );
        assert_eq!(
            network_cidr(&IpInfo {
                ip: "10.255.3.7".into(),
                prefix: 8
            }),
            "10.0.0.0/8"
        );
        assert_eq!(
            network_cidr(&IpInfo {
                ip: "192.168.1.6".into(),
                prefix: 30
            }),
            "192.168.1.4/30"
        );
        assert_eq!(
            network_cidr(&IpInfo {
                ip: "1.2.3.4".into(),
                prefix: 32
            }),
            "1.2.3.4/32"
        );
    }

    #[test]
    fn cidr_from_ip_and_prefix() {
        assert_eq!(
            network_cidr(&IpInfo {
                ip: "192.168.100.114".into(),
                prefix: 24
            }),
            "192.168.100.0/24"
        );
        assert_eq!(
            network_cidr(&IpInfo {
                ip: "10.255.3.7".into(),
                prefix: 8
            }),
            "10.0.0.0/8"
        );
        assert_eq!(
            network_cidr(&IpInfo {
                ip: "172.16.5.9".into(),
                prefix: 20
            }),
            "172.16.0.0/20"
        );
    }

    #[test]
    fn next_ip_increments_last_octet() {
        assert_eq!(next_ip("192.168.100.5").as_deref(), Some("192.168.100.6"));
        assert_eq!(next_ip("10.0.0.1").as_deref(), Some("10.0.0.2"));

        assert_eq!(next_ip("192.168.100.254"), None);
        assert_eq!(next_ip("192.168.100.255"), None);
        assert_eq!(next_ip("not-an-ip"), None);
    }

    #[test]
    fn subnet_labels_dedupe() {
        let ips = vec![
            IpInfo {
                ip: "192.168.100.114".into(),
                prefix: 24,
            },

            IpInfo {
                ip: "192.168.100.55".into(),
                prefix: 24,
            },
            IpInfo {
                ip: "10.1.2.3".into(),
                prefix: 8,
            },
        ];
        assert_eq!(subnet_labels(&ips), vec!["192.168.100.0/24", "10.0.0.0/8"]);
        assert_eq!(ips[0].cidr(), "192.168.100.0/24");
    }

    #[test]
    fn adapter_label_combines_friendly_parts() {
        let ad = AdapterInfo {
            guid: "g".into(),
            friendly: "以太网".into(),
            description: "Realtek PCIe".into(),
            ips: Vec::new(),
        };
        assert_eq!(ad.label(), t!("netinfo.adapter_label", name = "以太网", description = "Realtek PCIe"));
        let no_desc = AdapterInfo {
            description: String::new(),
            ..ad
        };
        assert_eq!(no_desc.label(), "以太网");
    }

    #[test]
    fn list_adapters_smoke() {
        let adapters = list_adapters();
        for a in &adapters {
            assert!(!a.guid.is_empty(), "the adapter GUID should not be empty");
            assert!(!a.friendly.is_empty(), "the adapter FriendlyName should not be empty");
            for ip in &a.ips {
                assert!(parse_ipv4(&ip.ip).is_some(), "invalid IPv4: {}", ip.ip);
                assert!(ip.prefix <= 32);
                assert!(network_cidr(ip).contains('/'));
            }
        }
    }
}
