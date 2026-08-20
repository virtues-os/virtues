<script lang="ts">
	import { mintFaceToken } from '$lib/api/client';
	import { backendUrl } from '$lib/config/backend';

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

{#if err}
	<div class="face-error">face unavailable: {err}</div>
{:else if src}
	<iframe
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
		border-radius: 8px;
		background: var(--color-surface, #fff);
	}
	.face-error,
	.face-loading {
		padding: 1rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle, #9ca3af);
		border: 1px dashed var(--color-border, #e5e7eb);
		border-radius: 8px;
	}
</style>
