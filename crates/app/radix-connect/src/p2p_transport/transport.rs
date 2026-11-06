use crate::prelude::*;

/// The transports a wallet can reach a connector extension with. The declaration order is the
/// order of preference: when several are connected a message goes over the first one, and the
/// linking prefers it (see [`P2pLinkingRace`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum P2pTransport {
    /// QUIC through an iroh relay.
    Iroh,
    /// WebRTC data channels negotiated through the signaling server.
    WebRtc,
}

/// Which transports the user lets Radix Connect use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum P2pTransportMode {
    /// WebRTC for every link, plus iroh for the links known to support it. Linking tries both
    /// and uses whichever connects first (iroh preferred).
    #[default]
    Automatic,
    /// WebRTC only.
    WebRtc,
    /// iroh only: connectors without iroh do not connect.
    Iroh,
}

/// Decides which transports each link uses. Every per-transport rule lives here, so that the
/// hosts only ask the policy.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct P2pTransportPolicy {
    pub mode: P2pTransportMode,
    /// The ids of the links (the hash of their password, as hex) known to support iroh: their
    /// iroh session connected at least once.
    pub iroh_capable_link_ids: Vec<String>,
}

impl P2pTransportPolicy {
    pub fn new(
        mode: P2pTransportMode,
        iroh_capable_link_ids: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            mode,
            iroh_capable_link_ids: iroh_capable_link_ids.into_iter().collect(),
        }
    }

    fn is_iroh_capable(&self, link_id: &str) -> bool {
        self.iroh_capable_link_ids.iter().any(|id| id == link_id)
    }

    /// The transports that connect to the connector extension of `link_id`, in order of
    /// preference.
    pub fn transports_for_link(&self, link_id: &str) -> Vec<P2pTransport> {
        match self.mode {
            // WebRTC keeps every existing connector working; iroh only where it is known to work.
            P2pTransportMode::Automatic if self.is_iroh_capable(link_id) => {
                vec![P2pTransport::Iroh, P2pTransport::WebRtc]
            }
            P2pTransportMode::Automatic | P2pTransportMode::WebRtc => {
                vec![P2pTransport::WebRtc]
            }
            P2pTransportMode::Iroh => vec![P2pTransport::Iroh],
        }
    }

    /// The transports that try to link a new connector extension (nothing is known about it
    /// yet), in order of preference: see [`P2pLinkingRace`].
    pub fn linking_transports(&self) -> Vec<P2pTransport> {
        match self.mode {
            P2pTransportMode::Automatic => {
                vec![P2pTransport::Iroh, P2pTransport::WebRtc]
            }
            P2pTransportMode::WebRtc => vec![P2pTransport::WebRtc],
            P2pTransportMode::Iroh => vec![P2pTransport::Iroh],
        }
    }

    /// Whether `transport` may connect any link at all (whether it needs to run).
    pub fn may_use(&self, transport: P2pTransport) -> bool {
        match self.mode {
            P2pTransportMode::Automatic => {
                transport == P2pTransport::WebRtc
                    || !self.iroh_capable_link_ids.is_empty()
            }
            P2pTransportMode::WebRtc => transport == P2pTransport::WebRtc,
            P2pTransportMode::Iroh => transport == P2pTransport::Iroh,
        }
    }
}

impl HasSampleValues for P2pTransportPolicy {
    fn sample() -> Self {
        Self::new(P2pTransportMode::Automatic, ["aa".to_owned()])
    }

    fn sample_other() -> Self {
        Self::new(P2pTransportMode::WebRtc, Vec::new())
    }
}

/// The connection status of a link over one transport, or over all of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum P2pLinkStatus {
    Open,
    Connecting,
    Closed,
}

/// The status of a link from the statuses of its transports: open if any is open, else
/// connecting if any is connecting, else closed (also when there is none).
pub fn p2p_link_status_merge(
    statuses: impl IntoIterator<Item = P2pLinkStatus>,
) -> P2pLinkStatus {
    statuses
        .into_iter()
        .fold(P2pLinkStatus::Closed, |merged, status| {
            match (merged, status) {
                (P2pLinkStatus::Open, _) | (_, P2pLinkStatus::Open) => {
                    P2pLinkStatus::Open
                }
                (P2pLinkStatus::Connecting, _)
                | (_, P2pLinkStatus::Connecting) => P2pLinkStatus::Connecting,
                _ => P2pLinkStatus::Closed,
            }
        })
}

/// The order in which a message is tried over the connected transports of a link: their order
/// of preference. The next one may be tried only when the previous one did not send the message
/// at all, never when it may have been delivered.
pub fn p2p_transports_send_order(
    connected: impl IntoIterator<Item = P2pTransport>,
) -> Vec<P2pTransport> {
    let mut order: Vec<_> = connected.into_iter().collect();
    order.sort();
    order.dedup();
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use P2pLinkStatus::*;
    use P2pTransport::*;

    const CAPABLE: &str = "aa";
    const OTHER: &str = "bb";

    fn policy(mode: P2pTransportMode) -> P2pTransportPolicy {
        P2pTransportPolicy::new(mode, [CAPABLE.to_owned()])
    }

    #[test]
    fn automatic_uses_iroh_only_for_capable_links() {
        let policy = policy(P2pTransportMode::Automatic);
        assert_eq!(policy.transports_for_link(CAPABLE), vec![Iroh, WebRtc]);
        assert_eq!(policy.transports_for_link(OTHER), vec![WebRtc]);
        assert_eq!(policy.linking_transports(), vec![Iroh, WebRtc]);
        assert!(policy.may_use(Iroh));
        assert!(policy.may_use(WebRtc));
    }

    #[test]
    fn automatic_without_capable_links_does_not_need_iroh() {
        let policy =
            P2pTransportPolicy::new(P2pTransportMode::Automatic, Vec::new());
        assert!(!policy.may_use(Iroh));
        assert!(policy.may_use(WebRtc));
        assert_eq!(policy.linking_transports(), vec![Iroh, WebRtc]);
    }

    #[test]
    fn single_transport_modes() {
        let webrtc = policy(P2pTransportMode::WebRtc);
        assert_eq!(webrtc.transports_for_link(CAPABLE), vec![WebRtc]);
        assert_eq!(webrtc.linking_transports(), vec![WebRtc]);
        assert!(!webrtc.may_use(Iroh));

        let iroh = policy(P2pTransportMode::Iroh);
        assert_eq!(iroh.transports_for_link(OTHER), vec![Iroh]);
        assert_eq!(iroh.linking_transports(), vec![Iroh]);
        assert!(!iroh.may_use(WebRtc));
    }

    #[test]
    fn merge_statuses() {
        assert_eq!(p2p_link_status_merge([Closed, Open, Connecting]), Open);
        assert_eq!(p2p_link_status_merge([Closed, Connecting]), Connecting);
        assert_eq!(p2p_link_status_merge([Closed, Closed]), Closed);
        assert_eq!(p2p_link_status_merge([]), Closed);
    }

    #[test]
    fn send_order_follows_preference() {
        assert_eq!(
            p2p_transports_send_order([WebRtc, Iroh]),
            vec![Iroh, WebRtc]
        );
        assert_eq!(p2p_transports_send_order([WebRtc, WebRtc]), vec![WebRtc]);
        assert!(p2p_transports_send_order([]).is_empty());
    }
}
