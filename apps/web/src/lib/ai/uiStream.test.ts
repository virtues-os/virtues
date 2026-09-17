/**
 * The box's UI-message stream, read by the AI SDK's own transport.
 *
 * The fixtures under ./fixtures are written by the Rust side
 * (`virtues-core/src/api/chat.rs`, `ui_stream_fixture` tests) from the same
 * `serialize_event` that serves a real turn, so what this test parses is
 * what a browser receives. The Rust test refuses to pass when the fixture is
 * stale, and this test refuses to pass when the SDK will not parse it. One
 * gate, two sides: change an event shape on the box and both sides say so.
 *
 * Parsing goes through `DefaultChatTransport` with a stubbed `fetch`, not a
 * hand-rolled reader, because the transport is where the SDK validates each
 * chunk against its schema. A chunk the schema rejects throws here exactly as
 * it would in the app.
 */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { DefaultChatTransport, readUIMessageStream, type UIMessage, type UIMessageChunk } from 'ai';

const here = dirname(fileURLToPath(import.meta.url));

function fixtureBody(name: string): string {
	const lines = readFileSync(join(here, 'fixtures', name), 'utf8')
		.split('\n')
		.filter((l) => l.trim().length > 0);
	return lines.map((l) => `data: ${l}\n\n`).join('') + 'data: [DONE]\n\n';
}

async function lastMessageOf(name: string): Promise<UIMessage> {
	const transport = new DefaultChatTransport({
		api: '/api/chat',
		fetch: async () =>
			new Response(fixtureBody(name), {
				status: 200,
				headers: {
					'Content-Type': 'text/event-stream',
					'x-vercel-ai-ui-message-stream': 'v1'
				}
			})
	});
	const stream = (await transport.sendMessages({
		chatId: 'chat_fixture',
		messageId: undefined,
		messages: [],
		abortSignal: undefined,
		trigger: 'submit-message'
	})) as ReadableStream<UIMessageChunk>;

	let last: UIMessage | undefined;
	for await (const message of readUIMessageStream({ stream })) {
		last = message;
	}
	if (!last) throw new Error('the stream produced no message');
	return last;
}

describe('the box speaks the UI message stream protocol', () => {
	it('a two-step turn with a tool result and a tool error parses whole', async () => {
		const message = await lastMessageOf('box-ui-stream.jsonl');
		const types = message.parts.map((p) => p.type);

		// Two steps, each opened by a step-start the box now emits.
		expect(types.filter((t) => t === 'step-start')).toHaveLength(2);

		// One text part per step: the SDK forgets open parts at finish-step.
		const text = message.parts.filter((p) => p.type === 'text') as { text: string }[];
		expect(text.map((t) => t.text)).toEqual(['Hello', ' world']);

		// The tool that succeeded and the one that failed, each in its own state.
		const search = message.parts.find((p) => p.type === 'tool-web_search') as
			| { state: string; output?: unknown }
			| undefined;
		expect(search?.state).toBe('output-available');
		const page = message.parts.find((p) => p.type === 'tool-create_page') as
			| { state: string; errorText?: string }
			| undefined;
		expect(page?.state).toBe('output-error');
		expect(page?.errorText).toBe('the page could not be written');

		// Reasoning arrived as its own part.
		expect(types).toContain('reasoning');
	});

	/**
	 * The rule the transcript and the thinking block split on, checked against
	 * what the SDK actually produces rather than against a hand-made array.
	 *
	 * A run of text with a tool call AFTER it was the model saying what it was
	 * about to do; the run with nothing after it is the reply. The canonical
	 * turn deliberately ENDS on a failed tool, so it has no reply at all — which
	 * is the case that would otherwise render a blank assistant message, with
	 * every word the model wrote hidden inside a collapsed block.
	 */
	it('text runs divide into narration and reply by the tool call after them', async () => {
		const message = await lastMessageOf('box-ui-stream.jsonl');
		const parts = message.parts as { type: string; text?: string }[];

		const lastToolIndex = parts.reduce(
			(last, p, i) => (p.type.startsWith('tool-') ? i : last),
			-1
		);
		const narration = parts
			.filter((p, i) => p.type === 'text' && p.text?.trim() && i < lastToolIndex)
			.map((p) => p.text!.trim());
		const reply = parts
			.filter((p, i) => p.type === 'text' && p.text?.trim() && i > lastToolIndex)
			.map((p) => p.text!.trim());

		expect(narration).toEqual(['Hello', 'world']);
		expect(reply).toEqual([]);
	});

	it('a stopped turn ends with abort and keeps what streamed', async () => {
		const message = await lastMessageOf('box-ui-stream-abort.jsonl');
		const text = message.parts.find((p) => p.type === 'text') as { text: string } | undefined;
		expect(text?.text).toBe('Partial');
	});
});
