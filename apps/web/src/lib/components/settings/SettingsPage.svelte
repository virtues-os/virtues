<script lang="ts">
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	/**
	 * Every Settings page has the same top: a link back, the title, one
	 * sentence. Section pages (SettingsSection) and the pages a row opens
	 * (Keys, Devices, What AI has cost…) all use this, so the room reads as
	 * one place, at one width (`--page-settings`). `width` is `full` for a
	 * page that holds a table, which takes the medium width.
	 */
	import type { Snippet } from 'svelte';
	import { openSettingsPage } from '$lib/settings/focus';

	let {
		title,
		lede,
		back = { route: '/virtues', label: 'Settings' },
		assistantName = DEFAULT_ASSISTANT_NAME,
		width = 'reading',
		actions,
		children,
	}: {
		title: string;
		lede?: string;
		/** Where the link at the top goes: Settings, or the page this one opens from. */
		back?: { route: string; label: string };
		/** For the tab label of the page the back link opens. */
		assistantName?: string;
		width?: 'reading' | 'full';
		/** A control beside the title, such as Add a key. */
		actions?: Snippet;
		children?: Snippet;
	} = $props();
</script>

<div class="page" class:full={width === 'full'}>
	<button type="button" class="back" onclick={() => openSettingsPage(back.route, assistantName)}>
		<svg viewBox="0 0 24 24" aria-hidden="true" width="16" height="16">
			<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.7" d="M15 6l-6 6l6 6" />
		</svg>
		{back.label}
	</button>
	<div class="head">
		<h1>{title}</h1>
		{#if actions}<div class="actions">{@render actions()}</div>{/if}
	</div>
	{#if lede}<p class="lede">{lede}</p>{/if}
	{@render children?.()}
</div>

<style>
	/* Border-box, as the landing is, so the text column is the same width on both. */
	.page {
		box-sizing: border-box;
		max-width: var(--page-settings);
		margin: 0 auto;
		padding: 32px 32px 96px;
	}

	.page.full {
		max-width: var(--page-medium);
	}

	.back {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin: 0 0 20px -4px;
		padding: 4px;
		border: none;
		border-radius: var(--radius-sm);
		background: none;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.back:hover {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}

	.head {
		display: flex;
		align-items: flex-end;
		justify-content: space-between;
		gap: 16px;
	}

	h1 {
		font-size: 2.5rem;
		font-weight: 400;
		line-height: 1.1;
		margin: 0;
	}

	.actions {
		flex: none;
		padding-bottom: 4px;
	}

	.lede {
		margin: 8px 0 24px;
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	@media (max-width: 767px) {
		.page {
			padding: 24px 16px 64px;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.back {
			transition: none;
		}
	}
</style>
