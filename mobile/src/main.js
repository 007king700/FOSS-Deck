// --- Optional Tauri discovery ---
const tauriCore = (globalThis.__TAURI__ && globalThis.__TAURI__.core) || null;
const invoke = tauriCore
    ? tauriCore.invoke
    : async () => { throw new Error("Tauri API not available"); };

const $ = (id) => document.getElementById(id);

// Screens
const homeScreen = $("homeScreen");
const connectedScreen = $("connectedScreen");

// Home UI
const scanBtn = $("scanBtn");
const homeSettingsBtn = $("homeSettingsBtn"); // currently unused
const directBtn = $("directBtn");
const scanStatus = $("scanStatus");
const recentList = $("recentList");
const availableList = $("availableList");
const recentSub = $("recentSub");

// Connected UI
const backBtn = $("backBtn");
const editBtn = $("editBtn");
const tileGrid = $("tileGrid");
const connTitle = $("connTitle");
const connSub = $("connSub");
const pairHint = $("pairHint");
const editHint = $("editHint");
const pagePrev = $("pagePrev");
const pageNext = $("pageNext");
const pageNum = $("pageNum");

// Pair modal
const pairModal = $("pairModal");
const pairCodeInput = $("pairCodeInput");
const pairCancel = $("pairCancel");
const pairConfirm = $("pairConfirm");
const pairError = $("pairError");

// --- Audio state from PC ---
const audioState = {
  muted: false,
  volume: 1.0,
};

// Debug log
const logEl = $("log");
function log(msg) {
  // logEl.classList.remove("hidden");
  // logEl.textContent += msg + "\n";
}

// --- Persistent identity ---
let deviceId = localStorage.getItem("fossdeck_device_id");
if (!deviceId) {
  deviceId = crypto.randomUUID();
  localStorage.setItem("fossdeck_device_id", deviceId);
}

// token is global (server issues token on pairing_ok)
let authToken = localStorage.getItem("fossdeck_token");

// --- Connection state ---
let ws = null;
let currentUrl = "";
let currentPcName = "";
let isPaired = false;
let heartbeatTimer = null;

// --- Recent PCs store ---
const RECENTS_KEY = "fossdeck_recents_v1";

function loadRecents() {
  try {
    const s = localStorage.getItem(RECENTS_KEY);
    const arr = s ? JSON.parse(s) : [];
    if (!Array.isArray(arr)) return [];
    return arr;
  } catch {
    return [];
  }
}

function saveRecents(arr) {
  localStorage.setItem(RECENTS_KEY, JSON.stringify(arr));
}

function upsertRecent({ name, url }) {
  const arr = loadRecents();
  const now = Date.now();
  const idx = arr.findIndex(x => x.url === url);
  const entry = { name: name || url, url, lastConnected: now };
  if (idx >= 0) arr[idx] = { ...arr[idx], ...entry };
  else arr.unshift(entry);
  // keep max 10
  const out = arr
      .sort((a,b) => (b.lastConnected || 0) - (a.lastConnected || 0))
      .slice(0, 10);
  saveRecents(out);
}

function forgetRecent(url) {
  const arr = loadRecents().filter(x => x.url !== url);
  saveRecents(arr);
}

// --- Layout store (modular grid) ---
const LAYOUT_KEY = "fossdeck_layout_v1";
const DEFAULT_LAYOUT = [
  "toggle_mute",
  "volume_down",
  "volume_up",
];

function loadLayout() {
  try {
    const s = localStorage.getItem(LAYOUT_KEY);
    const arr = s ? JSON.parse(s) : null;
    if (Array.isArray(arr) && arr.length) return arr;
  } catch {}
  return [...DEFAULT_LAYOUT];
}

function saveLayout(arr) {
  localStorage.setItem(LAYOUT_KEY, JSON.stringify(arr));
}

