import { el } from "./dom.js";
import { state } from "./state.js";
import { ACTIONS } from "./actions.js";
import { loadLayout, saveLayout } from "./storage.js";
import { escapeHtml } from "./ui.js";
import { openPairModal } from "./ws.js";
import { wireTileReorder } from "./reorder.js";

const prefersCoarsePointer =
    (window.matchMedia && window.matchMedia("(pointer: coarse)").matches) ||
    ("ontouchstart" in window);

export function setEditMode(on) {
    state.editMode = !!on;
    document.body.classList.toggle("edit-mode", state.editMode);
    el.editHint.classList.toggle("hidden", !state.editMode);
    renderTiles();
}

export function renderTiles() {
    el.tileGrid.innerHTML = "";

    const layout = loadLayout();
    const used = new Set();

    for (const actionId of layout) {
        if (used.has(actionId)) continue;
        used.add(actionId);

        const action = ACTIONS[actionId];
        if (!action) continue;

        el.tileGrid.appendChild(makeTile(action));
    }
}

function makeTile(action) {
    const tile = document.createElement("div");
    tile.className = "tile";
    tile.dataset.actionId = action.id;

    if (!action.enabled()) tile.classList.add("disabled");

    const iconSrc = typeof action.icon === "function" ? action.icon() : action.icon;

    tile.innerHTML = `
    ${state.editMode ? `<div class="badge">drag</div>` : ``}
    <img class="tile-icon"
         src="${iconSrc}"
         alt="${escapeHtml(action.title)}"
         draggable="false" />
    <div class="tile-title">${escapeHtml(action.title)}</div>
  `;

    tile.addEventListener("click", () => {
        if (Date.now() < state.suppressClickUntil) return;
        if (state.editMode) return;

        if (!action.enabled()) {
            if (!state.isPaired) openPairModal();
            return;
        }
        action.run();
    });

    // reorder wiring lives in reorder.js
    wireTileReorder({
        tile,
        prefersCoarsePointer,
        getEditMode: () => state.editMode,
        onReorder: ({ draggedId, targetId }) => {
            const layout = loadLayout();
            const from = layout.indexOf(draggedId);
            const to = layout.indexOf(targetId);
            if (from < 0 || to < 0) return;
            layout.splice(from, 1);
            layout.splice(to, 0, draggedId);
            saveLayout(layout);
            renderTiles();
        },
    });

    tile.draggable = state.editMode && !prefersCoarsePointer;
    return tile;
}
