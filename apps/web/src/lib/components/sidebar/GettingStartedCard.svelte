<!--
	The progress card: getting started's one mention outside its room. It sits
	at the foot of the rail, above Sources, and opens the room. From step 1 to
	graduation. It is the standing reminder that setup is unfinished — the app
	is not closed off while it is, so this is the only thing saying so.

	It is NOT a callout box. The accent tint and the accent border it used to
	carry are the register of every "onboarding widget" ever shipped, and they
	are what Pemberley refuses: paper, hairlines, one blue — and the blue means
	interactive, not decorative. So the card keeps a standing fill, because it
	has to hold its place in a column of plain rows, but the fill is a neutral
	wash of the foreground rather than a color of its own, and there is no
	border at all. Serif for the name, sans for the state: the same two
	registers the rest of the app sets a title and its metadata in.
-->
<script lang="ts">
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { GETTING_STARTED_CHAT_ID } from "$lib/components/chat/getting-started/getting-started";

	const steps = $derived(gettingStarted.steps);
	const open = $derived(gettingStarted.openCount);
	/* Name the thing, do not count it in words. "1 thing left" sends someone
	   hunting for which; "Introductions" is the answer they were going to have
	   to find anyway. The counting is the folio's job, off at the margin, so
	   the name can stand alone even when several are left. */
	const next = $derived(steps.find((s) => s.status === "open")?.title ?? null);
	const label = $derived(next ?? (open === 1 ? "1 thing left" : `${open} things left`));
	/* A folio, not a tally. Per-step marks were tried and read as stray
	   punctuation: the open step is rarely the last one, so the inked ones
	   came out as "— ——", a typo rather than a measure. A fraction is the
	   printed page's own answer to "where am I in this" — and it is the exact
	   shape the colophon two rows below already sets on this margin. */
	const settled = $derived(steps.filter((s) => s.status !== "open").length);

	function go() {
		windowShellStore.openTabFromRoute(`/chat/${GETTING_STARTED_CHAT_ID}`, {
			label: "Getting started",
			focusExisting: true,
		});
	}
</script>

<div class="slot">
	<button
		type="button"
		class="card"
		onclick={go}
		aria-label={`Getting started — ${settled} of ${steps.length} done, next: ${label}`}
	>
		<span class="title">Getting started</span>
		<span class="meta">
			<span class="next">{label}</span>
			<span class="folio" aria-hidden="true">{settled}/{steps.length}</span>
		</span>
	</button>
</div>

<style>
	/* Left inset, full right bleed: the card's box is the doors' box, so the
	   folio lands on the clock's right edge one row below. There is no rule
	   above it — the fill is the seam now, and a rule over a filled card reads
	   as a lid. */
	.slot {
		margin: 10px 0 4px 8px;
	}

	.card {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 3px;
		width: 100%;
		/* Matches the doors' box exactly (6px 10px inside the footer's 8px
		   inset), so the serif line begins on the same vertical as the door
		   labels' icons rather than 4px adrift of them. */
		padding: 6px 10px;
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		/* The standing fill. Mixed from the foreground rather than named as a
		   color, so it is the same quiet step off the rail in every theme —
		   including the ones where the rail and the page are the same paper
		   and a surface token would leave the card invisible. */
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		text-align: left;
		font: inherit;
		cursor: pointer;
		transition: background-color var(--sidebar-transition-duration)
			var(--sidebar-transition-easing);
	}

	.card:hover {
		background: var(--sidebar-active-bg);
	}

	.card:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	/* One line, always. The old card set the name and the state on a single
	   row, which wrapped "Getting started" onto two lines the moment the state
	   had a word in it — the surest tell that nothing was typeset. */
	.title {
		font-family: var(--font-serif-ui);
		font-size: 14px;
		line-height: 1.25;
		letter-spacing: 0.005em;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.meta {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	/* Sans under the serif — a caption under a heading, which is the pairing
	   the whole app already uses. Mono small-caps was tried here and read as a
	   console stamp: correct for the date at the foot of the rail, too
	   technical for the name of the next thing you have to do. */
	.next {
		font-size: 11px;
		line-height: 1.3;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.folio {
		font-size: 11px;
		line-height: 1.3;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-disabled);
		flex-shrink: 0;
	}

	.card:hover .next {
		color: var(--color-foreground);
	}
</style>
