# iap2-rs

A Rust implementation of **iAP2** (iPod Accessory Protocol 2) for building Apple MFi accessories that talk to iOS devices over Bluetooth.

Originally written for [Nocturne](https://usenocturne.com), but designed to be reusable for any Linux-based MFi accessory project.

## Features

- Full iAP2 link layer (DETECT, SYN, ACK, retransmission, sequencing)
- MFi authentication via a pluggable [`MfiAuthProvider`](src/mfi.rs) trait
  - Bring your own auth coprocessor (e.g. the Apple MFi auth IC over I2C) or use `MockMfiProvider` for testing
- Device identification and session negotiation
- Now Playing metadata (title, artist, album, duration, elapsed time, playback status, shuffle / repeat modes)
- HID remote (play / pause, next / previous, volume, shuffle, repeat)
- External Accessory (EA) sessions for app communication
- File transfers from the iOS device (e.g. artwork)
- Async / `tokio`-based, event-driven API

## Requirements

- Linux with BlueZ (uses `bluer` for RFCOMM)
- A paired iOS device and a working SDP record advertising the iAP2 service
- Apple MFi membership and an authentication coprocessor for production use

## Usage

Add it to your `Cargo.toml`:

```toml
[dependencies]
iap2-rs = { git = "https://github.com/usenocturne/iap2-rs" }
```

Then connect to a paired iOS device over an RFCOMM stream:

```rust
use std::sync::Arc;
use iap2_rs::{
    connect, ConnectionEvent, DeviceIdentification, Iap2Config, MockMfiProvider,
    ConnectionConfig, FileTransferConfig, HidConfig, LinkConfig, NowPlayingConfig, PowerConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Obtain an RFCOMM stream from `bluer` (paired iOS device, channel discovered via SDP).
    let stream: bluer::rfcomm::Stream = /* ... */;

    let config = Iap2Config {
        identification: DeviceIdentification {
            name: "My Accessory".into(),
            manufacturer: "Acme".into(),
            model_identifier: "ACME-1".into(),
            ea_protocol_name: "com.acme.myproto".into(),
            ..Default::default()
        },
        mfi_provider: Arc::new(MockMfiProvider::empty()), // replace with your auth IC driver
        enable_now_playing: true,
        enable_hid: true,
        link_config: LinkConfig::default(),
        now_playing_config: NowPlayingConfig::default(),
        hid_config: HidConfig::default(),
        file_transfer_config: FileTransferConfig::default(),
        connection_config: ConnectionConfig::default(),
        power_config: PowerConfig::default(),
    };

    let mut conn = connect(stream, config).await?;

    while let Some(event) = conn.events.recv().await {
        match event {
            ConnectionEvent::NowPlayingUpdate { update } => {
                println!("Now playing: {:?}", update);
            }
            ConnectionEvent::Disconnected => break,
            _ => {}
        }
    }

    Ok(())
}
```

### Implementing `MfiAuthProvider`

Real accessories need a hardware Apple Authentication Coprocessor. Implement the trait against your driver:

```rust
use async_trait::async_trait;
use iap2_rs::{MfiAuthProvider, Result};

struct MyAuthChip { /* I2C handle, etc. */ }

#[async_trait]
impl MfiAuthProvider for MyAuthChip {
    async fn read_certificate(&self) -> Result<Vec<u8>> {
        // Read the device certificate (registers 0x30..) from the auth IC.
        todo!()
    }

    async fn challenge_response(&self, challenge: &[u8]) -> Result<Vec<u8>> {
        // Write the challenge, trigger signing, read back the signature.
        todo!()
    }
}
```

### Sending HID commands and launching apps

```rust
use iap2_rs::HidCommand;

conn.send_hid_command(HidCommand::PlayPause)?;
conn.request_app_launch("com.spotify.client".into())?;
```

### EA sessions

When the device opens an EA session for your protocol, you'll receive an `EaSession` on `conn.ea_sessions`:

```rust
while let Some(session) = conn.ea_sessions.recv().await {
    let tx = session.clone_tx(); // hypothetical; use `session.send(bytes)`
    tokio::spawn(async move {
        let mut session = session;
        while let Some(data) = session.rx.recv().await {
            // handle bytes from the iOS app
            let _ = session.send(data); // echo
        }
    });
}
```

## Credits

This software was made possible only through the following individuals:

- [Neel Patel](https://github.com/68p)
- [Dominic Frye](https://github.com/itsnebulalol)

## License

This project is licensed under the **GPL-3.0** license.

We kindly ask that any modifications or distributions made outside of direct forks from this repository include attribution to the original project in the README, as we have worked hard on this. :)

---

> © 2026 Vanta Labs.

> [usenocturne.com](https://usenocturne.com) &nbsp;&middot;&nbsp;
> [GitHub](https://github.com/usenocturne) &nbsp;&middot;&nbsp;
> [Discord](https://discord.gg/mnURjt3M6m)

