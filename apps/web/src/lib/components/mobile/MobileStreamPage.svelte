<!--
	MobileStreamPage — one stream of this phone, on its own page.

	The Settings list keeps every stream to one line; anything a stream can
	be told (stop, when, where, notify) lives here instead of trailing under
	its row. The frame is the same for all six: a back header, a status card
	with the one primary action, a sentence on what the stream collects, the
	sync line, then whatever settings the stream has as children. Recent
	activity borrows the frame too — it is a page under the same list.
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import type { OutboxStats } from "$lib/mobile/deviceTypes";

	interface Props {
		title: string;
		icon: string;
		/** The status line under the title: "Recording · 1 syncing". */
		status: string;
		/** Lit when the stream is on. */
		on: boolean;
		/** What this stream collects, in one sentence. */
		description: string;
		action?: { label: string; onclick: () => void; disabled?: boolean } | null;
		sync?: OutboxStats | null;
		error?: string | null;
		onBack: () => void;
		/** The name of the list this page sits under. */
		backLabel?: string;
		children?: Snippet;
	}
	let {
		title,
		icon,
		status,
		on,
		description,
		action = null,
		sync = null,
		error = null,
		onBack,
		backLabel = "Settings",
		children,
	}: Props = $props();

	const syncLine = $derived.by(() => {
		if (!sync) return null;
		if (sync.failing > 0) return `${sync.failing} retrying`;
		if (sync.queued > 0) return `${sync.queued} waiting to sync`;
		return "Synced to your server";
	});
</script>

<div class="page">
	<div class="head">
		<button class="back" type="button" onclick={onBack}>
			<Icon icon="ri:arrow-left-s-line" width={22} />
			<span>{backLabel}</span>
		</button>
	</div>

	<div class="body">
		<div class="card hero">
			<div class="h-icon" class:on><Icon {icon} width={22} /></div>
			<div class="h-body">
				<div class="h-title">{title}</div>
				<div class="h-status">{status}</div>
			</div>
			{#if action}
				<button class="act" type="button" onclick={action.onclick} disabled={action.disabled}>
					{action.label}
				</button>
			{/if}
		</div>
		<p class="desc">{description}</p>
		{#if syncLine}
			<p class="desc sync">{syncLine}</p>
		{/if}
		{#if error}
			<p class="desc err">{error}</p>
		{/if}

		{#if children}
			{@render children()}
		{/if}
	</div>
</div>

<style>
	.page {
		position: fixed;
		inset: 0;
		z-index: 70;
		display: flex;
		flex-direction: column;
		background: var(--color-surface);
		color: var(--color-foreground);
		animation: slide 0.22s cubic-bezier(0.32, 0.72, 0, 1);
	}
	@keyframes slide {
		from {
			transform: translateX(28px);
			opacity: 0;
		}
	}
	.head {
		display: flex;
		align-items: center;
		padding: max(10px, env(safe-area-inset-top)) 8px 4px;
		flex: none;
	}
	.back {
		display: flex;
		align-items: center;
		gap: 2px;
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 15px;
		padding: 6px 8px 6px 2px;
		cursor: pointer;
	}
	.body {
		flex: 1;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		padding: 6px 16px max(24px, env(safe-area-inset-bottom));
	}
	.card {
		border: 1px solid var(--color-border);
		border-radius: 12px;
		overflow: hidden;
	}
	.hero {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 14px;
	}
	.h-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 40px;
		height: 40px;
		flex: none;
		border-radius: 10px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	.h-icon.on {
		background: color-mix(in srgb, var(--color-primary) 16%, transparent);
		color: var(--color-primary);
	}
	.h-body {
		flex: 1;
		min-width: 0;
	}
	.h-title {
		font-size: 19px;
		font-weight: 550;
		letter-spacing: -0.01em;
	}
	.h-status {
		font-size: 13px;
		color: var(--color-foreground-muted);
		margin-top: 2px;
	}
	.act {
		flex: none;
		border: 1px solid var(--color-primary);
		color: var(--color-primary);
		background: transparent;
		border-radius: 8px;
		padding: 7px 14px;
		font-size: 13px;
		font-weight: 600;
		cursor: pointer;
	}
	.act:disabled {
		opacity: 0.5;
	}
	.desc {
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 12px 4px 0;
	}
	.desc.sync {
		margin-top: 4px;
	}
	.desc.err {
		color: var(--color-foreground);
	}
</style>
