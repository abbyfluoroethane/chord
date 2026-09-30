//! The server connector: SRV lookup for `_xmpps-client._tcp` and `_xmpp-client._tcp`, direct
//! TLS (XEP-0368) and STARTTLS (RFC 6120).
//!
//! In `Srv` mode the connector looks up both record sets and merges them by priority. On the
//! same priority, the records of direct TLS come first, then the records of STARTTLS (XEP-0368,
//! section 3). Inside one group, the weights decide the order (RFC 2782). The connector tries
//! the targets in this order, and the next one when a target fails. So a failed direct TLS
//! target falls back to STARTTLS.
//!
//! The TLS server name and the certificate name are always the domain of the JID, never the
//! host of a target.

use core::hash::{BuildHasher, Hasher};
use core::time::Duration;
use std::collections::BTreeSet;
use std::net::IpAddr;

use sasl::common::ChannelBinding;
use tokio::io::BufStream;
use tokio::net::TcpStream;
use tokio_xmpp::Error;
use tokio_xmpp::connect::tls_common::{TlsStream, establish_tls_connection_with_alpn};
use tokio_xmpp::connect::{DnsConfig, ServerConnector, SrvRecord, starttls::starttls};
use tokio_xmpp::error::ProtocolError;
use tokio_xmpp::jid::Jid;
use tokio_xmpp::xmlstream::{PendingFeaturesRecv, StreamHeader, Timeouts, initiate_stream};

/// The ALPN protocol of XEP-0368.
const ALPN_XMPP_CLIENT: &[u8] = b"xmpp-client";
const SRV_DIRECT_TLS: &str = "_xmpps-client._tcp";
const SRV_STARTTLS: &str = "_xmpp-client._tcp";

/// Time limit for one target: TCP, TLS and the stream header.
const TARGET_TIMEOUT: Duration = Duration::from_secs(10);

/// How to find the server.
#[derive(Clone, Debug)]
pub(super) enum Mode {
    /// SRV records of the JID domain, both kinds.
    Srv,
    /// One host and port, STARTTLS.
    StartTls { host: String, port: u16 },
    /// One host and port, direct TLS.
    DirectTls { host: String, port: u16 },
}

#[derive(Clone, Debug)]
pub(super) struct Connector(pub Mode);

/// One place to connect to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub host: String,
    pub port: u16,
    pub direct_tls: bool,
}

type TlsTcp = BufStream<TlsStream<TcpStream>>;

impl ServerConnector for Connector {
    type Stream = TlsTcp;

    async fn connect(
        &self,
        jid: &Jid,
        ns: &'static str,
        timeouts: Timeouts,
    ) -> Result<(PendingFeaturesRecv<Self::Stream>, ChannelBinding), Error> {
        let targets = targets(&self.0, jid.domain().as_str()).await;
        connect_any(targets, jid, ns, timeouts).await
    }
}

/// Try the targets in order. Returns the first connection, or the last error.
async fn connect_any(
    targets: Vec<Target>,
    jid: &Jid,
    ns: &'static str,
    timeouts: Timeouts,
) -> Result<(PendingFeaturesRecv<TlsTcp>, ChannelBinding), Error> {
    {
        let mut last = Error::Disconnected;
        for target in targets {
            let how = if target.direct_tls {
                "direct TLS"
            } else {
                "STARTTLS"
            };
            log::info!("connecting to {}:{} with {how}", target.host, target.port);
            match tokio::time::timeout(TARGET_TIMEOUT, connect_target(&target, jid, ns, timeouts))
                .await
            {
                Ok(Ok(connected)) => return Ok(connected),
                Ok(Err(e)) => {
                    log::info!(
                        "cannot connect to {}:{} with {how}: {e}",
                        target.host,
                        target.port
                    );
                    last = e;
                }
                Err(_) => {
                    log::info!(
                        "connecting to {}:{} with {how} timed out",
                        target.host,
                        target.port
                    );
                    last = Error::Disconnected;
                }
            }
        }
        Err(last)
    }
}

