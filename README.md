# Torrctl - A High-Performance Rust BitTorrent Client

Torrctl is a minimal, blazingly fast BitTorrent client built entirely from scratch in Rust. Designed with performance and resource-safety in mind, `torrctl` bypasses heavyweight abstraction layers and interacts directly with BitTorrent swarms at the TCP socket layer. 

By implementing custom parsing and core chunks of the BitTorrent protocol specifications (BEPs), it provides an efficient and secure way to pull files from decentralized peer-to-peer networks using standard `.torrent` files or modern Magnet URIs.

---

## 🚀 Core Features

- **Magnet Link Support (BEP 9 / BEP 10):** Automatically negotiates with peers using the heavily specialized `Extension Protocol` to discover and fetch torrent metadata directly from the swarm, eliminating the need to physically download `.torrent` files.
- **Robust Bencode Parser:** Implements a custom, high-speed bencode decoder designed to unwrap tracker metadata and peer responses without choking on massive dictionaries.
- **OOM (Out-of-Memory) Resistant:** BitTorrent streams can easily become desynced or malicious. `torrctl` employs strict 2MB network payload bound checks to prevent giant memory allocations before reading buffers, guaranteeing your OS won't crash when negotiating with noisy peers.
- **Threaded TCP Dispatch System:** Rapidly spawns distinct communication threads per peer connection, managing non-blocking `WouldBlock` HTTP timeouts while isolating peer states so a slow seeder never halts the primary download queue.
- **Dynamic In-Place CLI Interface:** Features a totally invisible backend — muting all socket connection warnings — while exposing a gorgeous, terminal-friendly progress bar representing chunks loaded.

---

## 🛠️ Architecture

At the heart of the client are four main modules:
1. **`main.rs` & CLI Engine:** Leverages `clap` to process inputs, routing execution flows based on whether a local tracker file or a remote magnet hash was provided.
2. **`bencode.rs`:** Translates the raw byte-arrays received from trackers into heavily nested `serde_json::Value` structures.
3. **`network.rs`:** Dispatches requests to open HTTP trackers (like OpenTrackr), handles url-encoding of the SHA-1 info_hash, and requests initial BitTorrent handshakes.
4. **`peer.rs`:** The core engine that multiplexes the block downloading logic. Utilizing `Arc<Mutex<HashMap>>`, it safely coordinates global block states (what pieces are actively in-progress versus fully realized) ensuring 50+ concurrent peer threads can safely slice chunks of the target file into a single disk asset without data races.

---

## 📖 Installation & Usage

You can install `torrctl` directly via Cargo:

```bash
cargo install torrctl
```

Once installed, you can access the `torrctl` command globally from any directory.

### Commands Overview

| Flag | Argument | Description | Example |
|------|----------|-------------|---------|
| **`-t`** | `--torrent` | Download from a `.torrent` file using a local path or HTTP URL. | `torrctl -t ubuntu.iso.torrent` |
| **`-m`** | `--magnet` | Download from a Magnet URI. Automatically fetches metadata via BEP 9/10. | `torrctl -m "magnet:?xt=urn:btih:..."` |
| **`-o`** | `--output` | (*Optional*) Override the output directory. Defaults to the current working directory. | `torrctl -t sample.torrent -o ~/Downloads` |

### Quick Examples

```bash
# Download a magnet link directly to your Desktop
torrctl -m "magnet:?xt=urn:btih:3A000A3EC4..." -o ~/Desktop

# Download from a local torrent file
torrctl -t ./movies/sample.torrent
```
