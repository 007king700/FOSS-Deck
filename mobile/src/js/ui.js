import { el } from "./dom.js";

export function showHome() {
    el.homeScreen.classList.remove("hidden");
    el.connectedScreen.classList.add("hidden");
}

export function showConnected() {
    el.homeScreen.classList.add("hidden");
    el.connectedScreen.classList.remove("hidden");
}

export function setConnectedMeta(title, sub) {
    el.connTitle.textContent = title || "Connected";
    el.connSub.textContent = sub || "";
}

export function showHomeError(msg) {
    el.scanStatus.textContent = msg;
    el.scanStatus.style.color = "rgba(255,120,120,0.95)";
    setTimeout(() => { el.scanStatus.style.color = ""; }, 4000);
}

export function escapeHtml(s) {
    return String(s).replace(/[&<>"']/g, c => ({
        "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
    }[c]));
}

export function log(msg) {
    if (!el.logEl) return;
    el.logEl.classList.remove("hidden");
    el.logEl.textContent += String(msg) + "\n";
}
