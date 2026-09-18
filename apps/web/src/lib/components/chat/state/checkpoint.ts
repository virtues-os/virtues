/**
 * Where a compaction checkpoint goes in the transcript while a reply streams.
 *
 * The box sends `data-checkpoint` right after `start`, and `start` has
 * already pushed the assistant message for the reply. The AI SDK's next
 * write replaces the LAST message only if its id is the reply's; anything
 * else at the end makes it push a second copy of the reply and stream every
 * later token into that one. Appended at the end, the checkpoint did exactly
 * that, and dedupe (first id wins) then hid the copy that was being written:
 * every compaction turn showed an empty answer until reload.
 *
 * So the checkpoint goes BEFORE a trailing assistant message, and at the end
 * only when nothing has been pushed for the reply yet. Pure, so the SDK's
 * own write order can be tested against it (checkpoint.test.ts).
 */
export function placeCheckpoint<M extends { role: string }>(messages: M[], checkpoint: M): M[] {
	const tail = messages[messages.length - 1];
	return tail && tail.role === 'assistant'
		? [...messages.slice(0, -1), checkpoint, tail]
		: [...messages, checkpoint];
}
