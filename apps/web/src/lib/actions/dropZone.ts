/**
 * `use:dropZone` — make an element take the drags it accepts.
 *
 * A zone claims an accepted drag by cancelling its `dragover` and choosing a
 * `dropEffect`, and leaves every other drag alone, so the app-wide listener in
 * the app layout can refuse it. It reports through `onActive` whether it is
 * the place the drag will land, which is when it shows the drop tint
 * (agents/build/design.md, "Drag and drop").
 *
 * Whether the pointer is over a zone is not read from `dragleave`. WebKit
 * reports its `relatedTarget` as null, and the browser fires a leave every
 * time the pointer crosses into a child, so a zone that clears on leave
 * flickers as you move across it. Instead one window listener follows the
 * element under the pointer: every `dragenter` names it, and a `dragleave`
 * from that same element, with no `dragenter` before it, means the pointer
 * left the window or the drag was cancelled. Crossing a child does not look
 * like that, because the browser fires the new element's `dragenter` before
 * the old one's `dragleave`. A zone is active while that element is inside it.
 *
 * Every zone clears on a drop anywhere, on `dragend`, when the pointer leaves
 * the window, and when the zone unmounts.
 */

import type { Action } from "svelte/action";

export interface DropZoneParams {
	/**
	 * Whether this zone takes the drag. During a drag only `types` can be
	 * read, so check the kind of data, not its contents.
	 */
	accepts: (data: DataTransfer) => boolean;
	/**
	 * The drop of an accepted drag. Not called when something inside the zone
	 * took the drop first, by cancelling it (the chat composer does this).
	 */
	onDrop: (event: DragEvent) => void;
	/** Called when the zone starts and stops being where the drag will land. */
	onActive?: (active: boolean) => void;
	/** The cursor for an accepted drag. Defaults to "copy". */
	effect?: (data: DataTransfer) => DataTransfer["dropEffect"];
}

/** Whether a drag carries files from outside the page. */
export function hasFiles(data: DataTransfer): boolean {
	return data.types.includes("Files");
}

interface Zone {
	node: HTMLElement;
	params: DropZoneParams;
	active: boolean;
}

const zones = new Set<Zone>();
/** The element under the pointer during a drag, or null when there is none. */
let current: Node | null = null;

function setActive(zone: Zone, active: boolean) {
	if (zone.active === active) return;
	zone.active = active;
	zone.params.onActive?.(active);
}

function settle(data: DataTransfer | null) {
	for (const zone of zones) {
		const over = !!data && !!current && zone.node.contains(current);
		setActive(zone, over && zone.params.accepts(data!));
	}
}

function trackEnter(e: DragEvent) {
	current = e.target as Node | null;
	settle(e.dataTransfer);
}

// A `dragover` is always fired at the element under the pointer, so it also
// corrects `current` if an enter was missed.
function trackOver(e: DragEvent) {
	if (e.target === current) return;
	current = e.target as Node | null;
	settle(e.dataTransfer);
}

function trackLeave(e: DragEvent) {
	if (e.target !== current) return;
	current = null;
	settle(null);
}

function trackEnd() {
	current = null;
	settle(null);
}

const TRACKERS: [string, (e: DragEvent) => void][] = [
	["dragenter", trackEnter],
	["dragover", trackOver],
	["dragleave", trackLeave],
	["drop", trackEnd],
	["dragend", trackEnd],
];

function register(zone: Zone) {
	if (zones.size === 0) {
		for (const [type, fn] of TRACKERS) window.addEventListener(type, fn as EventListener, true);
	}
	zones.add(zone);
}

function unregister(zone: Zone) {
	zones.delete(zone);
	setActive(zone, false);
	if (zones.size === 0) {
		for (const [type, fn] of TRACKERS) window.removeEventListener(type, fn as EventListener, true);
		current = null;
	}
}

export const dropZone: Action<HTMLElement, DropZoneParams> = (node, params) => {
	const zone: Zone = { node, params, active: false };

	function claim(e: DragEvent) {
		const data = e.dataTransfer;
		if (!data || !zone.params.accepts(data)) return;
		e.preventDefault();
		data.dropEffect = zone.params.effect?.(data) ?? "copy";
	}

	function drop(e: DragEvent) {
		const data = e.dataTransfer;
		if (!data || !zone.params.accepts(data)) return;
		const taken = e.defaultPrevented;
		// Cancelled either way, so the browser never opens a dropped file.
		e.preventDefault();
		if (!taken) zone.params.onDrop(e);
	}

	node.addEventListener("dragenter", claim);
	node.addEventListener("dragover", claim);
	node.addEventListener("drop", drop);
	register(zone);

	return {
		update(next) {
			zone.params = next;
		},
		destroy() {
			node.removeEventListener("dragenter", claim);
			node.removeEventListener("dragover", claim);
			node.removeEventListener("drop", drop);
			unregister(zone);
		},
	};
};
