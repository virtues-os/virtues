/**
 * A compaction checkpoint arrives mid-stream, right after `start`. Where the
 * client puts it decides whether the reply is visible: the SDK replaces the
 * LAST message only when its id is the reply's, so a checkpoint appended at
 * the end makes it push a second copy and stream into that one — the bug
 * every compaction turn had until 2026-09-18. This drives the SDK's own
 * `AbstractChat` write path with a plain state, no framework, and the box's
 * chunk order.
 */
import { describe, expect, it } from 'vitest';
import { AbstractChat, DefaultChatTransport, type ChatState, type UIMessage } from 'ai';
import { placeCheckpoint } from './checkpoint';

// Stores a copy on every write, as the app's store does (chatInstances keeps
// a decoupled snapshot so Svelte re-renders only what changed). That copy is
// what makes the bug visible: a message pushed and never replaced again stays
// exactly as it was when pushed.
class PlainState implements ChatState<UIMessage> {
	status: ChatState<UIMessage>['status'] = 'ready';
	error: Error | undefined = undefined;
	messages: UIMessage[] = [];
	pushMessage = (m: UIMessage) => {
		this.messages = [...this.messages, structuredClone(m)];
	};
	popMessage = () => {
		this.messages = this.messages.slice(0, -1);
	};
	replaceMessage = (i: number, m: UIMessage) => {
		this.messages = [...this.messages.slice(0, i), structuredClone(m), ...this.messages.slice(i + 1)];
	};
	snapshot = <T>(t: T): T => structuredClone(t);
}

class PlainChat extends AbstractChat<UIMessage> {}

// The box's order for a turn that compacted first: start (with the reply's
// id), the checkpoint, then the reply's text.
const CHUNKS = [
	{ type: 'start', messageId: 'msg_reply' },
	{
		type: 'data-checkpoint',
		id: 'ckpt_1',
		data: { version: 1, messagesSummarized: 12, summary: 'Earlier: …', timestamp: 't' },
	},
	{ type: 'start-step' },
	{ type: 'text-start', id: 'msg_reply:t0' },
	{ type: 'text-delta', id: 'msg_reply:t0', delta: 'hello ' },
	{ type: 'text-delta', id: 'msg_reply:t0', delta: 'back' },
	{ type: 'text-end', id: 'msg_reply:t0' },
	{ type: 'finish-step' },
	{ type: 'finish', finishReason: 'stop' },
];

function body(): string {
	return CHUNKS.map((c) => `data: ${JSON.stringify(c)}\n\n`).join('') + 'data: [DONE]\n\n';
}

function makeChat(place: (messages: UIMessage[], checkpoint: UIMessage) => UIMessage[]) {
	const state = new PlainState();
	const chat = new PlainChat({
		id: 'chat_1',
		state,
		generateId: () => 'local',
		transport: new DefaultChatTransport({
			api: '/api/chat',
			fetch: async () =>
				new Response(body(), {
					status: 200,
					headers: { 'Content-Type': 'text/event-stream', 'x-vercel-ai-ui-message-stream': 'v1' },
				}),
		}),
		onData: (part) => {
			if (part.type !== 'data-checkpoint') return;
			const checkpoint = { id: part.id!, role: 'checkpoint', parts: [] } as unknown as UIMessage;
			chat.messages = place(chat.messages, checkpoint);
		},
	});
	return chat;
}

function textOf(m: UIMessage): string {
	return m.parts
		.filter((p): p is Extract<typeof p, { type: 'text' }> => p.type === 'text')
		.map((p) => p.text)
		.join('');
}

describe('a checkpoint that arrives while the reply streams', () => {
	it('placed before the reply, the reply stays the last message and keeps its text', async () => {
		const chat = makeChat(placeCheckpoint);
		await chat.sendMessage({ text: 'hi' });

		expect(chat.messages.map((m) => m.role)).toEqual(['user', 'checkpoint', 'assistant']);
		expect(textOf(chat.messages[2])).toBe('hello back');
	});

	it('appended after the reply, the SDK pushes a second reply and streams into that one', async () => {
		const chat = makeChat((messages, checkpoint) => [...messages, checkpoint]);
		await chat.sendMessage({ text: 'hi' });

		// The shape of the bug: two assistant messages with the same id, the
		// first (the one a first-wins dedupe keeps) empty.
		const assistants = chat.messages.filter((m) => m.role === 'assistant');
		expect(assistants).toHaveLength(2);
		expect(assistants[0].id).toBe(assistants[1].id);
		expect(textOf(assistants[0])).toBe('');
		expect(textOf(assistants[1])).toBe('hello back');
	});
});
