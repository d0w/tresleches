use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct Request {
    pub(super) version: u32,

    #[serde(flatten)]
    pub(super) command: Command,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum Command {
    GetStatus,
    UpsertNeighbor {
        peer_id: String,
        udp_address: String,
    },
}
