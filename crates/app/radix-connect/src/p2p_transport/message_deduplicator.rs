use crate::prelude::*;
use std::sync::Mutex;

/// When and over which transport a message first arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct P2pMessageSighting {
    pub transport: P2pTransport,
    /// The host's monotonic clock, in milliseconds.
    pub at_ms: u64,
}

impl HasSampleValues for P2pMessageSighting {
    fn sample() -> Self {
        Self {
            transport: P2pTransport::Iroh,
            at_ms: 1_000,
        }
    }

    fn sample_other() -> Self {
        Self {
            transport: P2pTransport::WebRtc,
            at_ms: 2_000,
        }
    }
}

/// Spots a message of a link that arrives again, over the same transport or the other one.
///
/// A connector extension sends a message again when it did not get its confirmation in time
/// (10 s on both transports), possibly over the other transport, although the wallet may have
/// received it: the same interaction then arrives twice. Only messages with an `interactionId`
/// are considered (see [`radix_connect_message_interaction_id`]), and only identical contents
/// within the window count as the same message, so that a dApp asking again later is not
/// affected.
#[derive(Debug)]
pub struct RadixConnectMessageDeduplicator {
    window_ms: u64,
    capacity: usize,
    recent: Mutex<HashMap<String, Vec<(Hash, P2pMessageSighting)>>>,
}

/// How long an incoming message is remembered.
pub const RADIX_CONNECT_DEDUPLICATION_WINDOW_MS: u64 = 120_000;

/// How many incoming messages are remembered per link.
pub const RADIX_CONNECT_DEDUPLICATION_CAPACITY: usize = 32;

impl Default for RadixConnectMessageDeduplicator {
    fn default() -> Self {
        Self::new(
            RADIX_CONNECT_DEDUPLICATION_WINDOW_MS,
            RADIX_CONNECT_DEDUPLICATION_CAPACITY,
        )
    }
}

impl RadixConnectMessageDeduplicator {
    pub fn new(window_ms: u64, capacity: usize) -> Self {
        Self {
            window_ms,
            capacity: capacity.max(1),
            recent: Mutex::new(HashMap::new()),
        }
    }

    /// The earlier arrival of the same `message` on `link_id` within the window, if any;
    /// otherwise this one is remembered (when it is an interaction) and `None` is returned.
    ///
    /// `now_ms` is the host's monotonic clock in milliseconds.
    pub fn previous_sighting(
        &self,
        link_id: &str,
        message: &str,
        transport: P2pTransport,
        now_ms: u64,
    ) -> Option<P2pMessageSighting> {
        radix_connect_message_interaction_id(message)?;
        let digest = hash_of(message.as_bytes());
        let mut recent = self.recent.lock().expect("not poisoned");
        let entries = recent.entry(link_id.to_owned()).or_default();
        entries.retain(|(_, sighting)| {
            now_ms.saturating_sub(sighting.at_ms) < self.window_ms
        });
        if let Some((_, previous)) = entries.iter().find(|(d, _)| *d == digest)
        {
            return Some(*previous);
        }
        entries.push((
            digest,
            P2pMessageSighting {
                transport,
                at_ms: now_ms,
            },
        ));
        if entries.len() > self.capacity {
            let excess = entries.len() - self.capacity;
            entries.drain(..excess);
        }
        None
    }

    /// Forgets `link_id`, once the link is deleted.
    pub fn forget(&self, link_id: &str) {
        self.recent.lock().expect("not poisoned").remove(link_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use P2pTransport::*;

    const LINK: &str = "link";
    const MESSAGE: &str = r#"{"interactionId":"a","items":{}}"#;

    #[test]
    fn a_message_arriving_again_is_spotted_over_any_transport() {
        let sut = RadixConnectMessageDeduplicator::default();
        assert_eq!(sut.previous_sighting(LINK, MESSAGE, Iroh, 1_000), None);
        assert_eq!(
            sut.previous_sighting(LINK, MESSAGE, WebRtc, 5_000),
            Some(P2pMessageSighting {
                transport: Iroh,
                at_ms: 1_000
            })
        );
    }

    #[test]
    fn only_within_the_window() {
        let sut = RadixConnectMessageDeduplicator::new(10_000, 32);
        assert_eq!(sut.previous_sighting(LINK, MESSAGE, Iroh, 0), None);
        assert_eq!(sut.previous_sighting(LINK, MESSAGE, Iroh, 10_000), None);
        assert!(sut.previous_sighting(LINK, MESSAGE, Iroh, 19_999).is_some());
    }

    #[test]
    fn different_contents_links_and_non_interactions_are_not_duplicates() {
        let sut = RadixConnectMessageDeduplicator::default();
        sut.previous_sighting(LINK, MESSAGE, Iroh, 0);
        assert_eq!(
            sut.previous_sighting(LINK, r#"{"interactionId":"b"}"#, Iroh, 1),
            None
        );
        assert_eq!(sut.previous_sighting("other", MESSAGE, Iroh, 1), None);

        let not_an_interaction = r#"{"discriminator":"accountList"}"#;
        sut.previous_sighting(LINK, not_an_interaction, Iroh, 2);
        assert_eq!(
            sut.previous_sighting(LINK, not_an_interaction, Iroh, 3),
            None
        );
    }

    #[test]
    fn capacity_and_forget() {
        let sut = RadixConnectMessageDeduplicator::new(100_000, 2);
        let message = |id: &str| format!(r#"{{"interactionId":"{id}"}}"#);
        sut.previous_sighting(LINK, &message("a"), Iroh, 0);
        sut.previous_sighting(LINK, &message("b"), Iroh, 1);
        sut.previous_sighting(LINK, &message("c"), Iroh, 2);
        // "a" was pushed out.
        assert_eq!(sut.previous_sighting(LINK, &message("a"), Iroh, 3), None);

        sut.forget(LINK);
        assert_eq!(sut.previous_sighting(LINK, &message("c"), Iroh, 4), None);
    }
}
