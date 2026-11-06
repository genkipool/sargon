//! Rules of Radix Connect between the wallet and the connector extensions that hold whatever
//! transport carries the messages (WebRTC data channels, iroh), so that every wallet applies them
//! the same way. The transports themselves stay in the hosts.

mod held_requests;
mod interaction_id_of;
mod linking_race;
mod message_deduplicator;
mod transport;

pub use held_requests::*;
pub use interaction_id_of::*;
pub use linking_race::*;
pub use message_deduplicator::*;
pub use transport::*;
