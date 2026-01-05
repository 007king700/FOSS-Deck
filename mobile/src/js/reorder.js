import { state } from "./state.js";

export function wireTileReorder({ tile, prefersCoarsePointer, getEditMode, onReorder }) {
    // Desktop drag/drop
    tile.addEventListener("dragstart", (e) => {
        if (!getEditMode() || prefersCoarsePointer) return;
        tile.classList.add("dragging");
        e.dataTransfer.setData("text/plain", tile.dataset.actionId);
        e.dataTransfer.effectAllowed = "move";
    });

    tile.addEventListener("dragend", () => tile.classList.remove("dragging"));

    tile.addEventListener("dragover", (e) => {
        if (!getEditMode() || prefersCoarsePointer) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
    });

    tile.addEventListener("drop", (e) => {
        if (!getEditMode() || prefersCoarsePointer) return;
        e.preventDefault();
        const draggedId = e.dataTransfer.getData("text/plain");
        const targetId = tile.dataset.actionId;
        if (!draggedId || !targetId || draggedId === targetId) return;
        onReorder({ draggedId, targetId });
    });

    // Mobile pointer reorder
    function clearTarget() {
        if (state.reorder.targetEl) {
            state.reorder.targetEl.classList.remove("reorder-target");
            state.reorder.targetEl = null;
        }
    }

    function finish() {
        tile.classList.remove("dragging");
        tile.style.pointerEvents = "";
        clearTarget();
        state.reorder.draggingId = null;
        state.reorder.moved = false;
    }

    tile.addEventListener("pointerdown", (e) => {
        if (!getEditMode() || !prefersCoarsePointer) return;
        e.preventDefault();

        state.reorder.draggingId = tile.dataset.actionId;
        state.reorder.startX = e.clientX;
        state.reorder.startY = e.clientY;
        state.reorder.moved = false;

        clearTarget();
        tile.classList.add("dragging");
        tile.style.pointerEvents = "none";
        try { tile.setPointerCapture(e.pointerId); } catch {}
    });

    tile.addEventListener("pointermove", (e) => {
        if (!getEditMode() || !prefersCoarsePointer) return;
        if (!state.reorder.draggingId) return;

        const dx = Math.abs(e.clientX - state.reorder.startX);
        const dy = Math.abs(e.clientY - state.reorder.startY);
        if (dx + dy > 6) state.reorder.moved = true;

        const under = document.elementFromPoint(e.clientX, e.clientY);
        const other = under?.closest?.(".tile");
        if (!other || other === tile) {
            clearTarget();
            return;
        }

        if (state.reorder.targetEl !== other) {
            clearTarget();
            state.reorder.targetEl = other;
            state.reorder.targetEl.classList.add("reorder-target");
        }
    });

    tile.addEventListener("pointerup", () => {
        if (!getEditMode() || !prefersCoarsePointer) return;
        if (!state.reorder.draggingId) return;

        const draggedId = state.reorder.draggingId;
        const targetId = state.reorder.targetEl?.dataset?.actionId;

        if (state.reorder.moved) state.suppressClickUntil = Date.now() + 350;

        if (targetId && draggedId && targetId !== draggedId) {
            onReorder({ draggedId, targetId });
        }
        finish();
    });

    tile.addEventListener("pointercancel", () => {
        if (!prefersCoarsePointer) return;
        finish();
    });
}
