<script lang="ts">
	/**
	 * Keyboard probe — on-device diagnostics for the Phase-1a spike
	 * (docs/mobile-ux-plan.md). Shows live viewport metrics while the iOS
	 * keyboard opens/closes so we can see which signals are trustworthy in
	 * THIS wry/iOS combo before building the --keyboard-inset bridge:
	 *   - does window.innerHeight change? (expected: no)
	 *   - does visualViewport.height shrink by the keyboard? (tauri#10631
	 *     says it may not)
	 *   - does iOS auto-scroll the webview on focus? (offsetTop/scrollY)
	 *   - do values revert after dismissal? (iOS 26 quirk)
	 * Temporary tooling: reachable only from This Device → "Keyboard probe".
	 */
	let { close }: { close: () => void } = $props();

	type Snap = {
		innerH: number;
		vvH: number | null;
		vvOffTop: number | null;
		vvScale: number | null;
		scrollY: number;
		bodyScrollTop: number;
	};

	function snap(): Snap {
		const vv = window.visualViewport;
		return {
			innerH: window.innerHeight,
			vvH: vv ? Math.round(vv.height * 10) / 10 : null,
			vvOffTop: vv ? Math.round(vv.offsetTop * 10) / 10 : null,
			vvScale: vv ? vv.scale : null,
			scrollY: Math.round(window.scrollY),
			bodyScrollTop: Math.round(document.body.scrollTop)
		};
	}

	let current = $state<Snap>(snap());
	let baseline = $state<Snap>(snap());
	let log = $state<string[]>([]);

	function stamp(): string {
		return new Date().toISOString().slice(11, 23);
	}

	function record(source: string) {
		current = snap();
		const kb =
			current.vvH !== null ? Math.round((current.innerH - current.vvH) * 10) / 10 : "n/a";
		log = [
			`${stamp()} ${source}: innerH=${current.innerH} vvH=${current.vvH} vvTop=${current.vvOffTop} scrollY=${current.scrollY} kbEst=${kb}`,
			...log.slice(0, 39)
		];
	}

	$effect(() => {
		const vv = window.visualViewport;
		const onVvResize = () => record("vv.resize");
		const onVvScroll = () => record("vv.scroll");
		const onWinResize = () => record("win.resize");
		const onScroll = () => record("win.scroll");
		vv?.addEventListener("resize", onVvResize);
		vv?.addEventListener("scroll", onVvScroll);
		window.addEventListener("resize", onWinResize);
		window.addEventListener("scroll", onScroll);
		const tick = setInterval(() => (current = snap()), 500);
		return () => {
			vv?.removeEventListener("resize", onVvResize);
			vv?.removeEventListener("scroll", onVvScroll);
			window.removeEventListener("resize", onWinResize);
			window.removeEventListener("scroll", onScroll);
			clearInterval(tick);
		};
	});

	const kbEstimate = $derived(
		current.vvH !== null ? Math.round((current.innerH - current.vvH) * 10) / 10 : null
	);
</script>

<div class="probe">
	<header>
		<span>Keyboard probe</span>
		<button type="button" onclick={close}>Done</button>
	</header>

	<div class="metrics">
		<div><span>window.innerHeight</span><b>{current.innerH}</b><i>base {baseline.innerH}</i></div>
		<div><span>visualViewport.height</span><b>{current.vvH ?? "unsupported"}</b><i>base {baseline.vvH}</i></div>
		<div><span>visualViewport.offsetTop</span><b>{current.vvOffTop ?? "—"}</b><i>base {baseline.vvOffTop}</i></div>
		<div><span>window.scrollY</span><b>{current.scrollY}</b><i>base {baseline.scrollY}</i></div>
		<div><span>body.scrollTop</span><b>{current.bodyScrollTop}</b><i>base {baseline.bodyScrollTop}</i></div>
		<div class="kb"><span>keyboard estimate (innerH − vvH)</span><b>{kbEstimate ?? "n/a"}px</b></div>
	</div>

	<!-- Top input: focusing here should NOT need any scroll -->
	<input class="field" placeholder="Focus me (top) — watch the numbers" />

	<div class="log">
		{#each log as line (line)}<div>{line}</div>{/each}
		{#if log.length === 0}<div class="hint">
				Focus an input, dismiss the keyboard, repeat. Every viewport event is
				logged here — screenshot this screen with the keyboard OPEN and again
				after dismissing it.
			</div>{/if}
	</div>

	<!-- Bottom input: the case that matters (composer position) -->
	<input class="field bottom" placeholder="Focus me (bottom) — does iOS scroll the page?" />
</div>

<style>
	.probe {
		position: fixed;
		inset: 0;
		z-index: 80;
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: calc(env(safe-area-inset-top) + 8px) 14px calc(env(safe-area-inset-bottom) + 8px);
		background: var(--color-background);
		color: var(--color-foreground);
		font-size: 13px;
	}
	header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		font-weight: 600;
	}
	header button {
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-foreground);
		border-radius: 8px;
		padding: 6px 14px;
	}
	.metrics div {
		display: flex;
		gap: 8px;
		justify-content: space-between;
		padding: 3px 0;
		border-bottom: 1px solid var(--color-border-subtle, var(--color-border));
	}
	.metrics span {
		color: var(--color-foreground-muted);
	}
	.metrics i {
		font-style: normal;
		color: var(--color-foreground-muted);
		font-size: 11px;
	}
	.metrics .kb b {
		color: var(--color-primary);
	}
	.field {
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 10px 12px;
		font-size: 16px; /* no auto-zoom even without the viewport lock */
		background: var(--color-surface);
		color: var(--color-foreground);
	}
	.field.bottom {
		margin-top: auto;
	}
	.log {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		font-family: var(--font-mono, monospace);
		font-size: 10px;
		line-height: 1.5;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 8px;
		padding: 8px;
	}
	.hint {
		color: var(--color-foreground-muted);
		font-family: var(--font-sans);
		font-size: 12px;
	}
</style>
