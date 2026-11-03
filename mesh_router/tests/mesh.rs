use std::sync::Arc;
use std::thread;
use std::time::Duration;

// Ensure your root Cargo.toml has [lib] defined or your project allows library imports.
// If this is a binary crate, you can place this test directly inside src/routing/engine.rs under #[cfg(test)].
use mesh_router::routing::engine::{Route, Router};

#[test]
fn test_three_node_relay_flow() {
    // 1. Initialize 3 nodes on ephemeral ports
    let mut node1 = Router::new(1, "127.0.0.1:0").unwrap();
    let mut node2 = Router::new(2, "127.0.0.1:0").unwrap();
    let mut node3 = Router::new(3, "127.0.0.1:0").unwrap();

    let addr1 = node1.local_addr().unwrap();
    let addr2 = node2.local_addr().unwrap();
    let addr3 = node3.local_addr().unwrap();

    // 2. Setup topology: Node 1 <-> Node 2 <-> Node 3
    node1.add_route(2, Route::Direct(addr2));
    node1.add_route(3, Route::Relay(addr2)); // Node 1 reaches Node 3 via Node 2

    node2.add_route(1, Route::Direct(addr1));
    node2.add_route(3, Route::Direct(addr3));

    node3.add_route(2, Route::Direct(addr2));
    node3.add_route(1, Route::Relay(addr2)); // Node 3 reaches Node 1 via Node 2

    // 3. Start listeners
    let n1 = Arc::new(node1);
    let n2 = Arc::new(node2);
    let n3 = Arc::new(node3);

    n1.clone().run_listener();
    n2.clone().run_listener();
    n3.clone().run_listener();

    thread::sleep(Duration::from_millis(50));

    // 4. Send message from Node 1 to Node 3
    n1.send_message(3, b"Relayed end-to-end payload");

    // Wait briefly for network traversal
    thread::sleep(Duration::from_millis(100));
}
