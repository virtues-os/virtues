/**
 * Whether the server still knows a view's access token.
 *
 * The server keeps these tokens in memory (`server/faces.rs`), so a restart
 * forgets every one, and each query a view left open makes after that is
 * refused. `FaceFrame` asks this while a view is showing and loads the view
 * again with a new token when the answer is yes.
 *
 * The question is an empty query on the view's own data route. The server
 * checks the token before it reads the SQL: it answers 401 for a token it
 * doesn't know, and refuses the empty SQL of one it does with 400. An empty
 * query is not kept in the list of what the view has asked.
 */
import { ApiError, deadline, request } from '$lib/api/client';

/**
 * How long one check may take. A request sent as the server goes down can
 * stay open with no answer, and the frame makes one check at a time, so a
 * check with no limit would be the last one it ever made.
 */
const CHECK_DEADLINE_MS = 10_000;

export async function tokenForgotten(token: string): Promise<boolean> {
	try {
		await request('/face/query', {
			method: 'POST',
			query: { vt: token },
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ sql: '' }),
			signal: deadline(CHECK_DEADLINE_MS),
		});
		return false;
	} catch (e) {
		// Anything else is a server that is away, busy or silent. The next check asks again.
		return e instanceof ApiError && e.status === 401;
	}
}
