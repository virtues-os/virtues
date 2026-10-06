<!--
	DayGloss.svelte

	A person's name in a day article, glossed: hover it (or click, or press
	Enter) for a small card of facts about them as of this day. When you first
	messaged, how many of the days before this one you were in touch, and the
	last day before it, which opens that day. Facts are computed from the
	record, never written by the narrator, so the card says only what the
	messages say (`/api/wiki/person/:id/gloss`).

	It keeps the quiet ref look and the `ref-link` class, so the veil hides
	the name like any other.
-->
<script lang="ts">
	import { portal } from "$lib/actions/portal";
	import { FloatingContent, useClickOutside, useEscapeKey } from "$lib/floating";
	import { getPersonGloss, type PersonGlossApi } from "$lib/wiki/api";
	import { parseDateSlug } from "$lib/utils/dateUtils";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { veil } from "$lib/stores/veil.svelte";

	interface Props {
		name: string;
		url: string;
		/** The page's day, `YYYY-MM-DD`. */
		date: string;
		onday?: (slug: string) => void;
	}

	let { name, url, date, onday }: Props = $props();

	const id = $derived(url.replace(/^\/person\//, "").split(/[?#]/)[0]);

	let button: HTMLElement | null = $state(null);
	let card: HTMLElement | null = $state(null);
	let open = $state(false);
	/** Opened by hover, so leaving closes it; a click pins it. */
	let hovered = false;
	let gloss = $state<PersonGlossApi | null>(null);
	let failed = $state(false);

	let showTimer: ReturnType<typeof setTimeout> | undefined;
	let hideTimer: ReturnType<typeof setTimeout> | undefined;

	useClickOutside(() => [card, button], () => (open = false), () => open);
	useEscapeKey(() => {
		open = false;
		button?.focus();
	}, () => open);

	function load() {
		const key = `${id}@${date}`;
		const hit = cache.get(key);
		if (hit) {
			gloss = hit;
			return;
		}
		failed = false;
		getPersonGloss(id, date)
			.then((g) => {
				cache.set(key, g);
				gloss = g;
			})
			.catch(() => (failed = true));
	}

	function show(byHover: boolean) {
		hovered = byHover;
		open = true;
		load();
	}

	function onclick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();
		clearTimeout(showTimer);
		if (open && !hovered) open = false;
		else show(false);
	}

	function onenter(e: PointerEvent) {
		if (e.pointerType !== "mouse") return;
		clearTimeout(hideTimer);
		if (open) return;
		showTimer = setTimeout(() => show(true), 350);
	}

	function onleave(e: PointerEvent) {
		if (e.pointerType !== "mouse") return;
		clearTimeout(showTimer);
		if (open && hovered) hideTimer = setTimeout(() => (open = false), 200);
	}

	const page = $derived(parseDateSlug(date));

	function day(slug: string): string {
		const d = parseDateSlug(slug);
		return d.toLocaleDateString("en-US", {
			month: "short",
			day: "numeric",
			...(d.getFullYear() === page.getFullYear() ? {} : { year: "numeric" }),
		});
	}

	/** "a year before this day" when the first message fell on this date in an earlier year. */
	function anniversary(slug: string): string | null {
		const d = parseDateSlug(slug);
		const years = page.getFullYear() - d.getFullYear();
		if (years < 1 || d.getMonth() !== page.getMonth() || d.getDate() !== page.getDate()) return null;
		return years === 1 ? "a year before this day" : `${years} years before this day`;
	}

	function openPerson() {
		open = false;
		windowShellStore.openRouteBeside(url, name);
	}
</script>

<script lang="ts" module>
	/** Glosses fetched this session, by `person@date`. */
	const cache = new Map<string, PersonGlossApi>();
</script>

<button
	bind:this={button}
	type="button"
	class="ref-link ref-link--quiet day-gloss"
	aria-haspopup="dialog"
	aria-expanded={open}
	{onclick}
	onpointerenter={onenter}
	onpointerleave={onleave}
	>{name}</button
>{#if open && button}
	<div use:portal>
		<FloatingContent anchor={button} options={{ placement: "bottom-start", offset: 6, flip: true, shift: true, padding: 12, strategy: "fixed" }}>
			<div
				class="gloss-card"
				bind:this={card}
				role="dialog"
				aria-label="About {name}"
				tabindex="-1"
				onpointerenter={() => clearTimeout(hideTimer)}
				onpointerleave={(e) => onleave(e)}
			>
				<div class="kick">Person</div>
				{#if veil.hiding}
					<div class="quiet">Veiled. Hold V to read.</div>
				{:else}
					<div class="name">{name}</div>
					{#if failed}
						<div class="quiet">Couldn't load what the record knows. Try again in a moment.</div>
					{:else if !gloss}
						<div class="quiet">Loading…</div>
					{:else if !gloss.first_message_on}
						<div class="quiet">No messages with them before this day.</div>
					{:else}
						<dl>
							<dt>First message</dt>
							<dd>{day(gloss.first_message_on)}{gloss.first_message_in_group ? ", in a group chat" : ""}</dd>
							{#if anniversary(gloss.first_message_on)}
								<dt></dt>
								<dd>{anniversary(gloss.first_message_on)}</dd>
							{/if}
							<dt>Messages</dt>
							<dd>
								on {gloss.days_in_window} of the {gloss.window_days} days before{#if gloss.direct_messages_in_window && gloss.group_messages_in_window}, {gloss.direct_messages_in_window} one-to-one and {gloss.group_messages_in_window} in groups{:else if gloss.group_messages_in_window}, all in group chats{/if}
							</dd>
							{#if gloss.last_message_before_on}
								<dt>Last before this day</dt>
								<dd>
									{#if onday}
										<button type="button" class="link" onclick={() => onday?.(gloss!.last_message_before_on!)}>{day(gloss.last_message_before_on)} →</button>
									{:else}
										{day(gloss.last_message_before_on)}
									{/if}
								</dd>
							{/if}
						</dl>
					{/if}
					<div class="foot"><button type="button" class="link" onclick={openPerson}>Open {name} →</button></div>
				{/if}
			</div>
		</FloatingContent>
	</div>
{/if}

<style>
	/* A gloss is reference, not a link out: the help cursor says "there's a
	   note on this", and the dotted line is the quiet ref's own. */
	.day-gloss {
		cursor: help;
	}

	.gloss-card {
		width: min(18rem, calc(100vw - 32px));
		padding: 0.75rem 0.875rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.gloss-card:focus {
		outline: none;
	}

	.kick {
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.name {
		margin: 0.125rem 0 0.5rem;
		font-family: var(--font-serif);
		font-size: 1.125rem;
		color: var(--color-foreground);
	}

	dl {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.3rem 0.75rem;
		margin: 0;
	}

	dt {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	dd {
		margin: 0;
		color: var(--color-foreground);
		font-variant-numeric: tabular-nums;
	}

	.quiet {
		margin: 0.25rem 0 0;
		color: var(--color-foreground-subtle);
	}

	.foot {
		margin: 0.75rem 0 0;
		padding-top: 0.625rem;
		border-top: 1px solid var(--color-border-subtle);
	}

	.link {
		font: inherit;
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
	}

	.link:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}
</style>
