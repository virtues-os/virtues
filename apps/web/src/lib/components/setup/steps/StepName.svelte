<!--
	Names, the first two beats: its name, then yours. (Home follows, in
	StepLocation; StepNames runs the three.)

	ONE SHAPE, TWICE (2026-09-25). The two beats used to be two different
	designs: "Meet" over the assistant's name set enormous as the field, then
	a two-line headline over a small underlined box for yours. They are one
	exchange, so they are now one layout, and the field never moves:

	  a small serif line    "Meet"            "And what should Sol call you?"
	  the name, enormous    Sol               Nick
	  one line of help      what the name is  what to put
	  the way on            "Keep Sol"        "Call me Nick"

	The name IS the field, with no box: typing over it replaces it, the old
	name dissolving into its own ink (vanish.ts, after Rauno Freiberg's
	vanishing input). Leaving either beat does the same: the name that was
	just settled dissolves, and the next line fades up over the empty field.
	(The first version replaced a noise-displacement "ink settle", 2026-09-24:
	Adam, "the curvy canvas weird thing".)

	Your name is asked in the assistant's name because the assistant's name
	is the one the whole product answers to (Setup's "one name",
	agents/build/onboarding.md). Full name lives in Settings; birth date on
	the timeline, where it is the left edge.

	THE PRESENCE is the mark at the top: each keystroke runs a ripple
	through its three dots (`setup.hear`, SetupMark). It had its own ∴ over
	the question until 2026-09-28, which stacked a second logo under the
	progress mark on every screen of the step.

	Both write as they commit (assistant profile, then profile), so going
	back shows both names as they were left.
