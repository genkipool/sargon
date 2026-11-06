use crate::prelude::*;
use std::sync::Mutex;

/// The dApp requests a wallet took from each link and has not answered yet: what it tells the
/// connector it still has, on every data channel it opens, with a `walletRequests` message (see
/// [`radix_connect_wallet_requests_message`]).
///
/// Kept in memory only: the list is lost with the app, exactly when the requests are. A
/// connector that knows it then sends again the ones it had delivered, and never one the wallet
/// kept (it was only in the background). The iroh transport does the same by itself.
///
/// Requests are read from the messages themselves (`interactionId`, `items`, an answer's
/// `discriminator`); anything else is ignored.
#[derive(Debug, Default)]
pub struct RadixConnectHeldRequests {
    by_link: Mutex<HashMap<String, Vec<String>>>,
}

/// Requests kept per link, the newest ones: one never answered is dropped at last.
pub const RADIX_CONNECT_MAX_HELD_REQUESTS: usize = 200;

enum Tracked {
    Request(String),
    Cancel(String),
    Response(String),
}

fn classify(message: &str) -> Option<Tracked> {
    let json: serde_json::Value = serde_json::from_str(message).ok()?;
    let json = json.as_object()?;
    let id = json.get("interactionId")?.as_str()?.to_owned();
    // An answer has items too: its discriminator tells it apart.
    match json.get("discriminator") {
        None => {}
        Some(discriminator) => {
            return match discriminator.as_str() {
                Some("success") | Some("failure") => {
                    Some(Tracked::Response(id))
                }
                _ => None,
            };
        }
    }
    let items = json.get("items")?.as_object()?;
    if items.get("discriminator").and_then(|d| d.as_str())
        == Some("cancelRequest")
    {
        Some(Tracked::Cancel(id))
    } else {
        Some(Tracked::Request(id))
    }
}

impl RadixConnectHeldRequests {
    pub fn new() -> Self {
        Self::default()
    }

    fn update(&self, link_id: &str, change: impl FnOnce(&mut Vec<String>)) {
        let mut by_link = self.by_link.lock().expect("not poisoned");
        let held = by_link.entry(link_id.to_owned()).or_default();
        change(held);
        if held.is_empty() {
            by_link.remove(link_id);
        }
    }

    /// The wallet took `message` from the connector of `link_id`.
    pub fn took(&self, link_id: &str, message: &str) {
        match classify(message) {
            Some(Tracked::Request(id)) => self.update(link_id, |held| {
                if !held.contains(&id) {
                    held.push(id);
                }
                if held.len() > RADIX_CONNECT_MAX_HELD_REQUESTS {
                    let excess = held.len() - RADIX_CONNECT_MAX_HELD_REQUESTS;
                    held.drain(..excess);
                }
            }),
            Some(Tracked::Cancel(id)) => {
                self.update(link_id, |held| held.retain(|h| *h != id))
            }
            _ => {}
        }
    }

    /// The wallet sent `message` to the connector of `link_id`, and it was delivered.
    pub fn sent(&self, link_id: &str, message: &str) {
        if let Some(Tracked::Response(id)) = classify(message) {
            self.update(link_id, |held| held.retain(|h| *h != id))
        }
    }

    /// The interaction ids the wallet still holds for `link_id`, oldest first.
    pub fn held(&self, link_id: &str) -> Vec<String> {
        self.by_link
            .lock()
            .expect("not poisoned")
            .get(link_id)
            .cloned()
            .unwrap_or_default()
    }

    /// The message telling the connector of `link_id` which of its requests the wallet still
    /// has.
    pub fn inventory_message(&self, link_id: &str) -> String {
        radix_connect_wallet_requests_message(self.held(link_id))
    }

    /// Forgets `link_id`, once the link is deleted.
    pub fn forget(&self, link_id: &str) {
        self.by_link.lock().expect("not poisoned").remove(link_id);
    }
}

