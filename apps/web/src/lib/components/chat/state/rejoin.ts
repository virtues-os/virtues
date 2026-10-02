/**
 * A rejoined turn's stream, made safe to append to what is already on screen.
 *
 * The box replays a live turn's whole event log to every reader, from its
 * first event. The AI SDK resumes INTO the last message when that message is
 * an assistant's, and adds a new part for every `text-start` and
 * `reasoning-start` without checking what is already there — so a reply that
 * was mid-stream when the wire dropped came back with every paragraph twice.
 *
 * The replay is the whole turn, so the partial copy is redundant: drop it and
 * let the replay rebuild it. Only the message the turn itself started is
 * dropped — the replay's `start` names it — never an earlier saved reply.
 *
 * Runs inside the transport's `reconnectToStream`, after the box has answered
 * 200 and before the SDK reads the last message, so a 204 touches nothing.
 */
export async function dropReplayedPartial<C extends { type: string }>(
	stream: ReadableStream<C>,
	lastMessageId: () => string | undefined,
	dropLast: () => void,
): Promise<ReadableStream<C>> {
	const reader = stream.getReader();
	const first = await reader.read();
	if (first.done) {
		reader.releaseLock();
		return emptyStream();
	}
	const chunk = first.value as C & { messageId?: string };
	if (chunk.type === "start" && chunk.messageId && chunk.messageId === lastMessageId()) {
		dropLast();
	}
	return new ReadableStream<C>({
		start(controller) {
			controller.enqueue(first.value);
		},
		async pull(controller) {
			const next = await reader.read();
			if (next.done) controller.close();
			else controller.enqueue(next.value);
		},
		cancel(reason) {
			return reader.cancel(reason);
		},
	});
}

function emptyStream<C>(): ReadableStream<C> {
	return new ReadableStream<C>({
		start(controller) {
			controller.close();
		},
	});
}
