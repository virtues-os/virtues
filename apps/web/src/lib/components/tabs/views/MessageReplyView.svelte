<!--
	MessageReplyView.svelte

	One reply the box drafted for a message thread, and the three things a
	person can do with it: send it as is, change it first, or let it go.

	The draft is the page. What was asked sits above it as context, the
	rationale below it as a footnote — the box's one line on what it used. The
	Send button is the whole point and is the only primary action; nothing here
	sends without that click.

	Sending is the Mac's job (Messages automation), so the button exists only
	in a shell new enough to have the command. Elsewhere the draft can still be
	copied, which is honest about what the page can do from here.
-->

<script lang="ts">
	import { Button, Page, Textarea } from '$lib';
	import type { Tab } from '$lib/tabs/types';
	import { isMacOS } from '$lib/utils/platform';
	import {
		dismissMessageReply,
		getMessageReply,
		markMessageReplySent,
		type MessageReply,
	} from '$lib/api/messageReplies';
	import {
		openMessagesThread,
		REPLY_SURFACE,
		sendIMessage,
		shellSupports,
	} from '$lib/tauri/bridge';

	let { tab }: { tab: Tab } = $props();

	const id = $derived(tab.route.match(/^\/reply\/(.+)$/)?.[1] ?? '');

	let reply = $state<MessageReply | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Uncontrolled once loaded, like the bookmark note: rebinding to the server
	// value would fight anyone still typing.
	let draft = $state('');
	let busy = $state<'send' | 'open' | 'dismiss' | null>(null);
	let actionError = $state<string | null>(null);
	let copied = $state(false);
	// Messages accepted the text. From here Send stays down whatever the box
	// says about recording it — a second click would send the person the same
	// message twice.
	let sentLocally = $state(false);

	// Whether this shell can send. Resolved once; the shell cannot change
	// under a running page.
	let canSend = $state(false);
	$effect(() => {
		void shellSupports(REPLY_SURFACE).then((ok) => (canSend = ok && isMacOS));
	});

	$effect(() => {
		const replyId = id;
		if (!replyId) {
			error = 'Malformed reply link.';
			loading = false;
			return;
		}
		loading = true;
		error = null;
		getMessageReply(replyId)
			.then((r) => {
				reply = r;
				draft = r.sent_text ?? r.draft;
			})
			.catch((e) => (error = e instanceof Error ? e.message : String(e)))
			.finally(() => (loading = false));
	});

	const who = $derived(reply ? (reply.from_name?.trim() || reply.from_handle) : '');
	const pending = $derived(reply?.status === 'pending' && !sentLocally);
	const text = $derived(draft.trim());

	async function send() {
		if (!reply || busy || !text || sentLocally) return;
		busy = 'send';
		actionError = null;
		try {
			await sendIMessage(reply.thread_id, reply.is_group ? null : reply.from_handle, text);
		} catch (e) {
			actionError = e instanceof Error ? e.message : 'Could not send that';
			busy = null;
			return;
		}
		sentLocally = true;
		try {
			reply = await markMessageReplySent(reply.id, text);
		} catch {
			// The collector will settle the row when it sees the outbound
			// message; say what happened rather than re-arm the button.
			actionError = 'Sent, but the box could not record it yet.';
		} finally {
			busy = null;
		}
	}

	// Opens Messages with the text typed in; the person presses return. Nothing
	// is marked sent here — the collector sees the outbound message and settles
	// the row, the same as a reply typed on the phone.
	async function openInMessages() {
		if (!reply || busy || !text) return;
		busy = 'open';
		actionError = null;
		try {
			await openMessagesThread(reply.from_handle, text);
		} catch (e) {
			actionError = e instanceof Error ? e.message : 'Could not open Messages';
		} finally {
			busy = null;
		}
	}

	async function dismiss() {
		if (!reply || busy) return;
		busy = 'dismiss';
		actionError = null;
		try {
			reply = await dismissMessageReply(reply.id);
		} catch (e) {
			actionError = e instanceof Error ? e.message : 'Could not dismiss that';
		} finally {
			busy = null;
		}
	}

	async function copy() {
		if (!text) return;
		try {
			await navigator.clipboard.writeText(text);
			copied = true;
			setTimeout(() => (copied = false), 1500);
		} catch {
			actionError = 'Could not copy';
		}
	}

	const statusLine = $derived.by(() => {
		if (!reply) return '';
		switch (reply.status) {
			case 'sent':
				return 'Sent.';
			case 'answered':
				return 'You answered this one yourself.';
			case 'dismissed':
				return 'Dismissed.';
			case 'expired':
				return 'This ask is a day old; the draft expired.';
			default:
				return '';
		}
	});
</script>

<Page title={who ? `Reply to ${who}` : 'Reply'} padding="compact">
	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="error">{error}</p>
	{:else if reply}
		<section class="ask">
			<p class="label">{who} asked{reply.is_group ? ' (group)' : ''}</p>
			<blockquote>{reply.ask_text}</blockquote>
		</section>

		<section class="draft">
			<Textarea bind:value={draft} rows={4} placeholder="Your reply" delight={false} />
			{#if reply.rationale}
				<p class="rationale">{reply.rationale}</p>
			{/if}
		</section>

		{#if pending}
			<div class="actions">
				{#if canSend}
					<Button onclick={send} disabled={!text || busy !== null}>
						{busy === 'send' ? 'Sending…' : 'Send'}
					</Button>
					{#if !reply.is_group}
						<Button variant="secondary" onclick={openInMessages} disabled={!text || busy !== null}>
							Open in Messages
						</Button>
					{/if}
				{:else}
					<Button onclick={copy} disabled={!text}>{copied ? 'Copied' : 'Copy'}</Button>
					<p class="muted">Sending happens from the Virtues app on your Mac.</p>
				{/if}
				<Button variant="ghost" onclick={dismiss} disabled={busy !== null}>Dismiss</Button>
			</div>
		{:else}
			<p class="muted">{sentLocally && reply.status === 'pending' ? 'Sent.' : statusLine}</p>
		{/if}

		{#if actionError}
			<p class="error">{actionError}</p>
		{/if}
	{/if}
</Page>

<style>
	.ask {
		margin-bottom: 1.25rem;
	}
	.label {
		margin: 0 0 0.35rem;
		font-size: 0.85rem;
		color: var(--color-foreground-muted);
	}
	blockquote {
		margin: 0;
		padding: 0.5rem 0.9rem;
		border-left: 2px solid var(--color-border);
		white-space: pre-wrap;
	}
	.draft {
		margin-bottom: 1rem;
	}
	.rationale {
		margin: 0.5rem 0 0;
		font-size: 0.8rem;
		color: var(--color-foreground-muted);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.6rem;
	}
	.muted {
		color: var(--color-foreground-muted);
		font-size: 0.9rem;
		margin: 0;
	}
	.error {
		color: var(--color-error);
		font-size: 0.9rem;
		margin: 0.5rem 0 0;
	}
</style>
