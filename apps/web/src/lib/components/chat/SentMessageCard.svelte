<script lang="ts">
	/**
	 * SentMessageCard
	 *
	 * A message the assistant sent as the owner (`send_imessage`), in the reply
	 * where it was sent. Framed, because these are the owner's words: a card is
	 * you answering (agents/build/design-grammar.md §2). The words are set in
	 * the serif at the size of what you kept; the line above says where the
	 * message stands, read live from the box's outbox (`outbox.rs`):
	 *
	 *   pending   Sending to Nick in 4s · Cancel   (or the scheduled time)
	 *   sending   Sending to Nick…
	 *   sent      Sent to Nick · 3:42 PM            handed to Messages
	 *   delivered Delivered to Nick · 3:42 PM       chat.db says it arrived
	 *   failed    Not delivered to Nick, and why
	 *   cancelled Not sent to Nick · Cancelled
	 *
	 * A tool call that failed outright never gets here; the chat's error line
	 * shows it instead.
	 */
	import {
		cancelOutboxMessage,
		getOutboxMessage,
		type OutboxAttachment,
		type OutboxMessage,
	} from "$lib/api/client";
	import { backendUrl } from "$lib/config/backend";
	import { openExternal } from "$lib/tauri/bridge";

	interface Props {
		/** The outbox row id, once the tool has answered. */
		outboxId?: string;
		/** What the tool answered, until the box says more. */
		initial?: Partial<OutboxMessage>;
		/** The owner named a time (the tool's `scheduled`). */
		scheduled?: boolean;
		/** While the model is still writing the call. */
		to: string;
		text: string;
		onOpenPerson?: (personId: string) => void;
	}

	let { outboxId, initial, scheduled = false, to, text, onOpenPerson }: Props = $props();

	let live = $state<Partial<OutboxMessage> | null>(null);
	let now = $state(Date.now());
	let cancelling = $state(false);
	let cancelError = $state("");

	const row = $derived<Partial<OutboxMessage>>({ ...initial, ...live });
	const sendAt = $derived(row.send_at ? Date.parse(row.send_at) : NaN);
	const secondsLeft = $derived(Math.max(0, Math.ceil((sendAt - now) / 1000)));
	// A send whose window has run out is going now, whatever the last poll said.
	const phase = $derived.by(() => {
		if (!outboxId) return "writing";
		const s = row.state ?? "pending";
		return s === "pending" && !scheduled && secondsLeft === 0 ? "sending" : s;
	});
	const recipient = $derived(row.recipient || to);
	const body = $derived(row.body || text);
	const files = $derived<OutboxAttachment[]>(row.attachments ?? []);
	const canOpen = $derived(Boolean(row.handle) && ["sent", "delivered", "failed"].includes(phase));

	// ── Following the row ────────────────────────────────────────────────────

	let timer: ReturnType<typeof setTimeout> | undefined;
	let ticker: ReturnType<typeof setInterval> | undefined;

	/** How soon to ask again, or `null` when the row has settled. */
	function nextPoll(r: Partial<OutboxMessage>): number | null {
		switch (r.state) {
			case "pending": {
				const wait = Date.parse(r.send_at ?? "") - Date.now();
				return wait > 60_000 ? 30_000 : Math.max(500, Math.min(wait + 300, 1_000));
			}
			case "sending":
				return 1_000;
			// The collector reads the owner's last few minutes again on each
			// tick, so delivery shows within minutes. Past an hour, the box
			// stops looking too.
			case "sent": {
				const age = Date.now() - Date.parse(r.sent_at ?? r.send_at ?? "");
				return age < 60 * 60_000 ? 15_000 : null;
			}
			default:
				return null;
		}
	}

	async function poll() {
		if (!outboxId) return;
		try {
			live = await getOutboxMessage(outboxId);
		} catch {
			// An older box without the outbox, or a moment offline: keep what
			// the tool answered and try again later.
		}
		const wait = nextPoll(row);
		if (wait !== null) timer = setTimeout(poll, wait);
	}

	$effect(() => {
		if (!outboxId) return;
		poll();
		ticker = setInterval(() => (now = Date.now()), 250);
		return () => {
			clearTimeout(timer);
			clearInterval(ticker);
		};
	});

	async function cancel() {
		if (!outboxId || cancelling) return;
		cancelling = true;
		cancelError = "";
		try {
			live = await cancelOutboxMessage(outboxId);
			if (live.state !== "cancelled") cancelError = "Too late to cancel: it had already gone.";
		} catch {
			cancelError = "Couldn't reach your server to cancel. Try again.";
		} finally {
			cancelling = false;
		}
	}

	/** Where the box serves a file the message carried. */
	function fileUrl(ref: string): string {
		if (ref.startsWith("/api/")) return backendUrl(ref);
		const id = ref.replace(/^\/drive\//, "").split(/[?#]/)[0];
		return backendUrl(`/api/drive/files/${encodeURIComponent(id)}/download`);
	}

	function openInMessages() {
		if (row.handle) void openExternal(`sms:${row.handle}`);
	}

	// ── Words ────────────────────────────────────────────────────────────────

	function clock(iso: string | null | undefined): string {
		if (!iso) return "";
		const at = new Date(iso);
		return Number.isNaN(at.getTime()) ? "" : at.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
	}

	/** "Today 9:00 AM", "Tomorrow 9:00 AM", "Thu, Oct 22 9:00 AM". */
	function when(iso: string | undefined): string {
		if (!iso) return "";
		const at = new Date(iso);
		const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
		const diff = Math.round((day(at) - day(new Date())) / 86_400_000);
		const label =
			diff === 0
				? "Today"
				: diff === 1
					? "Tomorrow"
					: at.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" });
		return `${label} ${clock(iso)}`;
	}

	const headline = $derived.by(() => {
		switch (phase) {
			case "writing":
				return "To";
			case "pending":
				return scheduled ? "Scheduled for" : "Sending to";
			case "sending":
				return "Sending to";
			case "sent":
				return "Sent to";
			case "delivered":
				return "Delivered to";
			case "failed":
				return row.sent_at ? "Not delivered to" : "Not sent to";
			default:
				return "Not sent to";
		}
	});

	const aside = $derived.by(() => {
		switch (phase) {
			case "pending":
				return scheduled ? when(row.send_at) : `in ${secondsLeft}s`;
			case "sent":
				return clock(row.sent_at);
			case "delivered":
				return clock(row.delivered_at ?? row.sent_at);
			case "cancelled":
				return "Cancelled";
			default:
				return "";
		}
	});
</script>

<figure class="sent-message" data-state={phase} aria-live="polite">
	<figcaption>
		<span class="headline">
			{headline}
			{#if row.person_id && onOpenPerson}
				<button type="button" class="link" onclick={() => onOpenPerson(row.person_id!)}>{recipient}</button>
			{:else}
				{recipient}
			{/if}{#if phase === "sending"}…{/if}
		</span>
		<span class="aside">
			{#if aside}
				{#if phase === "pending" || phase === "cancelled"}
					<span>{aside}</span>
				{:else}
					<time datetime={row.delivered_at ?? row.sent_at ?? undefined}>{aside}</time>
				{/if}
			{/if}
			{#if phase === "pending"}
				<button type="button" class="link" onclick={cancel} disabled={cancelling}>Cancel</button>
			{:else if canOpen}
				<button type="button" class="link" onclick={openInMessages}>Open in Messages</button>
			{/if}
		</span>
	</figcaption>
	{#if body}<blockquote>{body}</blockquote>{/if}
	{#if files.length}
		<ul class="files">
			{#each files as file (file.ref)}
				<li>
					{#if file.mime_type.startsWith("image/")}
						<img src={fileUrl(file.ref)} alt={file.filename} loading="lazy" />
					{:else}
						<span class="filename">{file.filename}</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	{#if phase === "failed" && row.error}
		<p class="note error">{row.error}</p>
	{:else if cancelError}
		<p class="note">{cancelError}</p>
	{/if}
</figure>

<style>
	.sent-message {
		margin: 0.75rem 0;
		max-width: 40em;
		padding: 0.75rem 1rem 0.875rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
	}

	/* On a narrow pane the time and actions drop under the name, rather than
	   breaking "Sent to" from who it went to. */
	figcaption {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.125rem 1rem;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.headline {
		min-width: 0;
		max-width: 100%;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.aside {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}

	.link {
		font: inherit;
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
	}

	.link:hover:not(:disabled) {
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.link:disabled {
		cursor: default;
		opacity: 0.6;
	}

	time {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	blockquote {
		margin: 0.375rem 0 0;
		font-family: var(--font-serif);
		font-size: 16px;
		line-height: 1.5;
		color: var(--color-foreground);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		transition: color 160ms ease;
	}

	/* Not yet gone, or never went: the words read as not (yet) sent. */
	[data-state="writing"] blockquote,
	[data-state="pending"] blockquote,
	[data-state="sending"] blockquote,
	[data-state="cancelled"] blockquote {
		color: var(--color-foreground-muted);
	}

	.files {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin: 0.5rem 0 0;
		padding: 0;
		list-style: none;
	}

	.files img {
		display: block;
		width: 96px;
		height: 96px;
		object-fit: cover;
		border-radius: var(--radius-lg);
		border: 1px solid var(--color-border);
	}

	.filename {
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* Not gone yet: the files read as not sent, like the words. */
	[data-state="writing"] .files,
	[data-state="pending"] .files,
	[data-state="sending"] .files,
	[data-state="cancelled"] .files {
		opacity: 0.6;
	}

	.note {
		margin: 0.5rem 0 0;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* A state the owner can act on: Open in Messages sits beside it. */
	.note.error {
		color: var(--color-error);
	}

	@media (prefers-reduced-motion: reduce) {
		blockquote {
			transition: none;
		}
	}
</style>
