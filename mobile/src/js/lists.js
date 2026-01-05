import { el } from "./dom.js";
import { escapeHtml } from "./ui.js";
import { loadRecents, forgetRecent } from "./storage.js";
import { connect } from "./ws.js";

export function renderRecents() {
    const recents = loadRecents();
    el.recentList.innerHTML = "";

    if (!recents.length) {
        el.recentSub.textContent = "No saved PCs yet";
        return;
    }

    el.recentSub.textContent = `${recents.length} saved`;

    for (const pc of recents) {
        const row = document.createElement("div");
        row.className = "list-item";
        row.innerHTML = `
      <div class="left">
        <div class="name">${escapeHtml(pc.name || "PC")}</div>
      </div>
      <div class="row-actions">
        <button class="btn small primary" data-act="connect">Connect</button>
        <button class="btn small secondary" data-act="forget">Forget</button>
      </div>
    `;

        row.querySelector('[data-act="connect"]').addEventListener("click", () => connect(pc.url, pc.name || ""));
        row.querySelector('[data-act="forget"]').addEventListener("click", () => {
            forgetRecent(pc.url);
            renderRecents();
        });

        el.recentList.appendChild(row);
    }
}

export function renderAvailable(hosts) {
    el.availableList.innerHTML = "";

    if (!hosts || !hosts.length) {
        el.scanStatus.textContent = "No PCs found.";
        return;
    }

    el.scanStatus.textContent = `Found ${hosts.length} host(s)`;

    for (const h of hosts) {
        const url = buildWsUrlFromHost(h);
        const name = h.name || "PC";

        const row = document.createElement("div");
        row.className = "list-item";
        row.innerHTML = `
      <div class="left">
        <div class="name">${escapeHtml(name)}</div>
      </div>
      <div class="row-actions">
        <button class="btn small primary">Connect</button>
      </div>
    `;

        row.querySelector("button").addEventListener("click", () => connect(url, name));
        el.availableList.appendChild(row);
    }
}

export function buildWsUrlFromHost(h) {
    const path = h.path || "/ws";
    return `ws://${h.ip}:${h.port}${path}`;
}

export function normalizeDirectInputToWs(input) {
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