// --- Actions registry (add more later without changing layout system) ---
const ACTIONS = {
  toggle_mute: {
    id: "toggle_mute",
    title: "Mute",
    icon: () =>
        audioState.muted
            ? "assets/mute.svg"
            : "assets/unmute.svg",
    enabled: () => isPaired,
    run: () => {
      // optimistic UI update
      audioState.muted = !audioState.muted;
      renderTiles();

      sendCmd({ cmd: "toggle_mute" });
    },
  },
  volume_up: {
    id: "volume_up",
    title: "Volume +",
    icon: "assets/volume_up.svg",
    enabled: () => isPaired,
    run: () => sendCmd({ cmd: "volume_up", delta: 0.05 }),
  },
  volume_down: {
    id: "volume_down",
    title: "Volume −",
    icon: "assets/volume_down.svg",
    enabled: () => isPaired,
    run: () => sendCmd({ cmd: "volume_down", delta: 0.05 }),
  },
};

// --- UI helpers ---
function showHome() {
  homeScreen.classList.remove("hidden");
  connectedScreen.classList.add("hidden");
}

function showConnected() {
  homeScreen.classList.add("hidden");
  connectedScreen.classList.remove("hidden");
}

function setConnectedMeta(title, sub) {
  connTitle.textContent = title || "Connected";
  connSub.textContent = sub || "—";
}

function openPairModal() {
  pairError.textContent = "";
  pairCodeInput.value = "";
  pairModal.classList.remove("hidden");
  setTimeout(() => pairCodeInput.focus(), 50);
}

function closePairModal() {
  pairModal.classList.add("hidden");
}

pairCancel.addEventListener("click", closePairModal);

// --- Heartbeat ---
function stopHeartbeat() {
  if (heartbeatTimer) {
    clearInterval(heartbeatTimer);
    heartbeatTimer = null;
  }
}

function startHeartbeat() {
  stopHeartbeat();
  sendCmd({ cmd: "get_status" });
  heartbeatTimer = setInterval(() => {
    if (ws && ws.readyState === WebSocket.OPEN && isPaired) {
      sendCmd({ cmd: "get_status" });
    }
  }, 5000);
}

// --- Render recents + available lists ---
function renderRecents() {
  const recents = loadRecents();
  recentList.innerHTML = "";

  if (!recents.length) {
    recentSub.textContent = "No saved PCs yet";
    return;
  }

  recentSub.textContent = `${recents.length} saved`;

  for (const pc of recents) {
    const row = document.createElement("div");
    row.className = "list-item";
    row.innerHTML = `
      <div class="left">
        <div class="name">${escapeHtml(pc.name || "PC")}</div>
        <div class="sub">${escapeHtml(pc.url)}</div>
      </div>
      <div class="row-actions">
        <button class="btn small primary" data-act="connect">Connect</button>
        <button class="btn small secondary" data-act="forget">Forget</button>
      </div>
    `;
    row.querySelector('[data-act="connect"]').addEventListener("click", () => {
      connect(pc.url, pc.name || "");
    });
    row.querySelector('[data-act="forget"]').addEventListener("click", () => {
      forgetRecent(pc.url);
      renderRecents();
    });
    recentList.appendChild(row);
  }
}

function renderAvailable(hosts) {
  availableList.innerHTML = "";

  if (!hosts || !hosts.length) {
    scanStatus.textContent = "No PCs found.";
    return;
  }

  scanStatus.textContent = `Found ${hosts.length} host(s)`;

  for (const h of hosts) {
    const url = buildWsUrlFromHost(h);
    const name = h.name || "PC";
    const row = document.createElement("div");
    row.className = "list-item";
    row.innerHTML = `
      <div class="left">
        <div class="name">${escapeHtml(name)}</div>
        <div class="sub">${escapeHtml(url)}</div>
      </div>
      <div class="row-actions">
        <button class="btn small primary">Connect</button>
      </div>
    `;
    row.querySelector("button").addEventListener("click", () => connect(url, name));
    availableList.appendChild(row);
  }
}

function buildWsUrlFromHost(h) {
  const path = h.path || "/ws";
  return `ws://${h.ip}:${h.port}${path}`;
}

function normalizeDirectInputToWs(input) {
  const raw = input.trim();
  if (raw.startsWith("ws://") || raw.startsWith("wss://")) return raw;

  const m = raw.match(/^(\d{1,3}(?:\.\d{1,3}){3})(?::(\d{1,5}))?$/);
  if (m) {
    const ip = m[1];
    const port = m[2] ? Number(m[2]) : 3030;
    return `ws://${ip}:${port}/ws`;
  }
  return "";
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  }[c]));
}

