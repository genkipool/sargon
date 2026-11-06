use crate::prelude::*;
use sargon::P2pLinkStatus as InternalP2pLinkStatus;
use sargon::P2pLinkingLoserAction as InternalP2pLinkingLoserAction;
use sargon::P2pLinkingRace as InternalP2pLinkingRace;
use sargon::P2pLinkingStep as InternalP2pLinkingStep;
use sargon::P2pMessageSighting as InternalP2pMessageSighting;
use sargon::P2pTransport as InternalP2pTransport;
use sargon::P2pTransportMode as InternalP2pTransportMode;
use sargon::P2pTransportPolicy as InternalP2pTransportPolicy;
use sargon::RadixConnectHeldRequests as InternalRadixConnectHeldRequests;
use sargon::RadixConnectMessageDeduplicator as InternalRadixConnectMessageDeduplicator;

/// The transports a wallet can reach a connector extension with, in order of preference.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, InternalConversion, uniffi::Enum,
)]
pub enum P2pTransport {
    Iroh,
    WebRtc,
}

/// Which transports the user lets Radix Connect use.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, InternalConversion, uniffi::Enum,
)]
pub enum P2pTransportMode {
    Automatic,
    WebRtc,
    Iroh,
}

/// Decides which transports each link uses.
#[derive(Clone, Debug, PartialEq, Eq, InternalConversion, uniffi::Record)]
pub struct P2pTransportPolicy {
    pub mode: P2pTransportMode,
    pub iroh_capable_link_ids: Vec<String>,
}

/// The connection status of a link over one transport, or over all of them.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, InternalConversion, uniffi::Enum,
)]
pub enum P2pLinkStatus {
    Open,
    Connecting,
    Closed,
}

/// When and over which transport a message first arrived.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, InternalConversion, uniffi::Record,
)]
pub struct P2pMessageSighting {
    pub transport: P2pTransport,
    pub at_ms: u64,
}

/// What a transport that connected during a linking does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum P2pLinkingStep {
    Won,
    Lost { winner: P2pTransport },
    WaitForPreferred { deadline_ms: u64 },
}

impl From<InternalP2pLinkingStep> for P2pLinkingStep {
    fn from(value: InternalP2pLinkingStep) -> Self {
        match value {
            InternalP2pLinkingStep::Won => Self::Won,
            InternalP2pLinkingStep::Lost { winner } => Self::Lost {
                winner: winner.into(),
            },
            InternalP2pLinkingStep::WaitForPreferred { deadline_ms } => {
                Self::WaitForPreferred { deadline_ms }
            }
        }
    }
}

/// What happens to the attempt of a transport that lost the linking while still trying.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, InternalConversion, uniffi::Enum,
)]
pub enum P2pLinkingLoserAction {
    Cancel,
    GoOnToLearnSupport,
}

fn internal_transports(
    transports: Vec<P2pTransport>,
) -> Vec<InternalP2pTransport> {
    transports.into_iter().map(Into::into).collect()
}

fn transports(transports: Vec<InternalP2pTransport>) -> Vec<P2pTransport> {
    transports.into_iter().map(Into::into).collect()
}

// === Transport policy ===

/// The transports that connect to the connector extension of `link_id`, in order of preference.
#[uniffi::export]
pub fn p2p_transport_policy_transports_for_link(
    policy: &P2pTransportPolicy,
    link_id: String,
) -> Vec<P2pTransport> {
    transports(policy.into_internal().transports_for_link(&link_id))
}

/// The transports that try to link a new connector extension, in order of preference.
#[uniffi::export]
pub fn p2p_transport_policy_linking_transports(
    policy: &P2pTransportPolicy,
) -> Vec<P2pTransport> {
    transports(policy.into_internal().linking_transports())
}

/// Whether `transport` may connect any link at all.
#[uniffi::export]
pub fn p2p_transport_policy_may_use(
    policy: &P2pTransportPolicy,
    transport: P2pTransport,
) -> bool {
    policy.into_internal().may_use(transport.into())
}

