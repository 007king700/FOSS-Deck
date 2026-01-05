import { state } from "./state.js";
import { el } from "./dom.js";
import { showHome, showConnected, showHomeError, setConnectedMeta } from "./ui.js";
import { loadToken, saveToken, clearToken, upsertRecent } from "./storage.js";
import { renderRecents } from "./lists.js";
import { renderTiles } from "./tiles.js";

export function sendCmd(obj) {
    if (!state.ws || state.ws.readyState !== WebSocket.OPEN) return;
    state.ws.send(JSON.stringify(obj));
}

export function stopHeartbeat() {
    if (state.heartbeatTimer) {
        clearInterval(state.heartbeatTimer);
        state.heartbeatTimer = null;
    }
}

export function startHeartbeat() {
    stopHeartbeat();
    sendCmd({ cmd: "get_status" });
    state.heartbeatTimer = setInterval(() => {
        if (state.ws && state.ws.readyState === WebSocket.OPEN && state.isPaired) {
            sendCmd({ cmd: "get_status" });
        }
    }, 5000);
}

export function disconnect() {
    if (state.disconnectInProgress) return;
    state.disconnectInProgress = true;
    stopHeartbeat();

    if (state.ws) {
        try { state.ws.close(); } catch {}
        state.ws = null;
    }

    state.isPaired = false;
    state.currentUrl = "";
    state.currentPcName = "";

    el.pairModal.classList.add("hidden");
    showHome();
}

export function connect(url, name) {
    state.disconnectInProgress = false;
    stopHeartbeat();

    if (state.ws) {
        try { state.ws.close(); } catch {}
        state.ws = null;
    }

    if (!/^wss?:\/\/.+/i.test(url)) {
        showHomeError("Invalid address. Use ws://IP:port/ws");
        return;
    }

    el.scanStatus.textContent = `Connecting to ${name || "PC"}…`;
    el.scanStatus.style.color = "";

    state.currentUrl = url;
    state.currentPcName = name || url;
    state.isPaired = false;

    state.ws = new WebSocket(url);

    const connectTimeout = setTimeout(() => {
        if (state.ws && state.ws.readyState !== WebSocket.OPEN) {
            try { state.ws.close(); } catch {}
            state.ws = null;
            showHomeError("Could not connect. PC might be offline or the address is wrong.");
        }
    }, 3500);

    state.ws.onopen = () => {
        clearTimeout(connectTimeout);

        setConnectedMeta(state.currentPcName, "");
        showConnected();
        renderTiles();

        state.authToken = loadToken();
        if (state.authToken) {
            sendCmd({ cmd: "auth", device_id: state.deviceId, token: state.authToken });

            setTimeout(() => {
                if (!state.isPaired && state.ws && state.ws.readyState === WebSocket.OPEN) {
                    el.pairHint.classList.remove("hidden");
                    openPairModal();
                }
            }, 600);
        } else {
            el.pairHint.classList.remove("hidden");
            openPairModal();
        }
    };

    state.ws.onmessage = (ev) => {
        let obj;
        try { obj = JSON.parse(ev.data); } catch { return; }

        if (obj.type === "hello") return;

        if (obj.type === "status") {
            if (typeof obj.muted === "boolean") state.audio.muted = obj.muted;
            if (typeof obj.volume === "number") state.audio.volume = obj.volume;
            if (typeof obj.mic_muted === "boolean") state.audio.micMuted = obj.mic_muted;
            renderTiles();
            return;
        }

        if (obj.type === "auth_ok") {
            state.isPaired = true;
            el.pairHint.classList.add("hidden");
            upsertRecent({ name: state.currentPcName, url: state.currentUrl });
            renderRecents();
            renderTiles();
            startHeartbeat();
            return;
        }

        if (obj.type === "auth_error") {
            clearToken();
            state.authToken = null;
            state.isPaired = false;
            el.pairHint.classList.remove("hidden");
            renderTiles();
            openPairModal();
            return;
        }

        if (obj.type === "pairing_ok") {
            if (obj.token) {
                saveToken(obj.token);
                state.authToken = obj.token;
            }
            state.isPaired = true;
            el.pairHint.classList.add("hidden");
            closePairModal();
            upsertRecent({ name: state.currentPcName, url: state.currentUrl });
            renderRecents();
            renderTiles();
            startHeartbeat();
            return;
        }

        if (obj.type === "pairing_error") {
            el.pairError.textContent = `Pairing error: ${obj.reason || "unknown"}`;
            return;
        }

        if (obj.type === "rate_limited") {
            const sec = obj.retry_after_secs ?? 0;
            el.pairError.textContent = `Rate limited (${obj.reason}). Try again in ${sec}s.`;
            return;
        }

        if (obj.type === "error" && obj.message === "unauthorized") {
            state.isPaired = false;
            el.pairHint.classList.remove("hidden");
            renderTiles();
            openPairModal();
            return;
        }

        if (obj.type === "shutdown") {
            disconnect();
            showHomeError("Server shut down.");
            return;
        }
    };

    state.ws.onerror = () => {
        clearTimeout(connectTimeout);
        if (el.connectedScreen.classList.contains("hidden")) {
            showHomeError("Connection failed. PC offline or invalid address.");
        }
        disconnect();
    };

    state.ws.onclose = () => {
        clearTimeout(connectTimeout);
        if (el.connectedScreen.classList.contains("hidden")) {
            showHomeError("Could not connect. PC offline or invalid address.");
            return;
        }
        disconnect();
    };
}

export function openPairModal() {
    el.pairError.textContent = "";
    el.pairCodeInput.value = "";
    el.pairModal.classList.remove("hidden");
    setTimeout(() => el.pairCodeInput.focus(), 50);
}

export function closePairModal() {
    el.pairModal.classList.add("hidden");
}
