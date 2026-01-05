const $ = (id) => document.getElementById(id);

export const el = {
    // screens
    homeScreen: $("homeScreen"),
    connectedScreen: $("connectedScreen"),

    // home
    scanBtn: $("scanBtn"),
    directBtn: $("directBtn"),
    scanStatus: $("scanStatus"),
    recentList: $("recentList"),
    availableList: $("availableList"),
    recentSub: $("recentSub"),

    // connected
    backBtn: $("backBtn"),
    editBtn: $("editBtn"),
    tileGrid: $("tileGrid"),
    connTitle: $("connTitle"),
    connSub: $("connSub"),
    pairHint: $("pairHint"),
    editHint: $("editHint"),

    // modal
    pairModal: $("pairModal"),
    pairCodeInput: $("pairCodeInput"),
    pairCancel: $("pairCancel"),
    pairConfirm: $("pairConfirm"),
    pairError: $("pairError"),

    // debug
    logEl: $("log"),
};
