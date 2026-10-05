import { describe, expect, it } from "vitest";
import { dropReplayedPartial } from "./rejoin";

const streamOf = (chunks: { type: string; [k: string]: unknown }[]) =>
	new ReadableStream({
		start(c) {
			for (const ch of chunks) c.enqueue(ch);
			c.close();
		},
	});

async function drain(s: ReadableStream<any>) {
	const out: any[] = [];
	const r = s.getReader();
	for (;;) {
		const { done, value } = await r.read();
		if (done) return out;
		out.push(value);
	}
}

describe("dropReplayedPartial", () => {
	const replay = [
		{ type: "start", messageId: "a1" },
		{ type: "text-start", id: "t0" },
		{ type: "text-delta", id: "t0", delta: "Hello" },
	];

	it("drops the partial copy of the turn being replayed, and passes every chunk on", async () => {
		let messages = ["u1", "a1"];
		const out = await dropReplayedPartial(
			streamOf(replay),
			() => messages.at(-1),
			() => (messages = messages.slice(0, -1)),
		);
		expect(messages).toEqual(["u1"]);
		expect(await drain(out)).toEqual(replay);
	});

	it("leaves an earlier saved reply alone", async () => {
		let messages = ["u1", "a0"];
		await dropReplayedPartial(
			streamOf(replay),
			() => messages.at(-1),
			() => (messages = messages.slice(0, -1)),
		);
		expect(messages).toEqual(["u1", "a0"]);
	});
});