/// The `walletRequests` message: the interactions of a connector the wallet still holds.
pub fn radix_connect_wallet_requests_message(
    interaction_ids: impl IntoIterator<Item = String>,
) -> String {
    serde_json::json!({
        "discriminator": "walletRequests",
        "interactionIds": interaction_ids.into_iter().collect::<Vec<_>>(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "link";

    fn request(id: &str) -> String {
        format!(
            r#"{{"interactionId":"{id}","items":{{"discriminator":"authorizedRequest"}},"metadata":{{}}}}"#
        )
    }

    fn cancel(id: &str) -> String {
        format!(
            r#"{{"interactionId":"{id}","items":{{"discriminator":"cancelRequest"}}}}"#
        )
    }

    fn success(id: &str) -> String {
        format!(
            r#"{{"discriminator":"success","interactionId":"{id}","items":{{}}}}"#
        )
    }

    #[test]
    fn a_taken_request_is_held_until_answered() {
        let sut = RadixConnectHeldRequests::new();
        sut.took(LINK, &request("a"));
        sut.took(LINK, &request("b"));
        assert_eq!(sut.held(LINK), vec!["a", "b"]);
        assert_eq!(
            sut.inventory_message(LINK),
            r#"{"discriminator":"walletRequests","interactionIds":["a","b"]}"#
        );

        sut.sent(LINK, &success("a"));
        assert_eq!(sut.held(LINK), vec!["b"]);
    }

    #[test]
    fn a_cancelled_request_is_no_longer_held() {
        let sut = RadixConnectHeldRequests::new();
        sut.took(LINK, &request("a"));
        sut.took(LINK, &cancel("a"));
        assert!(sut.held(LINK).is_empty());
    }

    #[test]
    fn failure_answers_count_and_other_messages_are_ignored() {
        let sut = RadixConnectHeldRequests::new();
        sut.took(LINK, &request("a"));
        sut.took(
            LINK,
            r#"{"discriminator":"accountList","interactionId":"x"}"#,
        );
        sut.took(LINK, r#"{"interactionId":"y"}"#);
        sut.took(LINK, "not json");
        // A response taken (not sent) is not a request.
        sut.took(LINK, &success("z"));
        // A request sent is not an answer.
        sut.sent(LINK, &request("a"));
        assert_eq!(sut.held(LINK), vec!["a"]);

        sut.sent(
            LINK,
            r#"{"discriminator":"failure","interactionId":"a","error":"rejectedByUser"}"#,
        );
        assert!(sut.held(LINK).is_empty());
    }

    #[test]
    fn a_request_taken_twice_is_held_once() {
        let sut = RadixConnectHeldRequests::new();
        sut.took(LINK, &request("a"));
        sut.took(LINK, &request("b"));
        sut.took(LINK, &request("a"));
        assert_eq!(sut.held(LINK), vec!["a", "b"]);
    }

    #[test]
    fn the_oldest_requests_are_dropped_beyond_the_limit() {
        let sut = RadixConnectHeldRequests::new();
        for i in 0..=RADIX_CONNECT_MAX_HELD_REQUESTS {
            sut.took(LINK, &request(&i.to_string()));
        }
        let held = sut.held(LINK);
        assert_eq!(held.len(), RADIX_CONNECT_MAX_HELD_REQUESTS);
        assert_eq!(held.first().map(String::as_str), Some("1"));
    }

    #[test]
    fn links_are_kept_apart_and_forgotten() {
        let sut = RadixConnectHeldRequests::new();
        sut.took(LINK, &request("a"));
        sut.took("other", &request("b"));
        assert_eq!(sut.held("other"), vec!["b"]);
        sut.forget(LINK);
        assert!(sut.held(LINK).is_empty());
        assert_eq!(
            sut.inventory_message(LINK),
            r#"{"discriminator":"walletRequests","interactionIds":[]}"#
        );
    }
}
