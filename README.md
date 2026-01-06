# FOSS-Deck

An open‑source, phone-based alternative to an Elgato Stream Deck.

This repo contains:

- **pc-server/** – a Rust WebSocket server (Warp) that runs on **Windows** and can control system audio via the Windows Core Audio API.
- **mobile/** – a **Tauri v2** app targeting **Android**. It connects to the PC over WebSocket and sends commands (e.g., volume up/down, mute).

---

## What’s inside

- **Rust** (tokio, warp, serde, anyhow, env_logger)
- **Windows Core Audio** via `windows` crate (`IAudioEndpointVolume`)
- **Tauri v2** for Android (JS frontend + Rust mobile core)
- **WebSocket protocol** with simple JSON commands (snake_case)

---

## Features (current)

- WebSocket server at `ws://<pc-ip>:3030/ws`
- Health check: `http://<pc-ip>:3030/health` → `ok`
- Volume controls
- Multimedia controls
- Screenshot
- Microphone control

---

## Getting started

Just download the builds for Windows and Android.