/// The targets to try, in order.
async fn targets(mode: &Mode, domain: &str) -> Vec<Target> {
    match mode {
        Mode::StartTls { host, port } => vec![Target {
            host: host.clone(),
            port: *port,
            direct_tls: false,
        }],
        Mode::DirectTls { host, port } => vec![Target {
            host: host.clone(),
            port: *port,
            direct_tls: true,
        }],
        Mode::Srv => {
            let fallback = Target {
                host: domain.to_owned(),
                port: 5222,
                direct_tls: false,
            };
            // A literal IP address has no SRV records.
            if domain.parse::<IpAddr>().is_ok() {
                return vec![fallback];
            }
            let (direct, plain) = tokio::join!(
                DnsConfig::srv_records(domain, SRV_DIRECT_TLS),
                DnsConfig::srv_records(domain, SRV_STARTTLS)
            );
            let (direct, plain) = (direct.unwrap_or_default(), plain.unwrap_or_default());
            let mut ordered = order_targets(direct, plain, &mut random);
            if ordered.is_empty() {
                // RFC 6120, 3.2.2: with no SRV record, use the domain and the port 5222.
                ordered.push(fallback);
            }
            ordered
        }
    }
}

/// Merge the SRV records of direct TLS and of STARTTLS into one list of targets.
///
/// Lower priority first. On the same priority, direct TLS before STARTTLS. Inside one
/// group, a weighted random order (RFC 2782): a record with weight 0 comes after the records
/// with a weight. A target of "." means "no service" and is dropped.
/// `random(n)` returns a number below `n`.
fn order_targets(
    direct: Vec<SrvRecord>,
    plain: Vec<SrvRecord>,
    random: &mut impl FnMut(u32) -> u32,
) -> Vec<Target> {
    let priorities: BTreeSet<u16> = direct.iter().chain(&plain).map(|r| r.priority).collect();
    let mut out: Vec<Target> = Vec::new();
    for priority in priorities {
        for (records, direct_tls) in [(&direct, true), (&plain, false)] {
            let group: Vec<&SrvRecord> = records
                .iter()
                .filter(|r| r.priority == priority && !r.target.is_empty())
                .collect();
            for record in weighted_order(group, random) {
                let target = Target {
                    host: record.target.clone(),
                    port: record.port,
                    direct_tls,
                };
                if !out.contains(&target) {
                    out.push(target);
                }
            }
        }
    }
    out
}

/// RFC 2782 selection: pick one record at a time, in proportion to its weight.
fn weighted_order<'a>(
    mut left: Vec<&'a SrvRecord>,
    random: &mut impl FnMut(u32) -> u32,
) -> Vec<&'a SrvRecord> {
    let mut out = Vec::with_capacity(left.len());
    while !left.is_empty() {
        let total: u32 = left.iter().map(|r| u32::from(r.weight)).sum();
        let index = if total == 0 {
            0
        } else {
            let mut pick = random(total);
            left.iter()
                .position(|r| {
                    let weight = u32::from(r.weight);
                    if pick < weight {
                        true
                    } else {
                        pick -= weight;
                        false
                    }
                })
                .unwrap_or(0)
        };
        out.push(left.remove(index));
    }
    out
}

/// A random number below `n`. The standard library seeds its hasher from the system, which
/// is enough to spread the load over equal SRV records.
fn random(n: u32) -> u32 {
    let value = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    (value % u64::from(n.max(1))) as u32
}

