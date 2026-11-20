<script lang="ts">
	/**
	 * The frame the radar and the graph share: the wedge for what is in no
	 * thread, a wash on the slice of the thread you chose, a band over each
	 * group of threads, and each thread's name on the rim. Both views draw
	 * inside it, so a thread is in the same place in both.
	 */
	import { BOX_H, BOX_W, CX, CY, RING_R, arcPath, type Layout } from '$lib/inbox/radar';

	let {
		lay,
		active = null,
		onThread,
	}: {
		lay: Layout;
		/** The thread you chose, or `LOOSE`. */
		active?: string | null;
		onThread: (id: string) => void;
	} = $props();

	const OUTER = RING_R[RING_R.length - 1] + 6;
	const lane = $derived(active ? lay.lanes[active] : undefined);
	/** A slice of the dial, as a background for a disc the size of the dial. */
	const sliceOf = (from: number, width: number, fill: string) =>
		`conic-gradient(from ${from}deg, ${fill} 0deg ${width}deg, transparent ${width}deg 360deg)`;
</script>

{#snippet disc(cls: string, background: string)}
	<div
		class="disc {cls}"
		style:left="{CX - OUTER}px"
		style:top="{CY - OUTER}px"
		style:width="{OUTER * 2}px"
		style:height="{OUTER * 2}px"
		style:background
	></div>
{/snippet}

{@render disc('', sliceOf(lay.loose.from, lay.loose.width, 'var(--fill-bg)'))}
{#if lane}
	{@render disc('chosen', sliceOf(lane.center - lane.width / 2, lane.width, 'var(--chosen)'))}
{/if}

<svg class="bands" width={BOX_W} height={BOX_H} viewBox="0 0 {BOX_W} {BOX_H}" aria-hidden="true">
	{#each lay.arcs as a (a.key)}<path d={arcPath(a.from, a.to)} />{/each}
</svg>

{#each lay.rim as r (r.key)}
	<button
		type="button"
		class="rim"
		class:on={active === r.threadId}
		style:left="{r.x}px"
		style:top="{r.y}px"
		style:transform="translate({r.tx}, {r.ty})"
		style:align-items={r.align}
		aria-pressed={active === r.threadId}
		onclick={() => onThread(r.threadId)}
	>
		{#if r.caps}<span class="caps">{r.caps}</span>{/if}
		<span class="name">{r.name}</span>
	</button>
{/each}

<style>
	.disc {
		position: absolute;
		border-radius: 50%;
	}
	.chosen {
		--chosen: color-mix(in srgb, var(--color-primary) 8%, transparent);
	}
	.bands {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}
	.bands path {
		fill: none;
		stroke: var(--color-border-field);
		stroke-width: 1.5;
		stroke-linecap: round;
	}

	/* A thread's name, on the rim of its slice. The thing it sits under is
	   named once for its group, in small capitals. */
	.rim {
		position: absolute;
		display: flex;
		flex-direction: column;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		white-space: nowrap;
		color: var(--color-foreground);
		cursor: pointer;
		transition: color var(--motion-hover-out) var(--ease-premium);
	}
	.rim:hover,
	.rim.on {
		color: var(--color-primary);
	}
	.caps {
		font-size: var(--text-caption);
		line-height: 16px;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--color-foreground-muted);
	}
	.name {
		max-width: 168px;
		overflow: hidden;
		text-overflow: ellipsis;
		font-family: var(--font-serif);
		font-size: var(--text-body);
		line-height: 20px;
	}

	@media (prefers-reduced-motion: reduce) {
		.rim {
			transition: none;
		}
	}
</style>