-->
<script lang="ts">
	import { onMount, tick } from "svelte";
	import { useShowing, mayTakeFocus } from "../showing";
	import { fade } from "svelte/transition";
	import Icon from "$lib/components/Icon.svelte";
	import { updateAssistantProfile, updateProfile } from "$lib/api/client";
	import { inComposition } from "$lib/utils/ime";
	import { setup } from "../setup.svelte";
	import { vanish } from "../vanish";

	let { eyebrow, onnext }: { eyebrow?: string; onnext: () => void } = $props();

	let beat = $state<1 | 2>(1);
	/** What the field holds, and what it opened with on this beat. */
	let text = $state(setup.assistantName);
	let original = $state(setup.assistantName);
	/** The assistant's name as settled, for the question and for Back. */
	let assistant = $state(setup.assistantName);
	let busy = $state(false);
	/** A settled name is dissolving: the field shows neither it nor a placeholder. */
	let leaving = $state(false);
	/** The button keeps saying what it did while the name dissolves. */
	let frozenLabel = $state<string | null>(null);
	let error = $state<string | null>(null);

	let field = $state<HTMLInputElement | null>(null);
	let mirror = $state<HTMLSpanElement | null>(null);
	let width = $state(0);

	const still =
		typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

	const trimmed = $derived(text.trim());
	const changed = $derived(trimmed !== original.trim());
	const placeholder = $derived(beat === 1 ? "Ari" : "Your name");
	const goLabel = $derived(
		beat === 1
			? !trimmed
				? "Name your assistant"
				: changed
					? `Call it ${trimmed}`
					: `Keep ${trimmed}`
			: trimmed
				? `Call me ${trimmed}`
				: "Enter your name",
	);

	/** The cursor at the end, not the name selected: a selected name sits in
	 *  a grey block, which on a word this size reads as a broken field. */
	async function focusEnd() {
		await tick();
		if (!mayTakeFocus(showing, field)) return;
		field?.focus();
		const end = field?.value.length ?? 0;
		field?.setSelectionRange(end, end);
	}

	onMount(async () => {
		// The store may still be reading when the step mounts.
		if (!setup.loaded) await setup.refresh();
		assistant = setup.assistantName;
		text = original = assistant;
		await focusEnd();
	});

	// The field is as wide as its text, so the name stays centered as it is
	// typed. A hidden span in the same type measures it, again once the serif
	// has loaded: a measure taken in the fallback face is narrower and clips.
	const showing = useShowing();
	let fontsReady = $state(false);
	onMount(() => {
		void document.fonts?.ready.then(() => (fontsReady = true));
	});
	$effect(() => {
		void text;
		void placeholder;
		void fontsReady;
		if (mirror) width = mirror.getBoundingClientRect().width;
	});

	/** A long name shrinks to fit the space it has instead of running off it:
	 *  the measure stays at full size, and the field scales down from it. The
	 *  space is the step's own width, not the window's, because in the app
	 *  Setup runs in a pane beside the sidebar. */
	let stageWidth = $state(0);
	const room = $derived(
		(stageWidth || (typeof window === "undefined" ? 1024 : window.innerWidth)) - 32,
	);
	const fit = $derived(Math.max(0.35, Math.min(1, room / (Math.max(width, 40) + 8))));

	/**
	 * Typing over the name replaces it. While the field still holds the name
	 * it opened with, the first printable key takes the whole name's place
	 * and the old one dissolves; after that the field edits normally. Arrow
	 * keys, Backspace and shortcuts keep their usual meaning, and a caret
	 * inside the name means editing it, not replacing it.
	 */
	function onKey(e: KeyboardEvent) {
		if (!field || e.metaKey || e.ctrlKey || e.altKey || inComposition(e)) return;
		if (e.key.length !== 1 || text !== original || !text) return;
		const a = field.selectionStart ?? 0;
		const b = field.selectionEnd ?? 0;
		if (a === b && b !== text.length) return;
		e.preventDefault();
		void vanish(field, text);
		text = e.key;
	}

	/**
	 * THE FIRST CLICK ON AN UNTOUCHED NAME SELECTS ALL OF IT, so typing
	 * replaces it (and vanishes it). A click on "Ari" always lands mid-word,
	 * where `onKey` reads a collapsed caret as editing and inserts: renaming
	 * produced "ANickri" (2026-09-28). The rename field's usual manners: one
	 * click selects the name, and every click after places the caret.
	 */
	let claimed = false;
	function onPoint() {
		if (!field || claimed || text !== original || !text) return;
		claimed = true;
		requestAnimationFrame(() => field?.select());
	}

	/** The settled name dissolves; the field is empty when this resolves. */
	async function dissolve() {
		if (!field || !trimmed) return;
		leaving = true;
		frozenLabel = goLabel;
		const gone = vanish(field, text);
		text = "";
		await gone;
		leaving = false;
		frozenLabel = null;
	}

	/**
	 * Save, then move on. Only the save can fail the step: the dissolve and
	 * the refresh after it are the page's own business, and when they sat in
	 * the same `try` a hiccup in either read as "couldn't save", with the name
	 * already saved.
	 */
	async function go() {
		if (!trimmed || busy) return;
		busy = true;
		error = null;
		try {
			if (beat === 1) {
				if (changed) await updateAssistantProfile({ assistant_name: trimmed });
			} else {
				await updateProfile({ preferred_name: trimmed });
			}
		} catch (e) {
			console.error("[setup] saving a name failed", e);
			error =
				beat === 1 ? "Your server couldn't save that name. Try again." : "Your server couldn't save your name. Try again.";
			busy = false;
			return;
		}
		const settle = async (what: string, run: () => Promise<unknown>) => {
			try {
				await run();
			} catch (e) {
				console.error(`[setup] after saving a name, ${what} failed`, e);
			}
		};
		if (beat === 1) {
			assistant = trimmed;
			setup.assistantName = trimmed;
			await settle("the dissolve", dissolve);
			leaving = false;
			frozenLabel = null;
			beat = 2;
			claimed = false;
			text = original = setup.profile?.preferred_name ?? "";
			await focusEnd();
			busy = false;
		} else {
			await settle("the dissolve", dissolve);
			leaving = false;
			frozenLabel = null;
			await settle("the refresh", () => setup.refresh());
			onnext();
		}
	}

	async function back() {
		beat = 1;
		error = null;
		text = original = assistant;
		await focusEnd();
	}
</script>

