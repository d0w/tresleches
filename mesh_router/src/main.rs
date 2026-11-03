use mesh_router::routing::{engine::Route, engine::Router};
use std::sync::Arc;
use std::thread;

fn main() -> std::io::Result<()> {
    println!("Hello, world!");
    let mut node1 = Router::new(1, "127.0.0.1:9001")?;
    let mut node2 = Router::new(2, "127.0.0.1:9002")?;
    let mut node3 = Router::new(3, "127.0.0.1:9003")?;

    // Configure Node 1 routes:
    // Directly connected to Node 2
    node1.add_route(2, Route::Direct("127.0.0.1:9002".parse().unwrap()));
    // To reach Node 3, forward through Node 2
    node1.add_route(3, Route::Relay("127.0.0.1:9002".parse().unwrap()));

    // Configure Node 2 routes (acts as an intermediary relay for both):
    node2.add_route(1, Route::Direct("127.0.0.1:9001".parse().unwrap()));
    node2.add_route(3, Route::Direct("127.0.0.1:9003".parse().unwrap()));

    // Configure Node 3 routes:
    node3.add_route(2, Route::Direct("127.0.0.1:9002".parse().unwrap()));
    node3.add_route(1, Route::Relay("127.0.0.1:9002".parse().unwrap()));

    // Wrap nodes in Arc to run concurrent listener threads
    let node1 = Arc::new(node1);
    let node2 = Arc::new(node2);
    let node3 = Arc::new(node3);

    node1.clone().run_listener();
    node2.clone().run_listener();
    node3.clone().run_listener();

    // Allow sockets to bind and listen
    thread::sleep(std::time::Duration::from_millis(50));

    println!("--- Sending Message 1: Node 1 -> Node 2 (Direct) ---");
    node1.send_message(2, b"Hello Neighbor Node 2!");
    thread::sleep(std::time::Duration::from_millis(50));

    println!("\n--- Sending Message 2: Node 1 -> Node 3 (Multi-hop Relay via Node 2) ---");
    node1.send_message(3, b"Greetings across the mesh, Node 3!");
    thread::sleep(std::time::Duration::from_millis(100));

    Ok(())
}
