// Avoid ESM import for Tauri API on Android;
// use the injected global instead. If it doesn't exist, we degrade gracefully.
const tauriCore = (globalThis.__TAURI__ && globalThis.__TAURI__.core) || null;
const invoke = tauriCore
    ? tauriCore.invoke
    : async () => { throw new Error("Tauri API not available"); };

const $ = (id) => document.getElementById(id);

// Elements
const scanBtn = $("scan");
const scanStatus = $("scanStatus");
const hostsEl = $("hosts");

const wsUrlInput = $("wsUrl");
const useEmuBtn = $("useEmu");
const connectBtn = $("connect");
const disconnectBtn = $("disconnect");
const connStatus = $("connStatus");

const volSlider = $("vol");
const volVal = $("volVal");
const volUpBtn = $("volUp");
const volDownBtn = $("volDown");
const muteBtn = $("mute");
const unmuteBtn = $("unmute");
const toggleMuteBtn = $("toggleMute");
const getStatusBtn = $("getStatus");

const logEl = $("log");
const clearLogBtn = $("clearLog");

// State
let ws = null;
let volumeDebounce = null;

// ----- Helpers -----
function log(line, kind = "info") {
  const time = new Date().toLocaleTimeString();
  const prefix = kind === "err" ? "❌" : kind === "ok" ? "✅" : "•";
  logEl.textContent += `[${time}] ${prefix} ${line}\n`;
  logEl.scrollTop = logEl.scrollHeight;
}

function setConnectedUI(connected, url = null) {
  if (connected) {
    connStatus.textContent = `Connected ${url ? `to ${url}` : ""}`;
    connStatus.classList.remove("err");
    connStatus.classList.add("ok");
  } else {
    connStatus.textContent = "Disconnected";
    connStatus.classList.remove("ok");
    connStatus.classList.remove("err");
  }

  connectBtn.disabled = connected;
  disconnectBtn.disabled = !connected;

  const controls = [
    volSlider, volUpBtn, volDownBtn, muteBtn, unmuteBtn, toggleMuteBtn,
    getStatusBtn
  ];
  for (const el of controls) el.disabled = !connected;
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
    const base = `Connected${muted ? " (muted)" : ""}`;
    connStatus.textContent = base;
  }
}

function buildWsUrlFromHost(h) {
  const path = h.path || "/ws";
  return `ws://${h.ip}:${h.port}${path}`;
}

// ----- Discovery -----
scanBtn.addEventListener("click", async () => {
  if (!tauriCore) {
    scanStatus.textContent = "Unavailable (no Tauri core)";
    log("Scan blocked: Tauri API not available", "err");
    return;
  }

  hostsEl.innerHTML = "";
  scanBtn.disabled = true;
  scanStatus.textContent = "Scanning...";
  try {
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
        wsUrlInput.value = btn.dataset.url;
        connect();
      });
    }

    if (hosts.length === 1) {
      wsUrlInput.value = buildWsUrlFromHost(hosts[0]);
    }
  } catch (e) {
    scanStatus.textContent = "Scan failed";
    log(`Scan error: ${e}`, "err");
  } finally {
    scanBtn.disabled = false;
  }
});

// ----- Manual connect / emulator helper -----
useEmuBtn.addEventListener("click", () => {
  wsUrlInput.value = "ws://10.0.2.2:3030/ws";
});

connectBtn.addEventListener("click", connect);
disconnectBtn.addEventListener("click", disconnect);

function connect() {
  const url = wsUrlInput.value.trim();
  if (!url.startsWith("ws://") && !url.startsWith("wss://")) {
    setStatusError("Invalid URL (must start with ws:// or wss://)");
    return;
  }
  try {
    if (ws) {
      try { ws.close(); } catch {}
      ws = null;
    }
    log(`Connecting to ${url} ...`);
    ws = new WebSocket(url);

    ws.onopen = () => {
      log("WebSocket open", "ok");
      setConnectedUI(true, url);
      sendCmd({ cmd: "get_status" });
    };

    ws.onmessage = (ev) => {
      const data = ev.data;
      try {
        const obj = JSON.parse(data);
        if (obj.type === "status") {
          applyStatus(obj);
          log(`Status: vol=${Math.round(obj.volume * 100)}% mute=${obj.muted ? "on" : "off"}`);
        } else if (obj.type === "hello") {
          log(`Server hello: ${obj.server} v${obj.version}`);
        } else if (obj.type === "ok") {
          if (typeof obj.volume === "number") applyStatus(obj);
          log(`OK: ${obj.action ?? ""}`);
        } else if (obj.type === "error") {
          setStatusError(`Server error: ${obj.message}`);
          log(`Error from server: ${obj.message}`, "err");
        } else {
          log(`Msg: ${String(data).slice(0, 200)}`);
        }
      } catch {
        log(`Non-JSON msg: ${String(data).slice(0, 200)}`, "err");
      }
    };

    ws.onerror = (ev) => {
      setStatusError("WebSocket error");
      log(`WS error: ${JSON.stringify(ev)}`, "err");
    };

    ws.onclose = () => {
      log("WebSocket closed");
      setConnectedUI(false);
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
  setConnectedUI(false);
  log("Disconnected");
}

// ----- Commands -----
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
muteBtn.addEventListener("click", () => sendCmd({ cmd: "mute" }));
unmuteBtn.addEventListener("click", () => sendCmd({ cmd: "unmute" }));
toggleMuteBtn.addEventListener("click", () => sendCmd({ cmd: "toggle_mute" }));
getStatusBtn.addEventListener("click", () => sendCmd({ cmd: "get_status" }));

// Debounce set_volume while dragging the slider
volSlider.addEventListener("input", () => {
  const level = parseFloat(volSlider.value);
  volVal.textContent = `${Math.round(level * 100)}%`;
  if (volumeDebounce) clearTimeout(volumeDebounce);
  volumeDebounce = setTimeout(() => {
    sendCmd({ cmd: "set_volume", level });
  }, 120);
});

// ----- Log -----
clearLogBtn.addEventListener("click", () => (logEl.textContent = ""));

// ----- Init -----
(function init() {
  setConnectedUI(false);
  wsUrlInput.placeholder = "ws://<pc-ip>:3030/ws";

  // If Tauri API isn't available, disable scan gracefully
  if (!tauriCore) {
    scanBtn.disabled = true;
    scanStatus.textContent = "Scan disabled (no Tauri API)";
  }
})();
