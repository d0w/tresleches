use std::{collections::BTreeMap, net::SocketAddr};
use tokio::sync::{Notify, RwLock};

pub struct MeshState {
    pub(crate) udp_address: String,
    pub(crate) neighbors: RwLock<BTreeMap<String, String>>,
    pub(crate) neighbors_changed: Notify,
}

impl MeshState {
    pub fn new(udp_address: SocketAddr) -> Self {
        Self {
            udp_address: udp_address.to_string(),
            neighbors: RwLock::new(BTreeMap::new()),
            neighbors_changed: Notify::new(),
        }
    }
}
