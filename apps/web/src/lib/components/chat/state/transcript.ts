/**
 * transcript — turning what the box stored into what the view renders.
 *
 * Three load paths reach this file (mount, tab switch, and the re-read after
 * a compaction or after the getting-started room speaks), and they must agree
 * about the shape of a turn. They did not: `createdAt` was carried by one of
 * them and dropped by the other two, which put the interview's opening above
 * turns that preceded it (2026-09-14). One converter, used by all three.
 *
 * Pure functions, no state. The per-message metadata the render needs
 * (agent/provider, and the partial-reply flags) cannot be derived from parts,
 * so the caller hands in the map it renders from and the converter writes to
 * it — the same map, not a copy, because a reload must see the same flags a
 * live stream set.
 */

/** Type for tool result parts in messages */
export interface ToolResultPart {
	type: string;
	state?: string;
	toolCallId: string;
	output?: {
		page_id?: string;
		title?: string;
	};
}

/** What a message carries that its parts cannot say. */
export interface MessageMeta {
	agentId?: string;
	provider?: string;
	stopped?: boolean;
	cutShort?: boolean;
	interrupted?: boolean;
	unattended?: boolean;
	maxSteps?: boolean;
	budget?: boolean;
}

/** Helper function to convert database messages to Chat parts */
export function convertMessageToParts(msg: any, metadata: Map<string, MessageMeta>) {
	// Carry agent/provider + the partial-reply flags so the notice under a
	// stub survives a reload: subject='cancelled' is the person's stop,
	// subject='length' is the model's output window running out, and
	// subject='interrupted' is the stream or the model quitting (VIR-334).
	const stopped = msg.subject === "cancelled";
	const cutShort = msg.subject === "length";
	const interrupted = msg.subject === "interrupted";
	// The box's own cap, and the turn using up its steps. Both used to arrive
	// as one of the three above — the cap as the person's own stop.
	const unattended = msg.subject === "unattended";
	const maxSteps = msg.subject === "max_steps";
	// The turn's own cost or time ceiling, checked between steps.
	const budget = msg.subject === "budget";
	if (
		msg.agentId ||
		msg.provider ||
		stopped ||
		cutShort ||
		interrupted ||
		unattended ||
		maxSteps ||
		budget
	) {
		metadata.set(msg.id, {
			agentId: msg.agentId,
			provider: msg.provider,
			stopped,
			cutShort,
			interrupted,
			unattended,
			maxSteps,
			budget,
		});
	}

	// If message already has parts array (e.g., checkpoint messages), use it directly
	if (msg.parts && Array.isArray(msg.parts) && msg.parts.length > 0) {
		return msg.parts;
	}

	// Otherwise, construct parts from individual fields (legacy format)
	const parts: any[] = [];

	if (msg.reasoning) {
		parts.push({
			type: "reasoning" as const,
			text: msg.reasoning,
			state: "done" as const,
		});
	}

	if (msg.content) {
		parts.push({
			type: "text" as const,
			text: msg.content,
		});
	}

	if (msg.tool_calls && Array.isArray(msg.tool_calls)) {
		for (const toolCall of msg.tool_calls) {
			parts.push({
				type: `tool-${toolCall.tool_name}` as const,
				toolCallId:
					toolCall.tool_call_id ||
					`${msg.id}_${toolCall.tool_name}_${Date.now()}`,
				toolName: toolCall.tool_name,
				input: toolCall.arguments,
				state: "output-available" as const,
				output: toolCall.result,
			});
		}
	}

	return parts;
}

/** One stored turn as the view holds it. `createdAt` rides along because
 *  the getting-started room places the interview's opening among the
 *  turns by time — all three load paths must carry it, and only one did
 *  (2026-09-14: the opening landed above turns that preceded it). */
export function toUiMessage(msg: any, metadata: Map<string, MessageMeta>) {
	return {
		id: msg.id,
		role: msg.role as "user" | "assistant" | "checkpoint",
		parts: convertMessageToParts(msg, metadata),
		createdAt: msg.timestamp ? new Date(msg.timestamp) : undefined,
		// The room marks each line it speaks (`gs:done:connect_ai`, …), and
		// the render needs it to tell a settled step from the one being
		// asked. Dropped here until now, so every line looked the same.
		subject: msg.subject ?? undefined,
	};
}

/** Helper function to deduplicate messages by ID */
export function deduplicateMessages(messages: any[]): any[] {
	if (!messages || messages.length === 0) return [];
	const seen = new Set<string>();
	return messages.filter((msg) => {
		if (seen.has(msg.id)) {
			return false;
		}
		seen.add(msg.id);
		return true;
	});
}

/** A line the room speaks about a step that is already settled. */
export function isSettledLine(message: { subject?: string }): boolean {
	const s = message.subject;
	return !!s && (s.startsWith("gs:done:") || s === "gs:promise" || s === "gs:graduated");
}

/** What `record_introductions` wrote in this turn, if it was called. */
export function introductionsRecorded(message: { parts?: any[] }): any | null {
	const part = message.parts?.find(
		(p) => p?.type === "tool-record_introductions" && p?.state === "output-available",
	);
	return part?.output?.fields ?? null;
}

/**
 * The one line per step that carries its number: the ask, or — for a step
 * settled before the room ever asked (AI already connected, integrations
 * already there) — the line that settled it. Keyed by message id.
 */
export function eyebrowsFor(messages: { id: string; subject?: string }[]): Map<string, string> {
	const out = new Map<string, string>();
	const claimed = new Set<string>();
	for (const m of messages) {
		const match = m.subject?.match(/^gs:(ask|done):(.+)$/);
		if (!match || claimed.has(match[2])) continue;
		claimed.add(match[2]);
		out.set(m.id, match[2]);
	}
	return out;
}

/**
 * The turns the owner took, indexed for the rail (ConversationRail.svelte).
 *
 * The anchor ids are the `exchange-N` ids the view already puts on user rows,
 * and N counts user messages — so this must count them the same way or the
 * rail scrolls to the wrong turn. A turn's `preview` is the opening of the
 * next assistant reply, which is what makes an entry recognizable: a lone
 * "and then?" names nothing.
 *
 * Mention pills come out of the composer as `[Name](/person/id)`; the rail
 * shows the name, since the URL is noise at this size.
 */
export function railTurns(
	messages: { id: string; role: string; parts?: any[] }[],
): { id: string; anchor: string; label: string; preview: string }[] {
	const out: { id: string; anchor: string; label: string; preview: string }[] = [];
	let n = 0;
	for (let i = 0; i < messages.length; i++) {
		const m = messages[i];
		if (m.role !== "user") continue;
		const anchor = `exchange-${n}`;
		n++;
		const label = plainText(m.parts);
		if (!label) continue;
		let preview = "";
		for (let j = i + 1; j < messages.length; j++) {
			if (messages[j].role === "user") break;
			if (messages[j].role !== "assistant") continue;
			preview = plainText(messages[j].parts);
			if (preview) break;
		}
		out.push({ id: m.id, anchor, label, preview: clip(preview, 200) });
	}
	return out;
}

function plainText(parts: any[] | undefined): string {
	return clip(
		(parts ?? [])
			.filter((p: any) => p?.type === "text" && typeof p.text === "string")
			.map((p: any) => p.text)
			.join(" ")
			.replace(/\[([^\]\n]+)\]\([^)\s]+\)/g, "$1")
			.replace(/\s+/g, " ")
			.trim(),
		200,
	);
}

/** Long enough for two clamped lines, short enough not to carry a transcript. */
function clip(text: string, max: number): string {
	return text.length > max ? `${text.slice(0, max).trimEnd()}…` : text;
}
