// A stand-in for virtues-api's model endpoint, so the lab can stream a reply
// without asking a model: the same reply, at the same pace, every time.
//
// `POST /v1/ai/chat/completions` answers a streaming request with the reply
// below in the OpenAI server-sent-events shape the box reads
// (virtues-core/src/agent/stream.rs): one `choices[0].delta.content` per
// event, then a `finish_reason`. A request that does not stream (a title, a
// summary) gets a short complete answer. Everything else is a 404, which the
// box treats as the gateway being away and falls back from (the model list
// keeps its compiled copy).
//
// The reply is invented for this file, and long enough to exercise what makes
// streaming expensive to render: headings, lists, a table and a code block
// growing token by token at the end of a long chat.

import { createServer } from 'node:http';

export const TOKEN_INTERVAL_MS = 15;

const REPLY = [
	'## A plan for the weekend\n\n',
	'Here is one way to fit the garden, the bicycle and the book club into two days. ',
	'Nick can start early, since the seedlings need the cool morning, and David Okafor ',
	'has the chain tool, so the bicycle can wait for the afternoon.\n\n',
	'- Saturday morning: move the trellis and water the tomato seedlings.\n',
	'- Saturday afternoon: replace the bicycle chain and check the brakes.\n',
	'- Sunday: finish the book club chapter, then sort two attic boxes.\n\n',
	'| Task | Who | Time |\n|---|---|---|\n',
	'| Trellis | Nick | 1 hour |\n| Chain | David Okafor | 2 hours |\n| Chapter | Nick | 3 hours |\n| Boxes | both | 1 hour |\n\n',
	'If the rain comes back, the picnic moves indoors and the attic boxes come first. ',
	'A small script can keep the list in order:\n\n',
	'```ts\nexport function order(tasks: { name: string; hours: number }[]) {\n',
	'\treturn [...tasks].sort((a, b) => a.hours - b.hours).map((t) => t.name);\n}\n```\n\n',
	'The list is shorter than it looks, and the rest can wait until next weekend.'
].join('');

/** The reply cut into tokens of about four characters, as a model streams. */
export const TOKENS = REPLY.match(/[\s\S]{1,4}/g);
export const REPLY_TEXT = REPLY;

export function startFakeModel() {
	const server = createServer((req, res) => {
		if (req.method !== 'POST' || !req.url.startsWith('/v1/ai/chat/completions')) {
			res.writeHead(404).end();
			return;
		}
		let body = '';
		req.on('data', (c) => (body += c));
		req.on('end', () => {
			let stream = false;
			try {
				stream = JSON.parse(body).stream === true;
			} catch {
				/* not JSON: answer as a plain completion */
			}
			if (!stream) {
				res.writeHead(200, { 'Content-Type': 'application/json' });
				res.end(JSON.stringify({ choices: [{ message: { role: 'assistant', content: 'Weekend plan' }, finish_reason: 'stop' }], usage: { prompt_tokens: 1, completion_tokens: 1 } }));
				return;
			}
			res.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' });
			let i = 0;
			const timer = setInterval(() => {
				if (i < TOKENS.length) {
					res.write(`data: ${JSON.stringify({ choices: [{ index: 0, delta: { content: TOKENS[i++] } }] })}\n\n`);
					return;
				}
				clearInterval(timer);
				res.write(`data: ${JSON.stringify({ choices: [{ index: 0, delta: {}, finish_reason: 'stop' }], usage: { prompt_tokens: 1, completion_tokens: TOKENS.length } })}\n\n`);
				res.end('data: [DONE]\n\n');
			}, TOKEN_INTERVAL_MS);
			// The response's close, not the request's: a request closes as soon
			// as its body is read, which stopped every stream before it began.
			res.on('close', () => clearInterval(timer));
		});
	});
	return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ server, url: `http://127.0.0.1:${server.address().port}` })));
}