/// Connect to one target and open the stream.
///
/// TOKIO-XMPP-COPY: the STARTTLS branch is `StartTlsServerConnector::connect` and the direct
/// TLS branch is `DirectTlsServerConnector::connect` from tokio-xmpp 6.0.0
/// (src/connect/starttls.rs:41-85, src/connect/direct_tls.rs:38-66). The copy takes the TCP
/// connection of one target, which the originals cannot, and it sets the ALPN protocol for
/// direct TLS (a CHORD PATCH in tls_common.rs).
async fn connect_target(
    target: &Target,
    jid: &Jid,
    ns: &'static str,
    timeouts: Timeouts,
) -> Result<(PendingFeaturesRecv<TlsTcp>, ChannelBinding), Error> {
    let domain = jid.domain().as_str();
    let tcp = DnsConfig::no_srv(&target.host, target.port)
        .resolve()
        .await?;
    if target.direct_tls {
        let (tls, binding) =
            establish_tls_connection_with_alpn(tcp, domain, &[ALPN_XMPP_CLIENT]).await?;
        let header = StreamHeader {
            to: Some(domain.into()),
            // The server needs `from` to offer SASL 2 (XEP-0388).
            from: Some(jid.to_bare().as_str().to_owned().into()),
            id: None,
        };
        let stream = initiate_stream(BufStream::new(tls), ns, header, timeouts).await?;
        return Ok((stream, binding));
    }

    let header = || StreamHeader {
        to: Some(domain.into()),
        from: None,
        id: None,
    };
    let plain = initiate_stream(BufStream::new(tcp), ns, header(), timeouts).await?;
    let (features, plain) = plain.recv_features().await?;
    if !features.can_starttls() {
        return Err(Error::Protocol(ProtocolError::NoTls));
    }
    let (tls, binding) = starttls(plain, domain).await?;
    let stream = initiate_stream(BufStream::new(tls), ns, header(), timeouts).await?;
    Ok((stream, binding))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srv(priority: u16, weight: u16, host: &str) -> SrvRecord {
        SrvRecord {
            priority,
            weight,
            port: 5222,
            target: host.to_owned(),
        }
    }

    fn hosts(targets: &[Target]) -> Vec<(&str, bool)> {
        targets
            .iter()
            .map(|t| (t.host.as_str(), t.direct_tls))
            .collect()
    }

    #[test]
    fn direct_tls_comes_first_on_the_same_priority() {
        let direct = vec![srv(0, 5, "a.example")];
        let plain = vec![srv(0, 5, "a.example")];
        let out = order_targets(direct, plain, &mut |_| 0);
        assert_eq!(hosts(&out), [("a.example", true), ("a.example", false)]);
    }

    #[test]
    fn the_two_sets_merge_by_priority() {
        let direct = vec![srv(20, 1, "tls.example"), srv(5, 1, "tls-first.example")];
        let plain = vec![srv(10, 1, "plain.example")];
        let out = order_targets(direct, plain, &mut |_| 0);
        assert_eq!(
            hosts(&out),
            [
                ("tls-first.example", true),
                ("plain.example", false),
                ("tls.example", true)
            ]
        );
    }

    #[test]
    fn weights_pick_in_proportion() {
        let direct = vec![srv(0, 1, "light.example"), srv(0, 3, "heavy.example")];
        // The total is 4. A draw of 0 hits the first record, 1 to 3 hit the second.
        let first = |draw| order_targets(direct.clone(), vec![], &mut move |_| draw);
        assert_eq!(first(0)[0].host, "light.example");
        for draw in 1..4 {
            assert_eq!(first(draw)[0].host, "heavy.example");
        }
        // Every record stays in the list.
        assert_eq!(first(2).len(), 2);
    }

    #[test]
    fn a_zero_weight_comes_last_in_its_group() {
        let plain = vec![srv(0, 0, "spare.example"), srv(0, 2, "main.example")];
        let out = order_targets(vec![], plain, &mut |_| 0);
        assert_eq!(
            hosts(&out),
            [("main.example", false), ("spare.example", false)]
        );
    }

    #[test]
    fn the_dot_target_means_no_service() {
        let direct = vec![srv(0, 1, "")];
        let plain = vec![srv(0, 1, "a.example")];
        let out = order_targets(direct, plain, &mut |_| 0);
        assert_eq!(hosts(&out), [("a.example", false)]);
    }

    #[test]
    fn no_records_give_no_targets() {
        assert!(order_targets(vec![], vec![], &mut |_| 0).is_empty());
    }

    #[tokio::test]
    async fn explicit_modes_give_one_target() {
        let direct = targets(
            &Mode::DirectTls {
                host: "h.example".into(),
                port: 5223,
            },
            "example.org",
        )
        .await;
        assert_eq!(
            direct,
            [Target {
                host: "h.example".into(),
                port: 5223,
                direct_tls: true
            }]
        );
        let ip = targets(&Mode::Srv, "192.0.2.1").await;
        assert_eq!(hosts(&ip), [("192.0.2.1", false)]);
    }

    /// Live check, run by hand: a dead direct TLS target, then a real STARTTLS target.
    /// `cargo test -p chord-core --lib live_fallback -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "needs the network"]
    async fn live_fallback_from_direct_tls_to_starttls() {
        let jid = Jid::new("nobody@chat.foid.space").unwrap();
        let target = |host: &str, port, direct_tls| Target {
            host: host.into(),
            port,
            direct_tls,
        };
        let targets = vec![
            target("127.0.0.1", 1, true),
            target("chat.foid.space", 5222, false),
        ];
        let (stream, binding) = connect_any(targets, &jid, "jabber:client", Timeouts::default())
            .await
            .unwrap();
        let (features, _) = stream
            .recv_features::<tokio_xmpp::xmlstream::FallibleStreamElement>()
            .await
            .unwrap();
        assert!(!features.sasl_mechanisms.is_empty());
        println!("fell back to STARTTLS, binding {binding:?}");
    }

    #[test]
    fn random_stays_below_the_limit() {
        for n in 1..20 {
            assert!(random(n) < n);
        }
        assert_eq!(random(0), 0);
    }
}
