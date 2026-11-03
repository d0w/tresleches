//! A local, failure-tolerant content-transfer demo, not a network service.

use one_shot_test::{
    PeerId,
    discovery::MemoryDirectory,
    protocol::{Limits, Publication},
    transfer::{DownloadOptions, download, publish},
    transport::MemoryNetwork,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = Limits::default();
    let mut network = MemoryNetwork::new(limits);
    for id in 1..=4 {
        network.add_peer(PeerId(id), 1024 * 1024)?;
    }
    let mut directory = MemoryDirectory::default();
    let message = b"Data belongs to the network, not one supplying connection.";
    let publication = Publication::new(message, 8, limits)?;
    let published = publish(&publication, 2, &mut network, &mut directory)?;
    println!(
        "published {} bytes as {}",
        message.len(),
        published.content_id
    );
    println!(
        "stored {} chunks with {} replicas each",
        published.chunk_count, published.replicas
    );

    // Drop the local source bytes and disconnect its simulated peer. Fetching
    // now uses only the trusted root address and the other peers' stored data.
    drop(publication);
    network.set_online(PeerId(1), false)?;
    println!("publisher peer 1 disconnected");

    let received = download(
        published.content_id,
        DownloadOptions::default(),
        &mut network,
        &directory,
    )?;
    assert_eq!(received.bytes, message);
    println!(
        "reconstructed from {} chunk providers",
        received.providers.len()
    );
    println!("verified: {}", String::from_utf8(received.bytes)?);

    Ok(())
}
