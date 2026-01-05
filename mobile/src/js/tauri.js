const tauriCore = (globalThis.__TAURI__ && globalThis.__TAURI__.core) || null;

export const hasTauri = !!tauriCore;

export const invoke = tauriCore
    ? tauriCore.invoke
    : async () => { throw new Error("Tauri API not available"); };
