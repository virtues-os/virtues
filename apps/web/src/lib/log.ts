/**
 * Client-side logging that can reach the box.
 *
 * # Why
 *
 * This app had 135 bare `console.*` calls and no way to tell the box about any
 * of them. A phone that failed to pair, a view that threw on load, a stream
 * that died after the server thought it had answered — all of it lived in a
 * devtools console nobody was looking at, on a device that is usually a phone.
 * The box's journal was a complete account of the box and a blank page about
 * every client talking to it.
 *
 * `warn` and above are queued and posted to `POST /api/events`, where they
 * become lines in the box's own journal, stamped with this device. The console
 * still gets everything, unchanged — this adds a destination, it does not
 * replace the one you use while developing.
 *
 * # Rules this file lives by
 *
 * 1. **Never throw.** A logger that can break the thing it is logging about is
 *    worse than no logger. Every path here is wrapped.
 * 2. **Never recurse.** Reporting a failure of the reporter is how you get an
 *    infinite loop that is also a network flood. So `flush` swallows its own
 *    errors and logs nothing through this module — the only reason it is safe
 *    to call from an error handler.
 * 3. **Never block.** Nothing here is awaited by application code.
 * 4. **Bounded.** The queue has a cap and drops oldest-first. A tab left open
 *    for a week while offline must not grow without limit.
 */

type Severity = 'info' | 'warn' | 'error';

interface QueuedEvent {
	kind: string;
	severity: Severity;
	message: string;
	detail?: unknown;
	occurred_at: string;
}

/**
 * Queue cap. Past this we drop the OLDEST, because in a failure cascade the
 * first events are the cause and the rest are consequences — but we keep a
 * count, so the box is told how many were lost rather than being handed a
 * quietly incomplete picture.
 */
const MAX_QUEUE = 100;

/** How often a non-empty queue is flushed, in ms. */
const FLUSH_INTERVAL = 10_000;

/** Matches the server's `MAX_BATCH` in virtues-core/src/api/events.rs. */
const MAX_BATCH = 50;

let queue: QueuedEvent[] = [];
let dropped = 0;
let inFlight = false;
let timer: ReturnType<typeof setInterval> | null = null;
/**
 * Set when the box tells us it has no door. See the 404 branch in `flush`.
 */
let standDown = false;

function nowIso(): string {
	try {
		return new Date().toISOString();
	} catch {
		return '';
	}
}

/**
 * Serialize whatever a caller passed as detail into something JSON-safe.
 *
 * `Error` needs special handling: it has no enumerable own properties, so
 * `JSON.stringify(new Error('boom'))` is `{}` — the single most common way a
 * client error arrives at a server as an empty object.
 */
function safeDetail(detail: unknown): unknown {
	try {
		if (detail instanceof Error) {
			return { name: detail.name, message: detail.message, stack: detail.stack };
		}
		if (detail && typeof detail === 'object') {
			// Round-trip to drop functions, symbols, and anything cyclic. If it
			// cannot survive that, it cannot survive the wire either.
			return JSON.parse(JSON.stringify(detail));
		}
		return detail;
	} catch {
		return String(detail);
	}
}

function enqueue(severity: Severity, kind: string, message: string, detail?: unknown): void {
	try {
		queue.push({
			kind,
			severity,
			message: String(message ?? '').slice(0, 512),
			detail: detail === undefined ? undefined : safeDetail(detail),
			occurred_at: nowIso()
		});
		while (queue.length > MAX_QUEUE) {
			queue.shift();
			dropped += 1;
		}
		ensureTimer();
	} catch {
		/* rule 1 */
	}
}

function ensureTimer(): void {
	if (standDown || timer !== null || typeof setInterval !== 'function') return;
	timer = setInterval(() => {
		void flush();
	}, FLUSH_INTERVAL);
}

/**
 * Post what is queued. Never rejects.
 *
 * `keepalive` lets the request outlive the page, which is the whole point when
 * this is called from `visibilitychange` — a plain fetch is cancelled when the
 * tab goes away, losing exactly the events that explain why someone left.
 * Its 64 KB body cap is deliberately mirrored by the route's own body limit.
 *
 * Not `navigator.sendBeacon`, which looks made for this: on mobile the app is
 * a bundled SPA at a `tauri://` origin and reaches the box through a global
 * `window.fetch` proxy (see `lib/config/backend.ts`). sendBeacon does not go
 * through that proxy, so it would post to an origin that serves no API and
 * fail silently — the failure mode this whole file exists to remove.
 */