/// The status of a link from the statuses of its transports.
#[uniffi::export]
pub fn p2p_link_status_merge(statuses: Vec<P2pLinkStatus>) -> P2pLinkStatus {
    sargon::p2p_link_status_merge(statuses.into_iter().map(Into::into)).into()
}

/// The order in which a message is tried over the connected transports of a link.
#[uniffi::export]
pub fn p2p_transports_send_order(
    connected: Vec<P2pTransport>,
) -> Vec<P2pTransport> {
    transports(sargon::p2p_transports_send_order(internal_transports(
        connected,
    )))
}

// === Messages ===

/// The top-level `interactionId` of a message exchanged with a connector extension, when it has
/// one that looks like an id.
#[uniffi::export]
pub fn radix_connect_message_interaction_id(message: String) -> Option<String> {
    sargon::radix_connect_message_interaction_id(&message)
}

/// The `walletRequests` message: the interactions of a connector the wallet still holds.
#[uniffi::export]
pub fn radix_connect_wallet_requests_message(
    interaction_ids: Vec<String>,
) -> String {
    sargon::radix_connect_wallet_requests_message(interaction_ids)
}

/// The dApp requests a wallet took from each link and has not answered yet.
#[derive(uniffi::Object)]
pub struct RadixConnectHeldRequests {
    wrapped: InternalRadixConnectHeldRequests,
}

#[uniffi::export]
impl RadixConnectHeldRequests {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            wrapped: InternalRadixConnectHeldRequests::new(),
        })
    }

    /// The wallet took `message` from the connector of `link_id`.
    pub fn took(&self, link_id: String, message: String) {
        self.wrapped.took(&link_id, &message)
    }

    /// The wallet sent `message` to the connector of `link_id`, and it was delivered.
    pub fn sent(&self, link_id: String, message: String) {
        self.wrapped.sent(&link_id, &message)
    }

    /// The interaction ids the wallet still holds for `link_id`, oldest first.
    pub fn held(&self, link_id: String) -> Vec<String> {
        self.wrapped.held(&link_id)
    }

    /// The `walletRequests` message for the connector of `link_id`.
    pub fn inventory_message(&self, link_id: String) -> String {
        self.wrapped.inventory_message(&link_id)
    }

    /// Forgets `link_id`, once the link is deleted.
    pub fn forget(&self, link_id: String) {
        self.wrapped.forget(&link_id)
    }
}

/// Spots a message of a link that arrives again, over the same transport or the other one.
#[derive(uniffi::Object)]
pub struct RadixConnectMessageDeduplicator {
    wrapped: InternalRadixConnectMessageDeduplicator,
}

#[uniffi::export]
impl RadixConnectMessageDeduplicator {
    /// With the default window (120 s) and capacity (32 messages per link).
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            wrapped: InternalRadixConnectMessageDeduplicator::default(),
        })
    }

    #[uniffi::constructor]
    pub fn with_window(window_ms: u64, capacity: u32) -> Arc<Self> {
        Arc::new(Self {
            wrapped: InternalRadixConnectMessageDeduplicator::new(
                window_ms,
                capacity as usize,
            ),
        })
    }

    /// The earlier arrival of the same `message` on `link_id` within the window, if any;
    /// otherwise this one is remembered (when it is an interaction). `now_ms` is a monotonic
    /// clock in milliseconds.
    pub fn previous_sighting(
        &self,
        link_id: String,
        message: String,
        transport: P2pTransport,
        now_ms: u64,
    ) -> Option<P2pMessageSighting> {
        self.wrapped
            .previous_sighting(&link_id, &message, transport.into(), now_ms)
            .map(Into::into)
    }

    /// Forgets `link_id`, once the link is deleted.
    pub fn forget(&self, link_id: String) {
        self.wrapped.forget(&link_id)
    }
}

// === Linking ===

/// How long the linking waits for a preferred transport when a less preferred one connected
/// first, in milliseconds.
#[uniffi::export]
pub fn p2p_linking_preferred_transport_grace_ms() -> u64 {
    sargon::P2P_LINKING_PREFERRED_TRANSPORT_GRACE_MS
}

