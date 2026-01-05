export const state = {
    // connection
    ws: null,
    currentUrl: "",
    currentPcName: "",
    isPaired: false,
    heartbeatTimer: null,
    disconnectInProgress: false,

    // auth / identity
    deviceId: null,
    authToken: null,

    // audio state mirrored from PC
    audio: {
        muted: false,
        volume: 1.0,
        playing: true,
        micMuted: false,
    },

    // ui state
    editMode: false,
    suppressClickUntil: 0,

    // reorder state
    reorder: {
        draggingId: null,
        startX: 0,
        startY: 0,
        moved: false,
        targetEl: null,
    },
};
