// Use Tauri API if available (Android injects __TAURI__)
const tauriCore = (globalThis.__TAURI__ && globalThis.__TAURI__.core) || null;
const invoke = tauriCore
    ? tauriCore.invoke
    : async () => { throw new Error("Tauri API not available"); };

const $ = (id) => document.getElementById(id);

// Sections
const connectSection = $("connectSection");
const controlsSection = $("controlsSection");

// Connect UI
const scanBtn = $("scan");
const directBtn = $("direct");
const scanStatus = $("scanStatus");
const hostsEl = $("hosts");

// Controls UI
const connStatus = $("connStatus");
const volSlider = $("vol");
const volVal = $("volVal");
const volUpBtn = $("volUp");
const volDownBtn = $("volDown");
const toggleMuteBtn = $("toggleMute");
const muteStatus = $("muteStatus");
const getStatusBtn = $("getStatus");
const disconnectBtn = $("disconnect");

// Log
const logEl = $("log");
const clearLogBtn = $("clearLog");

// State
let ws = null;
let volumeDebounce = null;

// ---------- Helpers ----------
function log(line, kind = "info") {
  const ts = new Date().toLocaleTimeString();
  const prefix = kind === "err" ? "❌" : kind === "ok" ? "✅" : "•";
  logEl.textContent += `[${ts}] ${prefix} ${line}\n`;
  logEl.scrollTop = logEl.scrollHeight;
}

function setMode(connected) {
  // swap sections
  connectSection.classList.toggle("hidden", connected);
  controlsSection.classList.toggle("hidden", !connected);

  if (connected) {
    connStatus.classList.add("ok");
    connStatus.classList.remove("err");
  } else {
    connStatus.classList.remove("ok");
    connStatus.classList.remove("err");
  }
}

function setStatusError(msg) {
  connStatus.textContent = msg;
  connStatus.classList.remove("ok");
  connStatus.classList.add("err");
}

function applyStatus({ volume, muted }) {
  if (typeof volume === "number") {
    volSlider.value = String(volume);
    volVal.textContent = `${Math.round(volume * 100)}%`;
  }
  if (typeof muted === "boolean") {
    muteStatus.textContent = muted ? "Muted" : "Not muted";
  }
}

function buildWsUrlFromHost(h) {
  const path = h.path || "/ws";
  return `ws://${h.ip}:${h.port}${path}`;
}

function normalizeDirectInputToWs(input) {
  // Accept:
  // - ws://host[:port]/ws
  // - wss://host...
  // - 192.168.1.42            -> ws://192.168.1.42:3030/ws
  // - 192.168.1.42:3031       -> ws://192.168.1.42:3031/ws
  const raw = input.trim();
  if (raw.startsWith("ws://") || raw.startsWith("wss://")) return raw;

  // naive IP[:port]
  const m = raw.match(/^(\d{1,3}(?:\.\d{1,3}){3})(?::(\d{1,5}))?$/);
  if (m) {
    const ip = m[1];
    const port = m[2] ? Number(m[2]) : 3030;
    return `ws://${ip}:${port}/ws`;
  }
  return ""; // invalid
}

// ---------- Discovery ----------
scanBtn.addEventListener("click", async () => {
  if (!tauriCore) {
    scanStatus.textContent = "Scan disabled (no Tauri API)";
    log("Tauri API not available for discovery", "err");
    return;
  }

  hostsEl.innerHTML = "";
  scanBtn.disabled = true;
  scanStatus.textContent = "Scanning…";

  try {
    // lib.rs command: discover_hosts(timeout_ms?: u64)
    const hosts = await invoke("discover_hosts", { timeoutMs: 1200 });
    if (!hosts || hosts.length === 0) {
      scanStatus.textContent = "No PCs found.";
      return;
    }

    scanStatus.textContent = `Found ${hosts.length} host(s)`;
    hostsEl.innerHTML = hosts.map(h => {
      const url = buildWsUrlFromHost(h);
      const name = h.name ? `${h.name}` : "PC";
      const version = h.version ? ` • v${h.version}` : "";
      return `
        <div class="host">
          <div>
            <div class="name">${name}${version}</div>
            <div class="url">${url}</div>
          </div>
          <button data-url="${url}" class="primary">Connect</button>
        </div>
      `;
    }).join("");

    for (const btn of hostsEl.querySelectorAll("button[data-url]")) {
      btn.addEventListener("click", () => {
        connect(btn.dataset.url);
      });
    }
  } catch (e) {
    scanStatus.textContent = "Scan failed";
    log(`Scan error: ${e}`, "err");
  } finally {
    scanBtn.disabled = false;
  }
});