<div class="stage" bind:clientWidth={stageWidth}>
	{#if eyebrow}<p class="eyebrow">{eyebrow}</p>{/if}


	{#key beat}
		<p class="lead" in:fade={{ duration: still ? 0 : 420, delay: still ? 0 : 120 }}>
			{beat === 1 ? "Meet" : `And what should ${assistant} call you?`}
		</p>
	{/key}

	<form
		class="naming"
		onsubmit={(e) => {
			e.preventDefault();
			void go();
		}}
	>
		<span class="mirror" bind:this={mirror} aria-hidden="true">{text || placeholder}</span>
		<input
			bind:this={field}
			bind:value={text}
			onkeydown={onKey}
			onpointerup={onPoint}
			oninput={() => setup.hear()}
			class="name"
			class:leaving
			style:width="{Math.max(width, 40) * fit + 8}px"
			style:font-size={fit < 1 ? `calc(clamp(4.5rem, 13vw, 10rem) * ${fit})` : undefined}
			aria-label={beat === 1 ? "Your assistant's name" : "What your assistant should call you"}
			autocomplete={beat === 1 ? "off" : "given-name"}
			spellcheck="false"
			maxlength={beat === 1 ? 32 : 48}
			{placeholder}
		/>
	</form>

	{#key beat}
		<div class="after" in:fade={{ duration: still ? 0 : 420, delay: still ? 0 : 200 }}>
			<p class="subtitle">
				{beat === 1
					? "This is the name your assistant answers to. Keep it, or give it a new one."
					: "Your first name, or whatever you go by."}
			</p>
			<div class="actions">
				<button type="button" class="setup-go" disabled={!trimmed || busy} onclick={go}>
					{frozenLabel ?? goLabel}
					<Icon icon="ri:arrow-right-line" width="16" />
				</button>
				{#if beat === 2}
					<button type="button" class="setup-past" disabled={busy} onclick={back}>Back</button>
				{/if}
			</div>
		</div>
	{/key}

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}
</div>

<style>
	.stage {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		/* From the top, where every step starts (StepFrame's padding), not
		   centered: Names sat mid-screen and home at the top, so the page
		   jumped between the beats of one step. */
		justify-content: flex-start;
		min-height: 100%;
		padding: clamp(2.5rem, 8vh, 5.5rem) 16px 4rem;
		text-align: center;
		position: relative;
	}

	.eyebrow {
		position: absolute;
		top: clamp(1.5rem, 5vh, 3rem);
		left: 0;
		right: 0;
		margin: 0;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}



	.after {
		display: flex;
		flex-direction: column;
		align-items: center;
	}

	/* Two lines of room whatever it says, standing on its last line, so the
	   name under it sits in the same place for "Meet" and for the question. */
	.lead {
		margin: 0;
		max-width: 30ch;
		min-height: 2.5em;
		display: flex;
		align-items: flex-end;
		justify-content: center;
		line-height: 1.25;
		text-wrap: balance;
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: clamp(1.5rem, 3vw, 2.1rem);
		color: var(--color-foreground-muted);
	}

	.naming {
		position: relative;
		display: grid;
		place-items: center;
		margin: 0.2em 0 0;
		max-width: 100%;
	}

	/* The field and its measure share one type, so the width is exact. */
	.name,
	.mirror {
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: clamp(4.5rem, 13vw, 10rem);
		line-height: 1.05;
		letter-spacing: -0.02em;
	}
	.mirror {
		position: absolute;
		visibility: hidden;
		white-space: pre;
		pointer-events: none;
	}
	.name {
		position: relative;
		z-index: 1;
		max-width: 100%;
		padding: 0;
		border: none;
		outline: none;
		background: transparent;
		text-align: center;
		color: var(--color-foreground);
		caret-color: var(--color-primary);
	}
	.name::placeholder {
		color: var(--color-foreground-subtle);
		opacity: 0.55;
	}
	/* A settled name dissolving: its ink is on the vanish canvas now. */
	.name.leaving::placeholder {
		color: transparent;
	}
	.name::selection {
		background: color-mix(in srgb, var(--color-primary) 18%, transparent);
	}
	/* A hairline under the name while it is being edited: the only sign it
	   is a field at all. */
	.naming::after {
		content: "";
		position: absolute;
		left: 50%;
		bottom: 0.18em;
		width: 2.2em;
		height: 1px;
		transform: translateX(-50%);
		background: color-mix(in srgb, var(--color-primary) 55%, transparent);
		opacity: 0;
		transition: opacity 0.3s ease;
		font-size: clamp(1.5rem, 3vw, 2.1rem);
	}
	.naming:focus-within::after {
		opacity: 1;
	}

	/* Two lines of room too, so a help line that wraps on a phone does not
	   lift the name above it. */
	.subtitle {
		margin: 1.1rem 0 0;
		max-width: 30rem;
		min-height: 3.1em;
		font-size: 1rem;
		line-height: 1.55;
		color: var(--color-foreground-muted);
		text-wrap: balance;
	}



	.actions {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-wrap: wrap;
		gap: 1.25rem;
		margin-top: 2.5rem;
	}

	.error {
		margin: 1.25rem 0 0;
		font-size: 13px;
		color: var(--color-error);
	}
</style>
