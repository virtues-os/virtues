import { afterEach, describe, expect, it, vi } from "vitest";
import { DAY_REWRITE_ASK_MS, editKey, getDayRewrite } from "./api";

describe("editKey", () => {
	it("tells apart two entries that share a version_number", () => {
		// A chat edit from before versions were cut after the edit: the last
		// legacy row is both the end of one entry and the start of the next,
		// read against the live page.
		const credited = { before_version: 5, version_number: 6 };
		const live = { before_version: 6, version_number: 6 };
		expect(editKey(credited)).not.toBe(editKey(live));
	});

	it("is the same for the same pair, from a feed entry or a revision", () => {
		const entry = { subject_type: "day", subject_id: "d1", before_version: 2, version_number: 3 };
		const revision = { before_version: 2, version_number: 3, author: "ai", diff: [] };
		expect(editKey(entry)).toBe(editKey(revision));
	});

	it("keys a page's first version apart from any edit", () => {
		expect(editKey({ before_version: null, version_number: 1 })).not.toBe(
			editKey({ before_version: 1, version_number: 1 }),
		);
	});
});

describe("getDayRewrite", () => {
	afterEach(() => {
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it("asks with a deadline, so an ask that never settles can't hold the next one back", async () => {
		const deadline = new AbortController();
		const timeout = vi.spyOn(AbortSignal, "timeout").mockReturnValue(deadline.signal);
		const fetch = vi.fn(async (_url: string, _init?: RequestInit) =>
			new Response(JSON.stringify({ state: "idle", has_page: true, has_your_edits: false }), {
				status: 200,
				headers: { "Content-Type": "application/json" },
			}),
		);
		vi.stubGlobal("fetch", fetch);
		await expect(getDayRewrite("2026-10-06")).resolves.toMatchObject({ state: "idle", has_page: true });
		expect(timeout).toHaveBeenCalledWith(DAY_REWRITE_ASK_MS);
		expect(fetch.mock.calls[0][1]?.signal).toBe(deadline.signal);
	});

	it("keeps its deadline where AbortSignal.timeout is missing (Safari before 16)", async () => {
		vi.useFakeTimers();
		const timeout = AbortSignal.timeout;
		Object.defineProperty(AbortSignal, "timeout", { value: undefined, configurable: true });
		try {
			const fetch = vi.fn(
				(_url: string, init?: RequestInit) =>
					new Promise<Response>((_resolve, reject) => {
						init?.signal?.addEventListener("abort", () => reject(init.signal?.reason));
					}),
			);
			vi.stubGlobal("fetch", fetch);
			const ask = getDayRewrite("2026-10-06");
			const settled = expect(ask).rejects.toMatchObject({ name: "TimeoutError" });
			await vi.advanceTimersByTimeAsync(DAY_REWRITE_ASK_MS);
			await settled;
		} finally {
			Object.defineProperty(AbortSignal, "timeout", { value: timeout, configurable: true });
			vi.useRealTimers();
		}
	});

	it("rejects once the deadline passes, as any failed ask does", async () => {
		const deadline = new AbortController();
		vi.spyOn(AbortSignal, "timeout").mockReturnValue(deadline.signal);
		const fetch = vi.fn(
			(_url: string, init?: RequestInit) =>
				new Promise<Response>((_resolve, reject) => {
					init?.signal?.addEventListener("abort", () => reject(init.signal?.reason));
				}),
		);
		vi.stubGlobal("fetch", fetch);
		const ask = getDayRewrite("2026-10-06");
		deadline.abort(new DOMException("The operation timed out.", "TimeoutError"));
		await expect(ask).rejects.toMatchObject({ name: "TimeoutError" });
	});
});