/// What happens to the attempt of `loser`, still trying when `winner` won the linking.
#[uniffi::export]
pub fn p2p_linking_loser_action(
    loser: P2pTransport,
    winner: P2pTransport,
) -> P2pLinkingLoserAction {
    sargon::p2p_linking_loser_action(loser.into(), winner.into()).into()
}

/// Whether the attempt of `transport` may go on after a linking over `transports`.
#[uniffi::export]
pub fn p2p_linking_may_go_on_after_linking(
    transport: P2pTransport,
    transports: Vec<P2pTransport>,
) -> bool {
    sargon::p2p_linking_may_go_on_after_linking(
        transport.into(),
        &internal_transports(transports),
    )
}

/// One linking of a connector extension over several transports at once: decides, exactly once,
/// which transport completes it.
#[derive(uniffi::Object)]
pub struct P2pLinkingRace {
    wrapped: InternalP2pLinkingRace,
}

#[uniffi::export]
impl P2pLinkingRace {
    /// A race between `transports`, which all start trying now; `grace_ms` is how long a less
    /// preferred transport that connected first waits for the preferred ones.
    #[uniffi::constructor]
    pub fn new(transports: Vec<P2pTransport>, grace_ms: u64) -> Arc<Self> {
        Arc::new(Self {
            wrapped: InternalP2pLinkingRace::new(
                internal_transports(transports),
                grace_ms,
            ),
        })
    }

    /// The transport that completes the linking, once decided.
    pub fn winner(&self) -> Option<P2pTransport> {
        self.wrapped.winner().map(Into::into)
    }

    /// `transport` connected to the connector extension.
    pub fn connected(
        &self,
        transport: P2pTransport,
        now_ms: u64,
    ) -> P2pLinkingStep {
        self.wrapped.connected(transport.into(), now_ms).into()
    }

    /// The attempt of `transport` ended, whatever its result. Returns the winner when this
    /// decides the linking.
    pub fn attempt_ended(
        &self,
        transport: P2pTransport,
    ) -> Option<P2pTransport> {
        self.wrapped.attempt_ended(transport.into()).map(Into::into)
    }

    /// The deadline of a `WaitForPreferred` passed. Returns the winner, when decided.
    pub fn grace_over(&self, now_ms: u64) -> Option<P2pTransport> {
        self.wrapped.grace_over(now_ms).map(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_round_trip() {
        let policy = P2pTransportPolicy {
            mode: P2pTransportMode::Automatic,
            iroh_capable_link_ids: vec!["aa".to_owned()],
        };
        assert_eq!(
            p2p_transport_policy_transports_for_link(&policy, "aa".to_owned()),
            vec![P2pTransport::Iroh, P2pTransport::WebRtc]
        );
        assert!(p2p_transport_policy_may_use(&policy, P2pTransport::Iroh));
    }

    #[test]
    fn race_through_the_bindings() {
        let race = P2pLinkingRace::new(
            vec![P2pTransport::Iroh, P2pTransport::WebRtc],
            p2p_linking_preferred_transport_grace_ms(),
        );
        assert!(matches!(
            race.connected(P2pTransport::WebRtc, 0),
            P2pLinkingStep::WaitForPreferred { deadline_ms: 5_000 }
        ));
        assert_eq!(
            race.attempt_ended(P2pTransport::Iroh),
            Some(P2pTransport::WebRtc)
        );
    }

    #[test]
    fn held_requests_and_deduplicator_through_the_bindings() {
        let held = RadixConnectHeldRequests::new();
        held.took(
            "l".to_owned(),
            r#"{"interactionId":"a","items":{}}"#.to_owned(),
        );
        assert_eq!(held.held("l".to_owned()), vec!["a".to_owned()]);

        let dedup = RadixConnectMessageDeduplicator::new();
        let message = r#"{"interactionId":"a"}"#.to_owned();
        assert_eq!(
            dedup.previous_sighting(
                "l".to_owned(),
                message.clone(),
                P2pTransport::Iroh,
                1
            ),
            None
        );
        assert_eq!(
            dedup.previous_sighting(
                "l".to_owned(),
                message,
                P2pTransport::WebRtc,
                2
            ),
            Some(P2pMessageSighting {
                transport: P2pTransport::Iroh,
                at_ms: 1
            })
        );
    }
}
