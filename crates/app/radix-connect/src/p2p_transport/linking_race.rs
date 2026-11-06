use crate::prelude::*;
use std::collections::BTreeSet;
use std::sync::Mutex;

/// How long the linking waits for a preferred transport (iroh) when a less preferred one
/// (WebRTC) connected first. iroh usually connects within a couple of seconds of WebRTC, and a
/// link won by iroh is remembered as supporting it.
pub const P2P_LINKING_PREFERRED_TRANSPORT_GRACE_MS: u64 = 5_000;

/// What a transport that connected to the connector extension during a linking does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P2pLinkingStep {
    /// It completes the linking.
    Won,
    /// Another transport completes the linking: this one closes its connection.
    Lost { winner: P2pTransport },
    /// A preferred transport is still trying: wait for the linking to be decided, at most until
    /// `deadline_ms` (the host's monotonic clock), then call
    /// [`P2pLinkingRace::grace_over`].
    WaitForPreferred { deadline_ms: u64 },
}

/// What happens to the attempt of a transport that lost the linking while still trying.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum P2pLinkingLoserAction {
    /// It is cancelled and closed.
    Cancel,
    /// It is preferred to the winner: it goes on after the linking, within its own time limit,
    /// only to find out whether the connector extension supports it, and is closed once it
    /// connects. The linking does not wait for it. Otherwise a link whose iroh lost the race
    /// would never use iroh in the automatic mode.
    GoOnToLearnSupport,
}

#[derive(Debug)]
struct RaceState {
    trying: BTreeSet<P2pTransport>,
    winner: Option<P2pTransport>,
    // The transport waiting for preferred ones, and until when.
    waiting: Option<(P2pTransport, u64)>,
}

/// One linking of a connector extension over several transports at once, which decides which
/// transport completes it, following their order of preference ([`P2pTransport`]):
/// - The most preferred transport connecting first wins at once.
/// - A less preferred one connecting first while a preferred one is still trying waits up to
///   the grace period for it: the preferred one wins if it connects meanwhile; the waiting one
///   wins once the grace period is over, or as soon as the preferred attempts end.
/// - A transport connecting when no preferred one is trying any more wins.
///
/// The linking is decided exactly once. The host runs the attempts and the timers, and reports
/// to the race what happens; all times are the host's monotonic clock, in milliseconds.
#[derive(Debug)]
pub struct P2pLinkingRace {
    grace_ms: u64,
    state: Mutex<RaceState>,
}

impl P2pLinkingRace {
    /// A race between `transports`, which all start trying now.
    pub fn new(
        transports: impl IntoIterator<Item = P2pTransport>,
        grace_ms: u64,
    ) -> Self {
        Self {
            grace_ms,
            state: Mutex::new(RaceState {
                trying: transports.into_iter().collect(),
                winner: None,
                waiting: None,
            }),
        }
    }

    pub fn with_default_grace(
        transports: impl IntoIterator<Item = P2pTransport>,
    ) -> Self {
        Self::new(transports, P2P_LINKING_PREFERRED_TRANSPORT_GRACE_MS)
    }

    /// The transport that completes the linking, once decided.
    pub fn winner(&self) -> Option<P2pTransport> {
        self.state.lock().expect("not poisoned").winner
    }

    /// `transport` connected to the connector extension.
    pub fn connected(
        &self,
        transport: P2pTransport,
        now_ms: u64,
    ) -> P2pLinkingStep {
        let mut state = self.state.lock().expect("not poisoned");
        if let Some(winner) = state.winner {
            return Self::step_for(transport, winner);
        }
        if state.trying.iter().any(|t| *t < transport) {
            let deadline_ms = match state.waiting {
                Some((waiting, deadline_ms)) if waiting <= transport => {
                    deadline_ms
                }
                _ => {
                    let deadline_ms = now_ms.saturating_add(self.grace_ms);
                    state.waiting = Some((transport, deadline_ms));
                    deadline_ms
                }
            };
            return P2pLinkingStep::WaitForPreferred { deadline_ms };
        }
        state.winner = Some(transport);
        P2pLinkingStep::Won
    }

    /// The attempt of `transport` ended, whatever its result. Returns the winner when this
    /// decides the linking (a transport waiting for it wins).
    pub fn attempt_ended(
        &self,
        transport: P2pTransport,
    ) -> Option<P2pTransport> {
        let mut state = self.state.lock().expect("not poisoned");
        state.trying.remove(&transport);
        if state.winner.is_some() {
            return None;
        }
        let (waiting, _) = state.waiting?;
        if state.trying.iter().any(|t| *t < waiting) {
            return None;
        }
        state.winner = Some(waiting);
        state.winner
    }

