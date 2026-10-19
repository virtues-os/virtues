/**
 * FigureNode under the device's veil: what the day page hides of a figure,
 * the block page hides too. A quote's words go whole, as the day page's
 * `DayFigure` veils them, marked or not: a writer's quote carries no ⟦ ⟧,
 * and veiled phrase by phrase it showed in full where the day page hid it.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { ViewState } from "$lib/document/nodeviews.svelte";

vi.mock("$lib/stores/contextMenu.svelte", () => ({ contextMenu: { show() {} } }));
vi.mock("$lib/codemirror/extensions/long-press", () => ({ onContextGesture() {} }));

const { default: FigureNode } = await import("./FigureNode.svelte");
const { veil } = await import("$lib/stores/veil.svelte");

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;

/** Draw a figure with `attrs`, and let the veil wrap what it hides. */
async function show(attrs: Record<string, unknown>): Promise<void> {
	const view = {
		type: "figure",
		attrs,
		selected: false,
		editable: true,
		setAttrs() {},
		remove() {},
		replaceWith() {},
		select() {},
		text: () => "",
		editor: {},
	} as unknown as ViewState;
	mounted = mount(FigureNode, { target, props: { view } });
	flushSync();
	// The veil wraps once the node has rendered.
	await new Promise<void>((done) => queueMicrotask(done));
}

/** The text of each span the veil hides inside `selector`. */
const hidden = (selector: string) =>
	[...target.querySelectorAll<HTMLElement>(`${selector} .veil-hidden`)].map((s) => s.textContent ?? "");

beforeEach(() => {
	veil.on = true;
	veil.lifted = false;
	target = document.createElement("div");
	document.body.appendChild(target);
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	target.remove();
	veil.on = false;
});

describe("FigureNode under the veil", () => {
	it("hides a quote's words whole, as the day page does, marked or not", async () => {
		await show({ kind: "quote", text: "Save me a seat by the window.", who: "Nick", time: "9:12 AM" });
		expect(hidden("q")).toEqual(["Save me a seat by the window."]);
		expect(hidden("figcaption")).toEqual(["Nick · 9:12 AM"]);
	});

	it("hides a quote holding a marked phrase whole, its brackets gone", async () => {
		await show({ kind: "quote", text: "Meet me at ⟦the clinic⟧ at noon.", who: "You" });
		expect(hidden("q")).toEqual(["Meet me at the clinic at noon."]);
		// Attributions that name nobody stay.
		expect(hidden("figcaption")).toEqual([]);
	});

	it("shows the words with the veil off", async () => {
		veil.on = false;
		await show({ kind: "quote", text: "Save me a seat by the window.", who: "Nick" });
		expect(hidden("q")).toEqual([]);
		expect(target.querySelector("q")?.textContent).toBe("Save me a seat by the window.");
	});
});
