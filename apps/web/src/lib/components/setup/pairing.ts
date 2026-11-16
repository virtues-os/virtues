/**
 * Pairing by address and code, in words: the line `virtues pair` prints, and
 * what a failure means to the person holding the device.
 *
 * TODO(2026-11-15): `src-tauri/ui/connect.html` (its `Router`) has its own
 * copy of both, because that page is compiled into the binary and loads
 * nothing. Delete that copy when Windows and Linux carry their own copy of
 * the app and stop using connect.html (agents/plan/onboarding-diy-plan.md,
 * slice 3).
 */

/**
 * One line copied from `virtues pair` ("http://192.168.1.20:8000   123 456"),
 * or read from its QR code, into its address and code. Null when the text is
 * not that shape, so whatever was typed stays as it is.
 */
export function splitPasted(text: string): { address: string; code: string } | null {
	const m = String(text || '')
		.trim()
		.match(/^(\S+)\s+(\d{3})\s?(\d{3})$/);
	return m ? { address: m[1], code: m[2] + m[3] } : null;
}

/**
 * A pairing failure in words, from what the reach plugin threw: reqwest's
 * error chain, the server's JSON error, or an OS error. Pairing by address is
 * the direct connection, this device to the server with no relay, so a
 * failure before the server answers says "directly". `paired` is true when
 * the server took the code and only this device's own side failed afterwards.
 */
export function pairError(
	raw: unknown,
	{ address, device }: { address?: string; device: string },
): { paired: boolean; text: string } {
	const s = String(raw instanceof Error ? raw.message : (raw ?? ''));
	const at = String(address || 'that address')
		.replace(/^https?:\/\//, '')
		.replace(/\/+$/, '');
	const Device = device[0].toUpperCase() + device.slice(1);
	if (/bind 127\.0\.0\.1:\d+/.test(s) && /in use|os error (48|98)\b/i.test(s)) {
		return {
			paired: true,
			text: `Paired. Another copy of Virtues on ${device} is using the port it needs. Quit the other copy, then open Virtues again.`,
		};
	}
	if (/invalid_or_expired_token|\b401\b/.test(s)) {
		return {
			paired: false,
			text: "That code didn't work. It may have expired. On your server, run virtues pair for a new one.",
		};
	}
	if (/too_many_attempts|\b429\b/.test(s)) {
		return { paired: false, text: 'Too many wrong codes. Wait 30 minutes, then try a fresh code from virtues pair.' };
	}
	if (/relative URL|invalid (url|uri)|builder error|empty host|invalid port/i.test(s)) {
		return {
			paired: false,
			text: `That isn't an address ${device} can use. Type it the way virtues pair prints it, like http://192.168.1.20:8000.`,
		};
	}
	if (/dns error|failed to lookup|nodename nor servname|name or service not known|no such host/i.test(s)) {
		return {
			paired: false,
			text: `${Device} couldn't find ${at} on this network. Try the numbered address virtues pair printed. Some networks don't carry .local names.`,
		};
	}
	if (/connection refused|os error (61|111)\b/i.test(s)) {
		return {
			paired: false,
			text: `${at} is reachable, but Virtues isn't serving on that port. Check the port, usually 8000, and that the server is running.`,
		};
	}
	if (/timed out|timeout|no route to host|network is unreachable|host is down|os error (51|60|64|65|110|113)\b/i.test(s)) {
		return {
			paired: false,
			text: `${Device} couldn't reach ${at} directly. Make sure both are on the same network. Some networks and virtual machines keep devices apart.`,
		};
	}
	if (/\b404\b/.test(s)) {
		return { paired: false, text: `Something answered at ${at}, but it isn't a Virtues server. Check the address and port.` };
	}
	return { paired: false, text: "Pairing didn't finish. Try again with a fresh code from virtues pair." };
}