// ---------- Direct connect prompt ----------
directBtn.addEventListener("click", () => {
  const v = prompt("Enter PC IP (or IP:port) or full ws:// URL:", "");
  if (v === null) return; // canceled
  const url = normalizeDirectInputToWs(v);
  if (!url) {
    log("Invalid address. Use IP (e.g. 192.168.0.10) or ws:// URL.", "err");
    return;
  }
  connect(url);
});

// ---------- WebSocket ----------
function connect(url) {
  try {
    if (ws) {
      try { ws.close(); } catch {}
      ws = null;
    }
    log(`Connecting to ${url} …`);
    ws = new WebSocket(url);

    ws.onopen = () => {
      log("WebSocket open", "ok");
      setMode(true);
      connStatus.textContent = `Connected to ${url}`;
      sendCmd({ cmd: "get_status" });
    };

    ws.onmessage = (ev) => {
      const data = ev.data;
      try {
        const obj = JSON.parse(data);
        if (obj.type === "hello") {
          log(`Hello from ${obj.server} v${obj.version}`);
        } else if (obj.type === "status") {
          applyStatus(obj);
          log(
              `Status: vol=${Math.round(obj.volume * 100)}% mute=${obj.muted ? "on" : "off"}`
          );
        } else if (obj.type === "ok") {
          if (typeof obj.volume === "number") applyStatus(obj);
          log(`OK: ${obj.action ?? ""}`);
        } else if (obj.type === "shutdown") {
          // Server told us it's going down; close locally.
          log("Server shutdown — disconnecting");
          try { ws.close(); } catch {}
        } else if (obj.type === "error") {
          setStatusError(`Server error: ${obj.message}`);
          log(`Server error: ${obj.message}`, "err");
        } else {
          log(`Message: ${String(data).slice(0, 200)}`);
        }
      } catch {
        log(`Non-JSON message: ${String(data).slice(0, 200)}`, "err");
      }
    };

    ws.onerror = () => {
      setStatusError("WebSocket error");
      log("WebSocket error", "err");
    };

    ws.onclose = () => {
      log("WebSocket closed");
      setMode(false);
      connStatus.textContent = "Disconnected";
    };
  } catch (e) {
    setStatusError("Connect failed");
    log(`Connect exception: ${e}`, "err");
  }
}

function disconnect() {
  if (ws) {
    try { ws.close(); } catch {}
    ws = null;
  }
  setMode(false);
  connStatus.textContent = "Disconnected";
  log("Disconnected");
}

disconnectBtn.addEventListener("click", disconnect);

// ---------- Commands ----------
function sendCmd(obj) {
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    setStatusError("Not connected");
    return;
  }
  try {
    ws.send(JSON.stringify(obj));
  } catch (e) {
    log(`Send failed: ${e}`, "err");
  }
}

volUpBtn.addEventListener("click", () => sendCmd({ cmd: "volume_up", delta: 0.05 }));
volDownBtn.addEventListener("click", () => sendCmd({ cmd: "volume_down", delta: 0.05 }));
toggleMuteBtn.addEventListener("click", () => sendCmd({ cmd: "toggle_mute" }));
getStatusBtn.addEventListener("click", () => sendCmd({ cmd: "get_status" }));

// Debounce set_volume while dragging
volSlider.addEventListener("input", () => {
  const level = parseFloat(volSlider.value);
  volVal.textContent = `${Math.round(level * 100)}%`;
  if (volumeDebounce) clearTimeout(volumeDebounce);
  volumeDebounce = setTimeout(() => {
    sendCmd({ cmd: "set_volume", level });
  }, 120);
});

// ---------- Log ----------
clearLogBtn.addEventListener("click", () => (logEl.textContent = ""));

// ---------- Init ----------
(function init() {
  setMode(false);
  scanStatus.textContent = "";
  if (!tauriCore) {
    scanBtn.disabled = true;
    scanStatus.textContent = "Scan disabled (no Tauri API)";
  }
})();
