# FOSS-Deck

An open‑source, phone-based alternative to an Elgato Stream Deck.

This repo contains:

- **pc-server/** – a Rust WebSocket server (Warp) that runs on **Windows** and can control system audio via the Windows Core Audio API.
- **mobile/** – a **Tauri v2** app targeting **Android**. It connects to the PC over WebSocket and sends commands (e.g., volume up/down, mute).

> ⚠️ Security note: this is a **local‑network dev build** (no TLS, no auth). Do not expose it to the public internet yet.

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
- Volume controls (master endpoint):
  - `get_status`
  - `set_volume { level: 0.0..1.0 }`
  - `volume_up { delta?: 0.0..1.0 }` (default 0.05)
  - `volume_down { delta?: 0.0..1.0 }`
  - `toggle_mute`
  - `mute`
  - `unmute`

Example payloads:
```json
{"cmd":"get_status"}
{"cmd":"set_volume","level":0.42}
{"cmd":"volume_up","delta":0.1}
{"cmd":"toggle_mute"}
```

---

## Repo layout

```
FOSS-Deck/
├─ pc-server/            # Rust server (Windows-only)
│  ├─ src/
│  └─ Cargo.toml
├─ mobile/               # Tauri v2 Android app (JS + Rust core)
│  ├─ src/
│  ├─ src-tauri/
│  ├─ package.json
│  └─ tauri.conf.json
└─ README.md
```

---

## Prerequisites

### Windows (for the PC server)

- **Rust** (stable)
  ```powershell
  winget install --id Rustlang.Rustup -e
  ```

- **Visual Studio Build Tools + Windows SDK** (for linking & COM)
  ```powershell
  winget install --id Microsoft.VisualStudio.2022.BuildTools -e --interactive
  ```
  During the installer, select:
  - **C++ build tools**
  - **Windows 11 SDK**

> If you saw `link.exe not found` or COM linking errors, this fixes it.

### Android (for the mobile app)

- **Android Studio** (Narwhal or newer) – includes SDK/Platform Tools
- **Node.js** LTS + npm
- Enable **Windows Developer Mode** (Settings → For developers) to allow symlinks (Tauri creates symlinks for JNI libs).
- A device or emulator.

---

## Getting started

### 1) PC server (Windows)

```powershell
cd pc-server
cargo run
```

You should see logs like:
```
Listening on ws://0.0.0.0:3030/ws
```

**Firewall**: Allow the app/port **3030** on your Windows firewall if prompted.

### 2) Mobile app (Android)

```bash
cd mobile
npm install

# First time only (sets up Android bits)
npm run tauri android init

# Dev run on emulator or device
npm run tauri android dev
```

By default the app connects to your PC’s IP. If you’re using the **Android emulator**, use `10.0.2.2` (Android’s alias for host). On a **physical device**, use the PC’s LAN IP (e.g., `192.168.x.x`).  
If needed, update the connection URL in the mobile code (e.g., `src/main.js`) to:

```js
const WS_URL = "ws://10.0.2.2:3030/ws"; // emulator
// or
const WS_URL = "ws://192.168.0.109:3030/ws"; // real device on same Wi‑Fi
```

---

## Test quickly in a browser (optional)

Create `test.html` on your PC and open it in a browser:

```html
<!doctype html>
<meta charset="utf-8" />
<button id="get">Get Status</button>
<button id="up">Vol +</button>
<button id="down">Vol -</button>
<button id="mute">Toggle Mute</button>
<pre id="log"></pre>
<script>
  const ws = new WebSocket("ws://localhost:3030/ws");
  const log = m => document.querySelector("#log").textContent += m + "\n";

  ws.onopen    = () => log("connected");
  ws.onclose   = () => log("closed");
  ws.onerror   = e  => log("error " + e);
  ws.onmessage = ev => log("← " + ev.data);

  document.querySelector("#get").onclick  = () => ws.send(JSON.stringify({cmd:"get_status"}));
  document.querySelector("#up").onclick   = () => ws.send(JSON.stringify({cmd:"volume_up", delta:0.05}));
  document.querySelector("#down").onclick = () => ws.send(JSON.stringify({cmd:"volume_down", delta:0.05}));
  document.querySelector("#mute").onclick = () => ws.send(JSON.stringify({cmd:"toggle_mute"}));
</script>
```

---

## Common pitfalls

- **“Connection header did not include 'upgrade'”**  
  You’re hitting the HTTP endpoint with `fetch`/XHR. Use a **WebSocket** (`new WebSocket("ws://...")`) and the `/ws` path.

- **Emulator can’t reach the PC**  
  Use `ws://10.0.2.2:3030/ws`. That’s Android’s host alias.

- **Physical device can’t reach the PC**  
  Ensure both are on the same Wi‑Fi, use your PC’s LAN IP, and allow Windows Firewall on port **3030**.

- **“invalid json: unknown variant … get_status”**  
  The server expects **snake_case** commands. If you are editing the server, ensure:
  ```rust
  #[serde(tag = "cmd", rename_all = "snake_case")]
  enum WsCommand { /* ... */ }
  ```

- **Symlink error on Windows during Android dev**  
  Enable **Developer Mode** on Windows.

---

## Building release binaries

### PC server
```powershell
cd pc-server
cargo build --release
# target\release\pc-server.exe
```

### Android (APK)
From `mobile/`:
```bash
npm run tauri android build
# The signed/release flow depends on your keystore; for dev, you’ll get a debug APK.
```

---

## .gitignore

See `.gitignore` in the repo for a clean tree. In short, ignore:
- `pc-server/target/`
- `mobile/node_modules/`
- `mobile/src-tauri/target/`
- `mobile/src-tauri/gen/`
- Android/Gradle caches: `mobile/src-tauri/gen/android/.gradle/`, `**/build/`
- IDE stuff: `.idea/`, `mobile/.vscode/`

---

## Next steps (roadmap ideas)

- Secure pairing (shared secret / QR), TLS
- More actions: app launching, hotkeys, OBS/Spotify integrations
- Cross‑platform server (Windows/macOS/Linux; Linux via PipeWire/PulseAudio)
- Customizable buttons/layout on mobile UI
- Discovery / mDNS instead of manual IP entry
