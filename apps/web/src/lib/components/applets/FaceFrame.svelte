<script lang="ts">
	import { mintFaceToken } from '$lib/api/client';
	import { backendUrl } from '$lib/config/backend';
	import { parseRoute } from '$lib/tabs/registry';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import type { TabType } from '$lib/tabs/types';

	/**
	 * The applet face runtime: face/index.html rendered in a sandboxed
	 * iframe. sandbox="allow-scripts" (deliberately NO allow-same-origin)
	 * gives the document an opaque origin — no cookies, no storage, no
	 * parent access; its only data door is the token-scoped, read-only
	 * /api/face/query bridge injected via virtues.js.
	 */
	let { appletId, height = '420px' }: { appletId: string; height?: string } = $props();

	let src = $state<string | null>(null);
	let err = $state<string | null>(null);
	let frame = $state<HTMLIFrameElement | null>(null);

	// The rooms `virtues.open(route)` may open. A face reads the record, so
	// anything that carries a URL out of the app would let it send what it
	// read somewhere: the in-app browser is left out, and the query string is
	// dropped so no route can prefill a chat.
	const OPENABLE: ReadonlySet<TabType> = new Set<TabType>([
		'chat',
		'page',
		'tasks',
		'day',
		'year',
		'timeline',
		'wiki',
		'person',
		'place',
		'org',
		'project',
		'bookmark',
		'bookmarks',
		'applets',
		'applet-view'
	]);

	function onMessage(e: MessageEvent) {
		if (!frame || e.source !== frame.contentWindow) return;
		const data = e.data as { type?: unknown; route?: unknown } | null;
		if (data?.type !== 'virtues:open' || typeof data.route !== 'string') return;
		if (!data.route.startsWith('/') || data.route.startsWith('//')) return;
		const path = new URL(data.route, 'http://localhost').pathname;
		if (path === '/') {
			windowShellStore.newChat();
			return;
		}
		if (!OPENABLE.has(parseRoute(path).type)) return;
		windowShellStore.openTabFromRoute(path, { focusExisting: true });
	}

	$effect(() => {
		const id = appletId;
		src = null;
		err = null;
		void (async () => {
			try {
				const { token } = await mintFaceToken(id);
				const theme = document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
				// Absolute on mobile: an iframe src bypasses the fetch shim, so a
				// root-relative path would resolve against the bundled tauri://
				// origin and render an empty frame.
				src = backendUrl(
					`/face/${encodeURIComponent(id)}/?vt=${encodeURIComponent(token)}&theme=${theme}`
				);
			} catch (e) {
				err = e instanceof Error ? e.message : String(e);
			}
		})();
	});
</script>

<svelte:window onmessage={onMessage} />

{#if err}
	<div class="face-error">face unavailable: {err}</div>
{:else if src}
	<iframe
		bind:this={frame}
		{src}
		sandbox="allow-scripts"
		title="Applet face"
		class="face-frame"
		style:height
	></iframe>
{:else}
	<div class="face-loading">Loading face…</div>
{/if}

<style>
	.face-frame {
		width: 100%;
		border: 1px solid var(--color-border, #e5e7eb);
		border-radius: var(--radius-lg);
		background: var(--color-surface, #fff);
	}
	.face-error,
	.face-loading {
		padding: 1rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle, #9ca3af);
		border: 1px dashed var(--color-border, #e5e7eb);
		border-radius: var(--radius-lg);
	}
</style>
