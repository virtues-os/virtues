<!--
	FigureNode: an article's figure in a block page, a block atom. The day
	page draws each kind in full (`DayFigure`, which loads the record and the
	day's route); here it is a still card that fetches nothing, so opening a
	page asks the server for no figure's record.

	- quote: the words, with who said them and when;
	- thread: a line saying it is messages from one thread;
	- route: a line saying the day page draws it;
	- picture: the picture, through `backendUrl`, and its caption.

	A quote's words, whole, a named speaker and a caption's marked phrases
	go under the device's veil, as the day page veils them. It is selected
	and dragged whole; a right-click or a long press offers Remove.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { backendUrl } from "$lib/config/backend";
	import { onContextGesture } from "$lib/codemirror/extensions/long-press";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { veiled } from "$lib/actions/veil";
	import { veil } from "$lib/stores/veil.svelte";
	import { veilMarks } from "$lib/wiki/dayArticle";
	import { mediaSrc } from "$lib/document/media";
	import type { ViewState } from "$lib/document/nodeviews.svelte";

	let { view }: { view: ViewState } = $props();

	/** One of the figure's values, empty when it holds none. */
	function field(key: string): string {
		const value = view.attrs[key];
		return typeof value === "string" ? value : "";
	}

	const kind = $derived(field("kind"));
	const quote = $derived(veilMarks(field("text"), "text"));
	const caption = $derived(veilMarks(field("caption"), "text"));
	const who = $derived(field("who"));
	const time = $derived(field("time"));
	const src = $derived(field("src"));
	/** Attributions that name nobody; any other `who` is a person's name. */
	const UNNAMED = new Set(["You", "In a recording", "In a message"]);
	const made = $derived(field("style") === "pencil" ? "Drawn from the record" : "Painted from the record");

	let el: HTMLElement;

	onMount(() => {
		onContextGesture(el, (x, y) => {
			if (!view.editable) return;
			contextMenu.show({ x, y }, [
				{
					id: "remove",
					label: "Remove",
					icon: "ri:delete-bin-line",
					variant: "destructive" as const,
					action: () => view.remove(),
				},
			]);
		});
	});
</script>

<figure bind:this={el} class="doc-figure" class:selected={view.selected} data-kind={kind}>
	{#if kind === "quote"}
		<!-- The veil hides the words whole, marked or not, as the day page's
		     DayFigure does: a writer's quote carries no ⟦ ⟧. -->
		<q use:veiled={{ hiding: veil.hiding, whole: true }}>{quote.markdown}</q>
		{#if who || time}
			<figcaption use:veiled={{ hiding: veil.hiding && !!who && !UNNAMED.has(who), whole: true }}>
				{[who, time].filter(Boolean).join(" · ")}
			</figcaption>
		{/if}
	{:else if kind === "thread"}
		<figcaption>Messages from one thread, shown on the day page</figcaption>
	{:else if kind === "route"}
		<figcaption>The day's route, drawn on the day page</figcaption>
	{:else if kind === "picture"}
		{#if src}
			<!-- With names hidden, the alt names nothing either. -->
			<img src={backendUrl(mediaSrc(src))} alt={veil.hiding ? made : caption.markdown} loading="lazy" draggable="false" />
		{/if}
		<figcaption>
			<span class="kicker">{made}</span>
			{#if caption.markdown}
				<span use:veiled={{ hiding: veil.hiding, phrases: caption.phrases }}>{caption.markdown}</span>
			{/if}
		</figcaption>
	{/if}
</figure>

<style>
	.doc-figure {
		margin: 0;
		padding: 12px 16px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		color: var(--color-foreground);
	}

	.doc-figure.selected {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}

	q {
		display: block;
		font-family: var(--md-heading-major-family);
		font-size: var(--md-h3-size);
		line-height: var(--md-h3-lh);
		quotes: "\201C" "\201D";
	}

	figcaption {
		margin-top: 8px;
		font-family: var(--md-body-family);
		font-size: 0.8125rem;
		line-height: var(--md-body-lh);
		color: var(--color-foreground-muted);
	}

	[data-kind="thread"] figcaption,
	[data-kind="route"] figcaption {
		margin-top: 0;
	}

	img {
		display: block;
		max-width: 100%;
		border-radius: 6px;
	}

	.kicker {
		display: block;
	}

	@media print {
		.doc-figure {
			break-inside: avoid;
		}

		.doc-figure.selected {
			outline: none;
		}
	}
</style>
