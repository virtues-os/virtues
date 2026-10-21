import { describe, expect, it } from "vitest";
import { GESTURE_GAP_MS, MAC_NOTCH, sourceOf, wheelSource } from "./wheel";

const ev = (deltaY: number, t = 0, deltaX = 0, deltaMode = 0) => ({ deltaY, deltaX, deltaMode, timeStamp: t });

describe("sourceOf", () => {
	it("reads a Mac mouse notch, and an accelerated turn, as the mouse", () => {
		expect(sourceOf(ev(MAC_NOTCH))).toBe("mouse");
		expect(sourceOf(ev(-MAC_NOTCH * 3))).toBe("mouse");
	});
	it("reads Firefox's lines and Windows' 100 px notch as the mouse", () => {
		expect(sourceOf(ev(3, 0, 0, 1))).toBe("mouse");
		expect(sourceOf(ev(100))).toBe("mouse");
	});
	it("reads fractions, small deltas and any sideways part as fingers", () => {
		expect(sourceOf(ev(7.5))).toBe("trackpad");
		expect(sourceOf(ev(2))).toBe("trackpad");
		expect(sourceOf(ev(MAC_NOTCH, 0, 1))).toBe("trackpad");
	});
	it("gives no answer for an event that didn't move", () => {
		expect(sourceOf(ev(0))).toBeNull();
	});
});

describe("wheelSource", () => {
	it("keeps a swipe on the trackpad through a tail that looks like a notch", () => {
		const read = wheelSource();
		expect(read(ev(1.5, 0))).toBe("trackpad");
		expect(read(ev(MAC_NOTCH, 16))).toBe("trackpad");
	});
	it("reads again after a pause", () => {
		const read = wheelSource();
		expect(read(ev(1.5, 0))).toBe("trackpad");
		expect(read(ev(MAC_NOTCH, GESTURE_GAP_MS + 1))).toBe("mouse");
	});
});
