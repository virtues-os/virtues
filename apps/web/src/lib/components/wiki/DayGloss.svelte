<!--
	DayGloss.svelte

	A person's name in a day article, glossed: hover it (or click, or press
	Enter) for a small card of facts about them as of this day. How you know
	them, when you wrote it down; on how many of the 31 days before this one
	there were messages (one-to-one in either direction; group chats only
	where they wrote), as a dot a day; how many; the earliest message between
	you on record; and the last day before it, which opens that day. Facts are
	computed from the record, never written by the narrator
	(`/api/wiki/person/:id/gloss`). Its anatomy is the evidence card's
	(DayEvidence): a kicker line, the body, one row of actions.

	It keeps the quiet ref look and the `ref-link` class, so the veil hides
	the name like any other.
-->
<script lang="ts">
	import { portal } from "$lib/actions/portal";
	import { FloatingContent, useClickOutside, useEscapeKey } from "$lib/floating";
	import { getPersonGloss, type PersonGlossApi } from "$lib/wiki/api";
	import { ApiError } from "$lib/api/client";
	import { tick } from "svelte";
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
	/** The person is no longer in the wiki (merged or removed) though the page still names them. */
	let missing = $state(false);

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
		missing = false;
		getPersonGloss(id, date)
			.then((g) => {
				cache.set(key, g);
				gloss = g;
			})
			.catch((e) => {
				if (e instanceof ApiError && e.status === 404) missing = true;
				else failed = true;
			});
	}

	function show(byHover: boolean) {
		hovered = byHover;
		open = true;
		load();
		// Opened on purpose, the card takes focus so its links are the next Tab;
		// a hover never moves focus away from where the reader is.
		if (!byHover) void tick().then(() => card?.focus({ preventScroll: true }));
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

	/** On how many of the days before there were messages, in words. */
	function lede(g: PersonGlossApi): string {
		const n = g.days_in_window;
		const days = `the ${g.window_days} days before`;
		if (n === 0) return `No messages in ${days}`;
		const on = n === g.window_days ? `Messages on all ${days}` : `Messages on ${n} of ${days}`;
		return g.direct_messages_in_window === 0 ? `${on}, all from them in group chats` : on;
	}

	const firstName = $derived(name.split(/\s+/)[0] || name);

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
				<p class="kick"><span class="kind">Person</span><span>· as of {day(date)}</span></p>
				{#if veil.hiding}
					<p class="quiet">Veiled. Hold V to read.</p>
				{:else}
					<p class="name">{name}</p>
					{#if gloss?.relationship}<p class="rel">{gloss.relationship}</p>{/if}
					{#if missing}
						<p class="quiet">{name} isn't in your wiki anymore.</p>
					{:else if failed}
						<p class="quiet">Couldn't load your messages with them. Close this card and open it again to retry.</p>
					{:else if !gloss}
						<p class="quiet">Loading…</p>
					{:else if !gloss.first_message_on}
						<p class="quiet">No messages with them before this day.</p>
					{:else}
						<p class="lede">{lede(gloss)}</p>
						{#if gloss.days?.length}
							<div class="dots" aria-hidden="true">
								{#each gloss.days as on, i (i)}<i class:on></i>{/each}
							</div>
						{/if}
						<dl>
							<div class="fact">
								<dt>Messages that month</dt>
								<dd>{(gloss.direct_messages_in_window + gloss.group_messages_in_window).toLocaleString("en-US")}</dd>
							</div>
							<div class="fact">
								<dt>First on record</dt>
								<dd>{day(gloss.first_message_on)}{gloss.first_message_in_group ? ", in a group" : ""}</dd>
							</div>
							{#if gloss.last_message_before_on}
								<div class="fact">
									<dt>Last before this day</dt>
									<dd>
										{#if onday}
											<button type="button" class="link" onclick={() => onday?.(gloss!.last_message_before_on!)}>{day(gloss.last_message_before_on)} →</button>
										{:else}
											{day(gloss.last_message_before_on)}
										{/if}
									</dd>
								</div>
							{/if}
						</dl>
					{/if}
					{#if !missing}<div class="acts"><button type="button" class="link" onclick={openPerson}>Open {firstName}</button></div>{/if}
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
		width: min(20rem, calc(100vw - 32px));
		padding: 0.8rem 0.95rem 0.7rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.gloss-card:focus {
		outline: none;
	}

	.kick {
		display: flex;
		flex-wrap: wrap;
		gap: 0 0.35rem;
		margin: 0 0 0.45rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.kick .kind {
		font-weight: 500;
		color: var(--color-foreground-muted);
	}

	.name {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 1.375rem;
		line-height: 1.2;
		color: var(--color-foreground);
	}

	.rel {
		margin: 0;
		color: var(--color-foreground-muted);
	}

	.lede {
		margin: 0.6rem 0 0;
	}

	/* A dot a day, oldest first; a filled one had messages. */
	.dots {
		display: grid;
		grid-template-columns: repeat(31, 1fr);
		gap: 2px;
		margin: 0.35rem 0 0.4rem;
	}

	.dots i {
		aspect-ratio: 1;
		border-radius: 50%;
		background: var(--color-border-strong);
	}

	.dots i.on {
		background: var(--color-foreground-muted);
	}

	dl {
		margin: 0;
	}

	.fact {
		display: flex;
		justify-content: space-between;
		gap: 0.75rem;
		padding: 0.18rem 0;
	}

	dt {
		color: var(--color-foreground-subtle);
	}

	dd {
		margin: 0;
		color: var(--color-foreground);
		font-variant-numeric: tabular-nums;
	}

	.quiet {
		margin: 0.375rem 0 0;
		color: var(--color-foreground-subtle);
	}

	.acts {
		margin-top: 0.6rem;
		padding-top: 0.55rem;
		border-top: 1px solid var(--color-border-subtle);
		font-size: 0.75rem;
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