// --- Scan / Direct connect ---
scanBtn.addEventListener("click", async () => {
  availableList.innerHTML = "";
  scanStatus.textContent = "Scanning…";

  if (!tauriCore) {
    scanStatus.textContent = "Scan disabled (no Tauri API)";
    return;
  }

  try {
    const hosts = await invoke("discover_hosts", { timeoutMs: 1200 });
    renderAvailable(hosts);
  } catch (e) {
    scanStatus.textContent = "Scan failed";
    log(`Scan error: ${e}`);
  }
});

directBtn.addEventListener("click", () => {
  const v = prompt("Enter PC IP (or IP:port) or full ws:// URL:", "");
  if (v === null) return;
  const url = normalizeDirectInputToWs(v);
  if (!url) return;
  connect(url, url);
});

// --- Connected page "pages" (simple placeholder; can expand later) ---
let currentPage = 1;
pagePrev.addEventListener("click", () => {
  if (currentPage > 1) currentPage--;
  pageNum.textContent = String(currentPage);
});
pageNext.addEventListener("click", () => {
  currentPage++;
  pageNum.textContent = String(currentPage);
});

// --- Edit mode for modular tiles ---
let editMode = false;
editBtn.addEventListener("click", () => {
  editMode = !editMode;
  editHint.classList.toggle("hidden", !editMode);
  renderTiles();
});

// Back button
backBtn.addEventListener("click", () => {
  disconnect();
});

// --- Pair modal confirm ---
pairConfirm.addEventListener("click", () => {
  const code = (pairCodeInput.value || "").trim();
  if (!code) {
    pairError.textContent = "Please enter the code.";
    return;
  }
  sendCmd({
    cmd: "pair",
    code,
    device_id: deviceId,
    device_name: "Mobile",
  });
});

// --- WebSocket connect / disconnect ---
function connect(url, name) {
  stopHeartbeat();

  if (ws) {
    try { ws.close(); } catch {}
    ws = null;
  }

  currentUrl = url;
  currentPcName = name || url;
  isPaired = false;
  pairHint.classList.add("hidden");

  setConnectedMeta(currentPcName, url);
  showConnected();
  renderTiles();

  ws = new WebSocket(url);

  ws.onopen = () => {
    log("ws open");

    authToken = localStorage.getItem("fossdeck_token");

    if (authToken) {
      sendCmd({ cmd: "auth", device_id: deviceId, token: authToken });

      // If auth doesn't succeed quickly, show modal (covers cases where server is slow)
      setTimeout(() => {
        if (!isPaired && ws && ws.readyState === WebSocket.OPEN) {
          pairHint.classList.remove("hidden");
          openPairModal();
        }
      }, 600);
    } else {
      // No token => immediately prompt for pairing
      pairHint.classList.remove("hidden");
      openPairModal();
    }
  };

  ws.onmessage = (ev) => {
    let obj;
    try { obj = JSON.parse(ev.data); } catch { return; }

    if (obj.type === "hello") {
      // ok
      return;
    }

    if (obj.type === "status") {
      if (typeof obj.muted === "boolean") {
        audioState.muted = obj.muted;
      }
      if (typeof obj.volume === "number") {
        audioState.volume = obj.volume;
      }

      // Re-render tiles so mute icon updates
      renderTiles();
      return;
    }

    if (obj.type === "auth_ok") {
      isPaired = true;
      pairHint.classList.add("hidden");
      upsertRecent({ name: currentPcName, url: currentUrl });
      renderRecents();
      renderTiles();
      startHeartbeat();
      return;
    }

    if (obj.type === "auth_error") {
      // token invalid -> clear and require pairing again
      localStorage.removeItem("fossdeck_token");
      authToken = null;
      isPaired = false;
      pairHint.classList.remove("hidden");
      renderTiles();
      return;
      openPairModal();
      return;
    }

    if (obj.type === "pairing_ok") {
      if (obj.token) {
        localStorage.setItem("fossdeck_token", obj.token);
        authToken = obj.token;
      }
      isPaired = true;
      pairHint.classList.add("hidden");
      closePairModal();
      upsertRecent({ name: currentPcName, url: currentUrl });
      renderRecents();
      renderTiles();
      startHeartbeat();
      return;
    }

    if (obj.type === "pairing_error") {
      pairError.textContent = `Pairing error: ${obj.reason || "unknown"}`;
      return;
    }

    if (obj.type === "rate_limited") {
      // nice UX for your new server feature
      const sec = obj.retry_after_secs ?? 0;
      pairError.textContent = `Rate limited (${obj.reason}). Try again in ${sec}s.`;
      return;
    }

    if (obj.type === "error" && obj.message === "unauthorized") {
      isPaired = false;
      pairHint.classList.remove("hidden");
      renderTiles();
      openPairModal();
      return;
    }

    if (obj.type === "shutdown") {
      disconnect();
      return;
    }
  };

  ws.onclose = () => {
    disconnect(true);
  };

  ws.onerror = () => {
    // keep screen but show not connected
    disconnect(true);
  };
}

