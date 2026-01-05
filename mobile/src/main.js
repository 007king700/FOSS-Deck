import { hasTauri, invoke } from "./js/tauri.js";
import { el } from "./js/dom.js";
import { state } from "./js/state.js";
import { getOrCreateDeviceId } from "./js/storage.js";
import { renderRecents, renderAvailable, normalizeDirectInputToWs } from "./js/lists.js";
import { log } from "./js/ui.js";
import { connect, disconnect, sendCmd, closePairModal, openPairModal } from "./js/ws.js";
import { renderTiles, setEditMode } from "./js/tiles.js";

// init identity
state.deviceId = getOrCreateDeviceId();

// home buttons
el.scanBtn.addEventListener("click", async () => {
  el.availableList.innerHTML = "";
  el.scanStatus.textContent = "Scanning…";

  if (!hasTauri) {
    el.scanStatus.textContent = "Scan disabled (no Tauri API)";
    return;
  }

  try {
    const hosts = await invoke("discover_hosts", { timeoutMs: 1200 });
    renderAvailable(hosts);
  } catch (e) {
    el.scanStatus.textContent = "Scan failed";
    log(`Scan error: ${e}`);
  }
});

el.directBtn.addEventListener("click", () => {
  const v = prompt("Enter IP address of the PC:", "");
  if (v === null) return;
  const url = normalizeDirectInputToWs(v);
  if (!url) return;
  connect(url, url);
});

// connected screen buttons
el.backBtn.addEventListener("click", () => disconnect());
el.editBtn.addEventListener("click", () => setEditMode(!state.editMode));

// pairing modal
el.pairCancel.addEventListener("click", () => closePairModal());
el.pairConfirm.addEventListener("click", () => {
  const code = (el.pairCodeInput.value || "").trim();
  if (!code) {
    el.pairError.textContent = "Please enter the code.";
    return;
  }
  sendCmd({ cmd: "pair", code, device_id: state.deviceId, device_name: "Mobile" });
});

// boot
renderRecents();
el.scanStatus.textContent = "Tap ⟳ to scan.";
renderTiles();
