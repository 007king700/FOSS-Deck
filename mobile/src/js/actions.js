import { state } from "./state.js";
import { sendCmd } from "./ws.js";
import { renderTiles } from "./tiles.js";

export const ACTIONS = {
    toggle_mute: {
        id: "toggle_mute",
        title: "Mute",
        icon: () => state.audio.muted ? "assets/mute.svg" : "assets/unmute.svg",
        enabled: () => state.isPaired,
        run: () => {
            state.audio.muted = !state.audio.muted; // optimistic
            renderTiles();
            sendCmd({ cmd: "toggle_mute" });
        },
    },

    volume_up: {
        id: "volume_up",
        title: "Volume +",
        icon: "assets/volume_up.svg",
        enabled: () => state.isPaired,
        run: () => sendCmd({ cmd: "volume_up", delta: 0.05 }),
    },

    volume_down: {
        id: "volume_down",
        title: "Volume −",
        icon: "assets/volume_down.svg",
        enabled: () => state.isPaired,
        run: () => sendCmd({ cmd: "volume_down", delta: 0.05 }),
    },

    previous_track: {
        id: "previous_track",
        title: "Previous",
        icon: "assets/previous.png",
        enabled: () => state.isPaired,
        run: () => sendCmd({ cmd: "previous_track" }),
    },

    next_track: {
        id: "next_track",
        title: "Next",
        icon: "assets/next.png",
        enabled: () => state.isPaired,
        run: () => sendCmd({ cmd: "next_track" }),
    },

    toggle_play_pause: {
        id: "toggle_play_pause",
        title: "Play/Pause",
        icon: () => state.audio.playing ? "assets/pause.png" : "assets/resume.png",
        enabled: () => state.isPaired,
        run: () => {
            state.audio.playing = !state.audio.playing; // optimistic
            renderTiles();
            sendCmd({ cmd: "toggle_play_pause" });
        },
    },

    toggle_mic_mute: {
        id: "toggle_mic_mute",
        title: "Mic",
        icon: () => state.audio.micMuted ? "assets/mic_muted.png" : "assets/mic.png",
        enabled: () => state.isPaired,
        run: () => {
            state.audio.micMuted = !state.audio.micMuted; // optimistic
            renderTiles();
            sendCmd({ cmd: "toggle_mic_mute" });
        },
    },

    take_screenshot: {
        id: "take_screenshot",
        title: "Screenshot",
        icon: "assets/screenshot.png",
        enabled: () => state.isPaired,
        run: () => sendCmd({ cmd: "take_screenshot" }),
    },

    open_calculator: {
        id: "open_calculator",
        title: "Calculator",
        icon: "assets/calculator.png",
        enabled: () => state.isPaired,
        run: () => sendCmd({ cmd: "open_calculator" }),
    },
};