function disconnect(stayHome = false) {
  stopHeartbeat();

  if (ws) {
    try { ws.close(); } catch {}
    ws = null;
  }

  isPaired = false;
  currentUrl = "";
  currentPcName = "";

  closePairModal();

  if (!stayHome) {
    showHome();
  } else {
    // If it errored while connected, go back home
    showHome();
  }
}

// --- Send command ---
function sendCmd(obj) {
  if (!ws || ws.readyState !== WebSocket.OPEN) return;
  ws.send(JSON.stringify(obj));
}

// --- Render modular tiles ---
function renderTiles() {
  tileGrid.innerHTML = "";

  const layout = loadLayout();
  const used = new Set();

  // Build tiles in stored order
  for (const actionId of layout) {
    if (used.has(actionId)) continue;
    used.add(actionId);

    const action = ACTIONS[actionId];
    if (!action) continue;

    tileGrid.appendChild(makeTile(action));
  }

  // If layout references missing actions or you add new actions later,
  // you could optionally append those here.
}

function makeTile(action) {
  const el = document.createElement("div");
  el.className = "tile";
  el.dataset.actionId = action.id;

  const enabled = action.enabled();
  if (!enabled) el.classList.add("disabled");

  const iconSrc =
      typeof action.icon === "function"
          ? action.icon()
          : action.icon;

  el.innerHTML = `
    ${editMode ? `<div class="badge">drag</div>` : ``}
    <img class="tile-icon"
         src="${iconSrc}"
         alt="${escapeHtml(action.title)}"
         draggable="false" />
    <div class="tile-title">${escapeHtml(action.title)}</div>
  `;

  // tap behavior
  el.addEventListener("click", () => {
    if (editMode) return;
    if (!action.enabled()) {
      if (!isPaired) openPairModal();
      return;
    }
    action.run();
  });

  // drag & drop (only in edit mode)
  el.draggable = editMode;

  el.addEventListener("dragstart", (e) => {
    if (!editMode) return;
    el.classList.add("dragging");
    e.dataTransfer.setData("text/plain", action.id);
    e.dataTransfer.effectAllowed = "move";
  });

  el.addEventListener("dragend", () => {
    el.classList.remove("dragging");
  });

  el.addEventListener("dragover", (e) => {
    if (!editMode) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
  });

  el.addEventListener("drop", (e) => {
    if (!editMode) return;
    e.preventDefault();

    const draggedId = e.dataTransfer.getData("text/plain");
    const targetId = el.dataset.actionId;
    if (!draggedId || !targetId || draggedId === targetId) return;

    const layout = loadLayout();
    const from = layout.indexOf(draggedId);
    const to = layout.indexOf(targetId);
    if (from < 0 || to < 0) return;

    layout.splice(from, 1);
    layout.splice(to, 0, draggedId);

    saveLayout(layout);
    renderTiles();
  });

  return el;
}

// --- Init ---
renderRecents();
scanStatus.textContent = "Tap ⟳ to scan.";
