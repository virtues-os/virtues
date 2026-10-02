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
	unavailable?: boolean;
	unattended?: boolean;
	maxSteps?: boolean;
	budget?: boolean;
}

/** Why a reply is partial, as StoppedNotice words it. */
export type StopReason =
	| "stopped"
	| "length"
	| "interrupted"
	| "unavailable"
	| "unattended"
	| "max_steps"
	| "budget";

/**
 * The one reason to show under a partial reply, or null for a whole one. A
 * message carries at most one flag in practice; if it carried several, the
 * person's own stop wins, then the order below.
 */
export function stopReason(meta: MessageMeta | undefined): StopReason | null {
	if (!meta) return null;
	if (meta.stopped) return "stopped";
	if (meta.cutShort) return "length";
	if (meta.unavailable) return "unavailable";
	if (meta.interrupted) return "interrupted";
	if (meta.unattended) return "unattended";
	if (meta.maxSteps) return "max_steps";
	if (meta.budget) return "budget";
	return null;
}

/** An assistant turn split into what the thinking block shows and where the reply starts. */
export interface SplitTurn {
	/** Tool calls, for the thinking block. */
	toolParts: any[];
	/** Reasoning text, non-empty parts joined by newlines. */
	reasoning: string;
	/** Text runs that came before the reply: the model saying what it was about to do. */
	narration: string[];
	/**
	 * The newest narration run, if the call right after it is the newest
	 * call — the line that introduced what is in flight. Empty when the
	 * model has since made a call without saying anything.
	 */
	intent: string;
	/** Index of the first part that belongs in the body; text before it is narration. */
	bodyFromIndex: number;
	/** Whether the thinking block has anything to show. */
	hasThinkingContent: boolean;
}

/**
 * Split an assistant turn's parts.
 *
 * A turn is several runs of text with tool calls between them. A run with a
 * tool call AFTER it was the model saying what it was about to do; the run
 * with nothing after it is the reply. Only the reply belongs in the
 * transcript — the rest is working-out and goes to the thinking block,
 * which is where its status label comes from. "Has a tool after it" rather
 * than "is not the last one" because it has to hold mid-turn too: the line
 * the model just wrote is its answer until a tool starts, and at that moment
 * it becomes narration and moves. A message stored before `parts` carried
 * this order has one text run and no tool before it, so it is all reply.
 *
 * Where the reply starts is normally just past the last tool call. But a
 * turn that ENDED on a tool call — an error, a stop, the model quitting —
 * never wrote one, and treating all of its text as narration would leave a
 * blank message with the words hidden in a collapsed block. So once the turn
 * is over, the last thing it said stands as the reply. While it is still
 * streaming it does not: there is no reply yet, and the line the model wrote
 * is already showing as the status label.
 */
export function splitTurn(parts: any[], isStreaming: boolean): SplitTurn {
	const isText = (p: any) => p.type === "text" && !!p.text?.trim();
	let lastTool = -1;
	let lastText = -1;
	parts.forEach((p, i) => {
		if (p.type.startsWith("tool-")) lastTool = i;
		if (isText(p)) lastText = i;
	});
	const bodyFromIndex = isStreaming || lastText > lastTool ? lastTool + 1 : lastText;
	const toolParts = parts.filter((p) => p.type.startsWith("tool-"));
	const reasoning = parts
		.filter((p) => p.type === "reasoning")
		.map((p) => p.text || "")
		.filter(Boolean)
		.join("\n");
	const narrationAt = parts
		.map((p, i) => i)
		.filter((i) => isText(parts[i]) && i < bodyFromIndex);
	const narration = narrationAt.map((i) => parts[i].text.trim());
	const lastSaid = narrationAt.at(-1) ?? -1;
	const callsSince = parts.filter((p, i) => i > lastSaid && p.type.startsWith("tool-")).length;
	const intent = lastSaid >= 0 && callsSince === 1 ? narration[narration.length - 1] : "";
	return {
		toolParts,
		reasoning,
		narration,
		intent,
		bodyFromIndex,
		hasThinkingContent: !!reasoning || toolParts.length > 0 || narration.length > 0,
	};
}

/**
 * Did the turn go on after the tool call at `index` — another call, or
 * reply text? A failed call the model then recovered from is working-out
 * and belongs in the thinking block with the other calls; one the turn
 * ended on is what the person got instead of an answer, and stays in the
 * body. Measured on a live box: nine of nine sql_query failures in two
 * weeks were a guessed column followed by the right one.
 */
export function turnMovedPast(parts: any[], index: number): boolean {
	return parts.some(
		(p: any, i: number) =>
			i > index && (p.type.startsWith("tool-") || (p.type === "text" && p.text?.trim())),
	);
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
	// The model's provider turned the call away, so nothing was cut off.
	const unavailable = msg.subject === "unavailable";
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
		unavailable ||
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
			unavailable,
			unattended,
			maxSteps,
			budget,
		});
	}

	// If message already has parts array (e.g., checkpoint messages), use it directly
	if (msg.parts && Array.isArray(msg.parts) && msg.parts.length > 0) {
		return msg.parts.map(perToolType);
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

/**
 * A stored tool part under its own type, `tool-<toolName>`, as it streamed.
 * Every per-tool render matches that exact type. Boxes before the wire fix
 * serve reloaded parts as `tool-invocation`, and a phone can be newer than
 * its box, so the client accepts both.
 */
function perToolType(part: any): any {
	if (part?.type !== "tool-invocation" || !part.toolName) return part;
	return { ...part, type: `tool-${part.toolName}` };
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
		// The span of the turn that wrote it, as the box recorded it.
		startedAt: msg.startedAt ? new Date(msg.startedAt) : undefined,
		endedAt: msg.endedAt ? new Date(msg.endedAt) : undefined,
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
