<script lang="ts">
	import { mintFaceToken } from '$lib/api/client';
	import { backendUrl } from '$lib/config/backend';
	import { parseRoute } from '$lib/tabs/registry';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import type { TabType } from '$lib/tabs/types';
	import { getTheme, isThemeDark } from '$lib/utils/theme';
	import { siteBuilds } from '$lib/applets/sites.svelte';
	import { tokenForgotten } from '$lib/applets/viewToken';

	/**
	 * The applet face runtime: face/index.html rendered in a sandboxed
	 * iframe. sandbox="allow-scripts" (deliberately NO allow-same-origin)
	 * gives the document an opaque origin — no cookies, no storage, no
	 * parent access; its only data door is the token-scoped, read-only
	 * /api/face/query bridge injected via virtues.js.
	 */
	let {
		appletId,
		height = '420px',
		keepAlive = true
	}: {
		appletId: string;
		height?: string;
		/** Whether to keep checking the view's token. A card's small preview doesn't. */
		keepAlive?: boolean;
	} = $props();

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
		'day',
		'year',
		'wiki',
		'person',
		'place',
		'org',
		'thread',
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

	// A token lasts an hour on the server. The frame loads again with a new
	// one shortly before that, or every query in a view left open would start
	// failing with nothing on screen to say why.
	const RENEW_BEFORE_MS = 5 * 60 * 1000;
	// How long to wait before asking for a token again when the server didn't
	// answer, and how often to ask whether the token the view has still stands.
	const RETRY_AFTER_MS = 30_000;
	const CHECK_EVERY_MS = 60_000;
	let renewals = $state(0);
	// The token the frame was loaded with. Not drawn, so not reactive.
	let held: string | null = null;

	$effect(() => {
		const id = appletId;
		// Read so that a rebuild of this site, or a renewal, loads it again.
		void siteBuilds.counts[id];
		void renewals;
		err = null;
		let renew: ReturnType<typeof setTimeout> | undefined;
		let gone = false;
		void (async () => {
			try {
				const { token, expires_in_seconds } = await mintFaceToken(id);
				if (gone) return;
				held = token;
				renew = setTimeout(
					() => (renewals += 1),
					Math.max(60_000, expires_in_seconds * 1000 - RENEW_BEFORE_MS)
				);
				// The face's stylesheet knows only light and dark (faces.rs, VIRTUES_CSS).
				const theme = isThemeDark(getTheme()) ? 'dark' : 'light';
				// Absolute on mobile: an iframe src bypasses the fetch shim, so a
				// root-relative path would resolve against the bundled tauri://
				// origin and render an empty frame.
				src = backendUrl(
					`/face/${encodeURIComponent(id)}/?vt=${encodeURIComponent(token)}&theme=${theme}`
				);
			} catch (e) {
				if (gone) return;
				// A view that is already showing stays while the server is away,
				// and asks again. Only a view that never loaded says it couldn't.
				if (held) renew = setTimeout(() => (renewals += 1), RETRY_AFTER_MS);
				else err = e instanceof Error ? e.message : String(e);
			}
		})();
		return () => {
			gone = true;
			clearTimeout(renew);
		};
	});

	// The server keeps tokens in memory, so a restart forgets the one this
	// view has, well before the hour is up. While the window is showing, and
	// each time it comes back, the frame asks whether its token still stands
	// and loads again with a new one when the server has forgotten it.
	$effect(() => {
		if (!keepAlive) return;
		let asking = false;
		const check = async () => {
			const token = held;
			if (!token || asking || document.visibilityState !== 'visible') return;
			asking = true;
			const forgotten = await tokenForgotten(token);
			asking = false;
			// A renewal may have replaced the token while the server answered.
			if (forgotten && held === token) renewals += 1;
		};
		const timer = setInterval(check, CHECK_EVERY_MS);
		document.addEventListener('visibilitychange', check);
		window.addEventListener('online', check);
		return () => {
			clearInterval(timer);
			document.removeEventListener('visibilitychange', check);
			window.removeEventListener('online', check);
		};
	});
</script>

<svelte:window onmessage={onMessage} />

{#if err}
	<div class="face-error">Your server couldn't load this site: {err}. Close it and open it again.</div>
{:else if src}
	<iframe
		bind:this={frame}
		{src}
		sandbox="allow-scripts"
		title="Site"
		class="face-frame"
		style:height
	></iframe>
{:else}
	<div class="face-loading">Loading…</div>
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
