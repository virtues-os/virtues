<!--
	AppletNode: an applet's face in a block page.

	Mounted once the block scrolls near the screen (the node view is lazy), so
	a page with a dozen applets loads one frame per applet the reader reaches,
	not all of them. The face is `FaceFrame`, the sandboxed frame every face
	runs in. The handle under it sets the frame's height and writes `height`
	once, when the drag ends; it is a slider too, which Tab reaches and the
	arrows move (lib/document/resize.ts), and the menu sets a height without
	either. Prints as a bordered box naming the applet: a frame does not
	print.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import FaceFrame from "$lib/components/applets/FaceFrame.svelte";
	import { onContextGesture } from "$lib/codemirror/extensions/long-press";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { getApplet } from "$lib/api/client";
	import { DEFAULT_APPLET_HEIGHT } from "$lib/document/applet";
	import { dragToResize, sizeByKey } from "$lib/document/resize";
	import type { ViewState } from "$lib/document/nodeviews.svelte";

	let { view }: { view: ViewState } = $props();

	/** The shortest a frame is dragged to. */
	const MIN_HEIGHT = 120;
	/** The tallest the keys and the menu set it to. */
	const MAX_HEIGHT = 1200;

	const ref = $derived(String(view.attrs.ref ?? ""));
	const height = $derived(typeof view.attrs.height === "number" ? view.attrs.height : DEFAULT_APPLET_HEIGHT);

	let name = $state<string | null>(null);
	let dragHeight = $state<number | null>(null);
	let el: HTMLElement;

	$effect(() => {
		const id = ref;
		name = null;
		getApplet(id)
			.then((a) => {
				if (id === ref) name = a.name;
			})
			.catch(() => {});
	});

	function startResize(e: PointerEvent) {
		if (!view.editable) return;
		dragToResize(e.currentTarget as HTMLElement, e, {
			axis: "y",
			start: height,
			min: MIN_HEIGHT,
			max: Infinity,
			onSize: (size) => (dragHeight = size),
			onEnd: (size) => {
				if (size !== null && size !== height) view.setAttrs({ height: size });
				dragHeight = null;
			},
		});
	}

	function resizeByKey(e: KeyboardEvent) {
		if (!view.editable) return;
		const next = sizeByKey(e.key, height, { min: MIN_HEIGHT, max: Math.max(MAX_HEIGHT, height), step: 20, page: 200 });
		if (next === null) return;
		e.preventDefault();
		e.stopPropagation();
		if (next !== height) view.setAttrs({ height: next });
	}

	onMount(() => {
		if (!view.editable) return;
		onContextGesture(el, (x, y) =>
			contextMenu.show({ x, y }, [
				{
					id: "height-short",
					label: "Short (240px)",
					icon: "ri:contract-up-down-line",
					action: () => view.setAttrs({ height: 240 }),
				},
				{
					id: "height-tall",
					label: "Tall (480px)",
					icon: "ri:expand-up-down-line",
					action: () => view.setAttrs({ height: 480 }),
				},
				{
					id: "remove",
					label: "Remove applet",
					icon: "ri:delete-bin-line",
					variant: "destructive",
					dividerBefore: true,
					action: () => view.remove(),
				},
			]),
		);
	});
</script>

<figure bind:this={el} class="doc-applet" class:selected={view.selected}>
	<figcaption class="doc-applet-name">
		<Icon icon="ri:apps-2-line" width="14" />
		<span>{name ?? ref}</span>
	</figcaption>
	<div class="doc-applet-face">
		<FaceFrame appletId={ref} height={`${dragHeight ?? height}px`} />
	</div>
	{#if view.editable}
		<div
			class="doc-applet-handle"
			role="slider"
			tabindex="0"
			aria-label="Applet height"
			aria-orientation="vertical"
			aria-valuemin={MIN_HEIGHT}
			aria-valuemax={Math.max(MAX_HEIGHT, height)}
			aria-valuenow={dragHeight ?? height}
			aria-valuetext={`${dragHeight ?? height} px`}
			data-widget-control
			onpointerdown={startResize}
			onkeydown={resizeByKey}
		></div>
	{/if}
</figure>

<style>
	.doc-applet {
		margin: 0;
		padding: 8px 0;
	}

	.doc-applet.selected .doc-applet-face {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
		border-radius: 12px;
	}

	.doc-applet-name {
		display: flex;
		align-items: center;
		gap: 6px;
		padding-bottom: 4px;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	.doc-applet-handle {
		height: 8px;
		margin: 4px auto 0;
		width: 48px;
		border-radius: 9999px;
		background: var(--color-border);
		cursor: ns-resize;
		touch-action: none;
		opacity: 0;
	}

	.doc-applet:hover .doc-applet-handle,
	.doc-applet.selected .doc-applet-handle,
	.doc-applet-handle:focus-visible {
		opacity: 1;
	}

	.doc-applet-handle:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}

	/* On touch the handle takes the pointer only on a selected applet: a
	   thumb that begins a scroll over it scrolls, and writes no height. Its
	   hit area is a thumb's 44pt, grown around the 8px bar. */
	@media (pointer: coarse) {
		.doc-applet-handle {
			position: relative;
			pointer-events: none;
			touch-action: auto;
		}

		.doc-applet.selected .doc-applet-handle {
			pointer-events: auto;
			touch-action: none;
		}

		.doc-applet-handle::after {
			content: "";
			position: absolute;
			top: 50%;
			left: 50%;
			width: 48px;
			height: 44px;
			transform: translate(-50%, -50%);
		}
	}

	@media print {
		.doc-applet {
			break-inside: avoid;
			padding: 12px;
			border: 1px solid var(--color-border);
			border-radius: 12px;
		}

		.doc-applet-face,
		.doc-applet-handle {
			display: none;
		}

		.doc-applet-name {
			padding: 0;
			font-size: 0.875rem;
			color: var(--color-foreground);
		}
	}
</style>
