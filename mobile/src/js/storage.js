const RECENTS_KEY = "fossdeck_recents_v1";
const LAYOUT_KEY = "fossdeck_layout_v1";

export const DEFAULT_LAYOUT = [
    "previous_track",
    "toggle_play_pause",
    "next_track",
    "volume_down",
    "toggle_mute",
    "volume_up",
    "toggle_mic_mute",
    "take_screenshot",
    "open_calculator",
];

export function getOrCreateDeviceId() {
    let deviceId = localStorage.getItem("fossdeck_device_id");
    if (!deviceId) {
        deviceId = crypto.randomUUID();
        localStorage.setItem("fossdeck_device_id", deviceId);
    }
    return deviceId;
}

export function loadToken() {
    return localStorage.getItem("fossdeck_token");
}
export function saveToken(token) {
    localStorage.setItem("fossdeck_token", token);
}
export function clearToken() {
    localStorage.removeItem("fossdeck_token");
}

export function loadRecents() {
    try {
        const s = localStorage.getItem(RECENTS_KEY);
        const arr = s ? JSON.parse(s) : [];
        return Array.isArray(arr) ? arr : [];
    } catch {
        return [];
    }
}

export function saveRecents(arr) {
    localStorage.setItem(RECENTS_KEY, JSON.stringify(arr));
}

export function upsertRecent({ name, url }) {
    const arr = loadRecents();
    const now = Date.now();
    const idx = arr.findIndex(x => x.url === url);
    const entry = { name: name || url, url, lastConnected: now };
    if (idx >= 0) arr[idx] = { ...arr[idx], ...entry };
    else arr.unshift(entry);
    saveRecents(arr.sort((a,b) => (b.lastConnected || 0) - (a.lastConnected || 0)).slice(0, 10));
}

export function forgetRecent(url) {
    saveRecents(loadRecents().filter(x => x.url !== url));
}

export function loadLayout() {
    let arr = null;
    try {
        const s = localStorage.getItem(LAYOUT_KEY);
        const parsed = s ? JSON.parse(s) : null;
        if (Array.isArray(parsed) && parsed.length) arr = parsed;
    } catch {}

    const layout = arr ? [...arr] : [...DEFAULT_LAYOUT];

    // migration: append missing defaults
    let changed = !arr;
    const set = new Set(layout);
    for (const id of DEFAULT_LAYOUT) {
        if (!set.has(id)) {
            layout.push(id);
            changed = true;
        }
    }
    if (changed) saveLayout(layout);
    return layout;
}

export function saveLayout(arr) {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(arr));
}
