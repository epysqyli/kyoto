//! When adding new scripts that have a previous history, we can rescan
//! the filters for inclusions of these scripts and download the relevant
//! blocks.

use kyoto::{chain::checkpoints::HeaderCheckpoint, core::builder::NodeBuilder};
use kyoto::{Address, Client, Event, Log, Network};
use std::collections::HashSet;
use std::str::FromStr;

const NETWORK: Network = Network::Signet;
const RECOVERY_HEIGHT: u32 = 170_000;

#[tokio::main]
async fn main() {
    // Add third-party logging
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber).unwrap();
    // Add Bitcoin scripts to scan the blockchain for
    let address = Address::from_str("tb1q9pvjqz5u5sdgpatg3wn0ce438u5cyv85lly0pc")
        .unwrap()
        .require_network(NETWORK)
        .unwrap()
        .into();
    let mut addresses = HashSet::new();
    addresses.insert(address);
    // Create a new node builder
    let builder = NodeBuilder::new(NETWORK);
    // Add node preferences and build the node/client
    let (node, client) = builder
        // The Bitcoin scripts to monitor
        .add_scripts(addresses)
        // Only scan blocks strictly after an anchor checkpoint
        .anchor_checkpoint(HeaderCheckpoint::closest_checkpoint_below_height(
            RECOVERY_HEIGHT,
            NETWORK,
        ))
        // The number of connections we would like to maintain
        .num_required_peers(1)
        .build_node()
        .unwrap();
    // Run the node and wait for the sync message;
    tokio::task::spawn(async move { node.run().await });
    tracing::info!("Running the node and waiting for a sync message. Please wait a minute!");
    // Split the client into components that send messages and listen to messages
    let Client {
        sender,
        mut log_rx,
        mut event_rx,
    } = client;
    // Sync with the single script added
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                if let Some(Event::Synced(update)) = event {
                    tracing::info!("Synced chain up to block {}", update.tip().height);
                    tracing::info!("Chain tip: {}", update.tip().hash);
                    break;
                }
            }
            log = log_rx.recv() => {
                if let Some(log) = log {
                    match log {
                        Log::Dialog(d) => tracing::info!("{d}"),
                        Log::Warning(warning) => tracing::warn!("{warning}"),
                        _ => (),
                    }
                }
            }
        }
    }
    // Add new scripts to the node.
    let new_script =
        Address::from_str("tb1par6ufhp0t448t908kyyvkp3a48r42qcjmg0z9p6a0zuakc44nn2seh63jr")
            .unwrap()
            .require_network(NETWORK)
            .unwrap();
    sender.add_script(new_script).await.unwrap();
    // // Tell the node to look for these new scripts
    sender.rescan().await.unwrap();
    tracing::info!("Starting rescan");
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                if let Some(Event::Synced(update)) = event {
                    tracing::info!("Synced chain up to block {}", update.tip().height);
                    tracing::info!("Chain tip: {}", update.tip().hash);
                    break;
                }
            }
            log = log_rx.recv() => {
                if let Some(log) = log {
                    match log {
                        Log::Dialog(d) => tracing::info!("{d}"),
                        Log::Warning(warning) => tracing::warn!("{warning}"),
                        _ => (),
                    }
                }
            }
        }
    }
    let _ = sender.shutdown().await;
    tracing::info!("Shutting down");
}