    /// The deadline of a [`P2pLinkingStep::WaitForPreferred`] passed. Returns the winner (the
    /// waiting transport, unless a preferred one won meanwhile).
    pub fn grace_over(&self, now_ms: u64) -> Option<P2pTransport> {
        let mut state = self.state.lock().expect("not poisoned");
        if state.winner.is_some() {
            return state.winner;
        }
        let (waiting, deadline_ms) = state.waiting?;
        if now_ms < deadline_ms {
            return None;
        }
        state.winner = Some(waiting);
        state.winner
    }

    /// What `transport` does now that `winner` completes the linking.
    pub fn step_for(
        transport: P2pTransport,
        winner: P2pTransport,
    ) -> P2pLinkingStep {
        if transport == winner {
            P2pLinkingStep::Won
        } else {
            P2pLinkingStep::Lost { winner }
        }
    }
}

/// What happens to the attempt of `loser`, still trying when `winner` won the linking.
pub fn p2p_linking_loser_action(
    loser: P2pTransport,
    winner: P2pTransport,
) -> P2pLinkingLoserAction {
    if loser < winner {
        P2pLinkingLoserAction::GoOnToLearnSupport
    } else {
        P2pLinkingLoserAction::Cancel
    }
}

/// Whether the attempt of `transport` may go on after a linking over `transports` (it may lose
/// to a less preferred one, see [`P2pLinkingLoserAction::GoOnToLearnSupport`]): the host must
/// then run it outside of the linking, so that the end of the linking does not cancel it.
pub fn p2p_linking_may_go_on_after_linking(
    transport: P2pTransport,
    transports: &[P2pTransport],
) -> bool {
    transports.iter().any(|t| *t > transport)
}

#[cfg(test)]
mod tests {
    use super::*;
    use P2pLinkingStep::*;
    use P2pTransport::*;

    fn race() -> P2pLinkingRace {
        P2pLinkingRace::new([WebRtc, Iroh], 5_000)
    }

    #[test]
    fn the_preferred_transport_connecting_first_wins_at_once() {
        let sut = race();
        assert_eq!(sut.connected(Iroh, 100), Won);
        assert_eq!(sut.connected(WebRtc, 200), Lost { winner: Iroh });
        assert_eq!(sut.winner(), Some(Iroh));
    }

    #[test]
    fn a_less_preferred_transport_waits_and_loses_if_the_preferred_one_connects(
    ) {
        let sut = race();
        assert_eq!(
            sut.connected(WebRtc, 1_000),
            WaitForPreferred { deadline_ms: 6_000 }
        );
        assert_eq!(sut.connected(Iroh, 3_000), Won);
        assert_eq!(sut.grace_over(6_000), Some(Iroh));
        assert_eq!(sut.attempt_ended(Iroh), None);
    }

    #[test]
    fn the_waiting_transport_wins_when_the_grace_period_is_over() {
        let sut = race();
        sut.connected(WebRtc, 1_000);
        assert_eq!(sut.grace_over(5_999), None);
        assert_eq!(sut.winner(), None);
        assert_eq!(sut.grace_over(6_000), Some(WebRtc));
        // iroh connecting later loses (and goes on to learn support).
        assert_eq!(sut.connected(Iroh, 7_000), Lost { winner: WebRtc });
        assert_eq!(
            p2p_linking_loser_action(Iroh, WebRtc),
            P2pLinkingLoserAction::GoOnToLearnSupport
        );
    }

    #[test]
    fn the_waiting_transport_wins_as_soon_as_the_preferred_attempt_ends() {
        let sut = race();
        sut.connected(WebRtc, 1_000);
        assert_eq!(sut.attempt_ended(Iroh), Some(WebRtc));
        assert_eq!(sut.winner(), Some(WebRtc));
    }

    #[test]
    fn a_transport_wins_at_once_when_the_preferred_attempt_had_ended() {
        let sut = race();
        assert_eq!(sut.attempt_ended(Iroh), None);
        assert_eq!(sut.connected(WebRtc, 1_000), Won);
    }

    #[test]
    fn an_ended_less_preferred_attempt_decides_nothing() {
        let sut = race();
        assert_eq!(sut.attempt_ended(WebRtc), None);
        assert_eq!(sut.grace_over(100_000), None);
        assert_eq!(sut.winner(), None);
        assert_eq!(sut.connected(Iroh, 1_000), Won);
    }

    #[test]
    fn a_single_transport_wins_at_once() {
        let sut = P2pLinkingRace::with_default_grace([WebRtc]);
        assert_eq!(sut.connected(WebRtc, 0), Won);
    }

    #[test]
    fn loser_actions_and_who_may_go_on() {
        assert_eq!(
            p2p_linking_loser_action(WebRtc, Iroh),
            P2pLinkingLoserAction::Cancel
        );
        assert!(p2p_linking_may_go_on_after_linking(Iroh, &[Iroh, WebRtc]));
        assert!(!p2p_linking_may_go_on_after_linking(
            WebRtc,
            &[Iroh, WebRtc]
        ));
        assert!(!p2p_linking_may_go_on_after_linking(Iroh, &[Iroh]));
    }
}
