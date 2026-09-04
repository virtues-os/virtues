/**
 * Inline AI completion client.
 *
 * Consumes the bespoke SSE stream from POST /api/ai/complete and yields prose
 * text deltas. Minimal protocol (see virtues-core/src/api/ai_complete.rs):
 *   data: {"type":"delta","text":"…"}
 *   data: {"type":"done"}
 *   data: {"type":"error","message":"…"}
 *
 * Cancellation: pass an AbortSignal; aborting it drops the fetch, which tears
 * down the server stream.
 */

export type AiIntent = "rewrite" | "continue" | "generate";

export interface AiCompleteRequest {
	/** Omitted unless the person picked one — the box resolves the slot. */
	model?: string;
	intent: AiIntent;
	instruction: string;
	selection?: string;
	context_before?: string;
	context_after?: string;
	page_title?: string;
}

/**
 * Stream prose completions as an async generator of text chunks.
 * Throws on network/server error or if aborted (AbortError).
 */
export async function* streamCompletion(
	req: AiCompleteRequest,
	signal: AbortSignal,
): AsyncGenerator<string> {
	const resp = await fetch("/api/ai/complete", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(req),
		signal,
	});

	if (!resp.ok || !resp.body) {
		throw new Error(`AI request failed (${resp.status})`);
	}

	const reader = resp.body.getReader();
	const decoder = new TextDecoder();
	let buffer = "";

	try {
		while (true) {
			const { done, value } = await reader.read();
			if (done) break;
			buffer += decoder.decode(value, { stream: true });

			let newline: number;
			while ((newline = buffer.indexOf("\n")) >= 0) {
				const line = buffer.slice(0, newline).trim();
				buffer = buffer.slice(newline + 1);

				// SSE data lines only; skip keep-alive comments (":") and blanks.
				if (!line.startsWith("data:")) continue;
				const data = line.slice(5).trim();
				if (!data) continue;

				let event: { type?: string; text?: string; message?: string };
				try {
					event = JSON.parse(data);
				} catch {
					continue;
				}

				if (event.type === "delta" && typeof event.text === "string") {
					yield event.text;
				} else if (event.type === "error") {
					throw new Error(event.message || "AI error");
				} else if (event.type === "done") {
					return;
				}
			}
		}
	} finally {
		reader.cancel().catch(() => {});
	}
}