export async function flush(): Promise<void> {
	if (standDown || inFlight || queue.length === 0) return;
	if (typeof fetch !== 'function') return;

	const batch = queue.slice(0, MAX_BATCH);
	// Captured BEFORE the synthetic drop-notice is appended: this is how many
	// real queued events the batch consumes, and mixing the two up is how you
	// get a logger that silently eats one event per flush.
	const consumed = batch.length;
	const lost = dropped;
	inFlight = true;
	try {
		if (lost > 0) {
			batch.push({
				kind: 'client.report.dropped',
				severity: 'warn',
				message: `${lost} client events were dropped before they could be sent`,
				occurred_at: nowIso()
			});
		}
		const res = await fetch('/api/events', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ events: batch }),
			keepalive: true
		});
		if (res.ok) {
			// Only drop what we actually sent — more may have queued meanwhile.
			queue = queue.slice(consumed);
			dropped = 0;
		} else if (res.status === 404) {
			// This box is older than this route, and no amount of retrying will
			// change that. An app is always liable to be ahead of a box: phones
			// update themselves and servers update when someone types a command,
			// so this is the NORMAL state after a client release, not an error.
			// Without this branch a phone would post into a 404 every ten
			// seconds, forever, for every user who had not upgraded yet.
			//
			// Give up for the session. A reload after the box is upgraded picks
			// it back up; nothing here is important enough to persist across one.
			standDown = true;
			queue = [];
			dropped = 0;
			if (timer !== null) {
				clearInterval(timer);
				timer = null;
			}
		} else if (res.status === 429) {
			// Over budget. The box has already logged that it is dropping this
			// device's reports, so re-sending the same batch would only spend
			// both sides again to reach the same place.
			queue = queue.slice(consumed);
		}
		// Any other non-2xx keeps the queue and tries again next tick. The cap
		// is what stops that from growing: a box that is down or unpaired
		// simply means the newest 100 events survive and older ones counted.
	} catch {
		// Offline, or the box is unreachable. Not worth a console line every
		// ten seconds — that is noise about a condition the user can see.
	} finally {
		inFlight = false;
	}
}

function emit(severity: Severity, component: string, message: string, detail?: unknown): void {
	const tag = component ? `[${component}] ` : '';
	const line = `${tag}${message}`;
	try {
		if (severity === 'error') console.error(line, detail ?? '');
		else if (severity === 'warn') console.warn(line, detail ?? '');
		else console.info(line, detail ?? '');
	} catch {
		/* rule 1 */
	}
	// Rule: the box hears warnings and errors. `info` is for the developer at
	// the console; shipping it would be analytics, which this is not.
	if (severity !== 'info') {
		enqueue(severity, `client.${component || 'app'}`, message, detail);
	}
}

export const log = {
	info: (component: string, message: string, detail?: unknown) =>
		emit('info', component, message, detail),
	warn: (component: string, message: string, detail?: unknown) =>
		emit('warn', component, message, detail),
	error: (component: string, message: string, detail?: unknown) =>
		emit('error', component, message, detail),

	/**
	 * Report something to the box without printing to the console — for a
	 * condition that is interesting to us and not to whoever is looking at the
	 * screen.
	 */
	report: (kind: string, message: string, detail?: unknown) =>
		enqueue('warn', kind, message, detail),

	/** Exposed for tests and for the page-hide path. */
	flush
};

let installed = false;

/**
 * Catch what nobody caught: uncaught exceptions and unhandled promise
 * rejections, plus a flush when the page is hidden.
 *
 * This is the half that pays for itself. Most of the app's existing
 * `console.error` calls sit in `catch` blocks, which means the code *handled*
 * the failure; the errors that actually break a screen are the ones no catch
 * saw, and until now they reached nothing at all.
 *
 * Idempotent, and safe to call during SSR (where there is no `window`).
 */
export function installErrorReporting(): void {
	if (installed || typeof window === 'undefined') return;
	installed = true;

	window.addEventListener('error', (e: ErrorEvent) => {
		try {
			const where = e.filename ? `${e.filename}:${e.lineno}:${e.colno}` : 'unknown';
			enqueue('error', 'client.uncaught', e.message || 'uncaught error', {
				where,
				stack: e.error instanceof Error ? e.error.stack : undefined
			});
		} catch {
			/* rule 1 */
		}
	});

	window.addEventListener('unhandledrejection', (e: PromiseRejectionEvent) => {
		try {
			const reason = e.reason;
			enqueue(
				'error',
				'client.unhandled_rejection',
				reason instanceof Error ? reason.message : String(reason),
				reason
			);
		} catch {
			/* rule 1 */
		}
	});

	// The last chance to say anything. `visibilitychange` rather than
	// `beforeunload`/`pagehide`: it is the one that fires reliably when a
	// phone's browser goes to the background, which is how a mobile session
	// usually ends.
	document.addEventListener('visibilitychange', () => {
		if (document.visibilityState === 'hidden') void flush();
	});
}
