/// The longest `interactionId` taken as one.
pub const RADIX_CONNECT_MAX_INTERACTION_ID_LENGTH: usize = 64;

/// The top-level `interactionId` of a message exchanged with a connector extension (a wallet
/// interaction, its answer, or a ledger message), or `None` when it has none.
///
/// Only an id that looks like one is returned (1 to 64 letters, digits, `-` or `_`), so that
/// nothing else of a message can end up in a log.
pub fn radix_connect_message_interaction_id(message: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(message).ok()?;
    let id = json.as_object()?.get("interactionId")?.as_str()?;
    let looks_like_an_id = (1..=RADIX_CONNECT_MAX_INTERACTION_ID_LENGTH)
        .contains(&id.chars().count())
        && id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
    looks_like_an_id.then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interaction_id_of_an_interaction() {
        assert_eq!(
            radix_connect_message_interaction_id(
                r#"{"interactionId":"5f1c-a_9","items":{}}"#
            ),
            Some("5f1c-a_9".to_owned())
        );
    }

    #[test]
    fn no_interaction_id() {
        assert_eq!(
            radix_connect_message_interaction_id(r#"{"items":{}}"#),
            None
        );
        assert_eq!(radix_connect_message_interaction_id("not json"), None);
        assert_eq!(radix_connect_message_interaction_id("[1,2]"), None);
        assert_eq!(
            radix_connect_message_interaction_id(r#"{"interactionId":7}"#),
            None
        );
    }

    #[test]
    fn an_id_that_does_not_look_like_one_is_not_returned() {
        assert_eq!(
            radix_connect_message_interaction_id(r#"{"interactionId":""}"#),
            None
        );
        assert_eq!(
            radix_connect_message_interaction_id(
                r#"{"interactionId":"has space"}"#
            ),
            None
        );
        let long = "a".repeat(RADIX_CONNECT_MAX_INTERACTION_ID_LENGTH + 1);
        assert_eq!(
            radix_connect_message_interaction_id(&format!(
                r#"{{"interactionId":"{long}"}}"#
            )),
            None
        );
        let longest = "a".repeat(RADIX_CONNECT_MAX_INTERACTION_ID_LENGTH);
        assert_eq!(
            radix_connect_message_interaction_id(&format!(
                r#"{{"interactionId":"{longest}"}}"#
            )),
            Some(longest)
        );
    }
}
