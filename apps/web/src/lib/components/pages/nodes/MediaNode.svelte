<!--
	MediaNode: an image, audio, video or file block in a block page, at
	parity with the CodeMirror editor's media widgets (media-widgets.ts).

	- Image: drawn at its `width`; the handle on its right edge resizes it and
	  writes `width` once, when the drag ends. The handle is a slider too: Tab
	  reaches it and the arrows resize it (lib/document/resize.ts). An image
	  that fails to load shows as a file card here, without changing the page.
	- Audio: its name over a player. Video: a player. File: a card with its
	  icon, name and extension, opening the file.
	- Right-click or a long press: Open, Copy link, Edit (the name, or an
	  image's description, and the address), Turn into link, the image widths,
	  Remove.
	- Print: an image prints; audio, video and a file print as their card.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { backendUrl } from "$lib/config/backend";
	import { onContextGesture } from "$lib/codemirror/extensions/long-press";
	import { contextMenu, type ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { linkEditor } from "$lib/stores/linkEditor.svelte";
	import { fileIcon } from "$lib/utils/refRoutes";
	import { mediaSrc, nameOfSrc } from "$lib/document/media";
	import { shareableHref } from "$lib/document/links";
	import { dragToResize, sizeByKey } from "$lib/document/resize";
	import type { ViewState } from "$lib/document/nodeviews.svelte";

	let { view }: { view: ViewState } = $props();

	/** The narrowest an image is dragged to. */
	const MIN_WIDTH = 48;

	const src = $derived(String(view.attrs.src ?? ""));
	const href = $derived(backendUrl(mediaSrc(src)));
	const isImage = $derived(view.type === "image");
	/** What the block is called: an image's description, a file's name, else its address's last part. */
	const name = $derived.by(() => {
		const own = String((isImage ? view.attrs.alt : view.attrs.name) ?? "");
		return own || nameOfSrc(src);
	});
	const ext = $derived.by(() => {
		const dot = name.lastIndexOf(".");
		return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
	});
	const width = $derived(typeof view.attrs.width === "number" ? view.attrs.width : null);

	let failed = $state(false);
	let dragWidth = $state<number | null>(null);
	let el: HTMLElement;
	let img = $state<HTMLImageElement | null>(null);

	// A new address gets a fresh try.
	$effect(() => {
		void src;
		failed = false;
	});

	function startResize(e: PointerEvent) {
		if (!img || !view.editable) return;
		dragToResize(e.currentTarget as HTMLElement, e, {
			axis: "x",
			start: img.getBoundingClientRect().width,
			min: MIN_WIDTH,
			max: el.getBoundingClientRect().width,
			onSize: (size) => (dragWidth = size),
			onEnd: (size) => {
				if (size !== null && size !== width) view.setAttrs({ width: size });
				dragWidth = null;
			},
		});
	}

	function resizeByKey(e: KeyboardEvent) {
		if (!img || !view.editable) return;
		const now = width ?? Math.round(img.getBoundingClientRect().width);
		const max = Math.max(MIN_WIDTH, Math.round(el.getBoundingClientRect().width) || now);
		const next = sizeByKey(e.key, now, { min: MIN_WIDTH, max, step: 16, page: 160 });
		if (next === null) return;
		e.preventDefault();
		e.stopPropagation();
		if (next !== width) view.setAttrs({ width: next });
	}

	function edit(x: number, y: number) {
		linkEditor.show(
			{ label: name, href: src },
			({ label, href: next }) => view.setAttrs(isImage ? { alt: label, src: next } : { name: label, src: next }),
			{ x, y, width: 0, height: 0 },
		);
	}

	function turnIntoLink() {
		const { schema } = view.editor;
		const text = schema.text(name, [schema.marks.link.create({ href: src })]);
		view.replaceWith(schema.nodes.paragraph.create(null, text));
	}

	function menu(x: number, y: number): ContextMenuItem[] {
		const items: ContextMenuItem[] = [
			{
				id: "open",
				label: "Open",
				icon: "ri:arrow-right-up-line",
				action: () => void window.open(href, "_blank", "noopener"),
			},
			{
				id: "copy-link",
				label: "Copy link",
				icon: "ri:file-copy-line",
				action: () => void navigator.clipboard?.writeText(shareableHref(src)),
			},
		];
		if (!view.editable) return items;
		items.push(
			{ id: "edit", label: "Edit", icon: "ri:edit-line", dividerBefore: true, action: () => edit(x, y) },
			{ id: "turn-into-link", label: "Turn into link", icon: "ri:link", action: turnIntoLink },
		);
		if (isImage) {
			items.push(
				{
					id: "width-small",
					label: "Small (320px)",
					icon: "ri:contract-left-right-line",
					dividerBefore: true,
					action: () => view.setAttrs({ width: 320 }),
				},
				{ id: "width-medium", label: "Medium (600px)", icon: "ri:pause-line", action: () => view.setAttrs({ width: 600 }) },
				{
					id: "width-original",
					label: "Original size",
					icon: "ri:expand-left-right-line",
					action: () => view.setAttrs({ width: null }),
				},
			);
		}
		items.push({
			id: "remove",
			label: "Remove",
			icon: "ri:delete-bin-line",
			variant: "destructive",
			dividerBefore: true,
			action: () => view.remove(),
		});
		return items;
	}

	onMount(() => {
		onContextGesture(el, (x, y) => contextMenu.show({ x, y }, menu(x, y)));
	});
</script>

{#snippet card()}
	<a class="doc-file-card" {href} target="_blank" rel="noopener" data-widget-control>
		<Icon icon={fileIcon({ filename: name })} width="20" />
		<span class="doc-file-info">
			<span class="doc-file-name">{name}</span>
			{#if ext}<span class="doc-file-ext">{ext.toUpperCase()}</span>{/if}
		</span>
	</a>
{/snippet}

<figure bind:this={el} class="doc-media doc-media-{view.type}" class:selected={view.selected}>
	{#if isImage && !failed}
		<span class="doc-image-frame" style:width={dragWidth !== null ? `${dragWidth}px` : width ? `${width}px` : null}>
			<img
				bind:this={img}
				src={href}
				alt={String(view.attrs.alt ?? "")}
				loading="lazy"
				draggable="false"
				onerror={() => (failed = true)}
			/>
			{#if view.editable}
				<span
					class="doc-image-handle"
					role="slider"
					tabindex="0"
					aria-label="Image width"
					aria-orientation="horizontal"
					aria-valuemin={MIN_WIDTH}
					aria-valuenow={dragWidth ?? width ?? undefined}
					aria-valuetext={dragWidth ?? width ? `${dragWidth ?? width} px` : "Original size"}
					data-widget-control
					onpointerdown={startResize}
					onkeydown={resizeByKey}
				></span>
			{/if}
		</span>
	{:else if view.type === "audio"}
		<div class="doc-audio">
			<div class="doc-audio-head">
				<Icon icon="ri:music-2-line" width="16" />
				<span class="doc-file-name">{name}</span>
			</div>
			<audio controls preload="metadata" src={href}></audio>
		</div>
		<div class="doc-print-card">{@render card()}</div>
	{:else if view.type === "video"}
		<!-- svelte-ignore a11y_media_has_caption -->
		<video controls preload="metadata" src={href}></video>
		<div class="doc-print-card">{@render card()}</div>
	{:else}
		{@render card()}
	{/if}
</figure>

<style>
	.doc-media {
		margin: 0;
		padding: 8px 0;
	}

	.doc-media.selected .doc-image-frame,
	.doc-media.selected .doc-file-card,
	.doc-media.selected .doc-audio,
	.doc-media.selected video {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.doc-image-frame {
		position: relative;
		display: inline-block;
		max-width: 100%;
		vertical-align: top;
	}

	.doc-image-frame img {
		display: block;
		width: 100%;
		max-width: 100%;
		height: auto;
		border-radius: 6px;
	}

	.doc-image-handle {
		position: absolute;
		top: 50%;
		right: -4px;
		width: 8px;
		height: 48px;
		transform: translateY(-50%);
		border-radius: 9999px;
		background: var(--color-foreground-muted);
		opacity: 0;
		cursor: ew-resize;
		touch-action: none;
	}

	.doc-image-frame:hover .doc-image-handle,
	.doc-media.selected .doc-image-handle,
	.doc-image-handle:focus-visible {
		opacity: 0.6;
	}

	.doc-image-handle:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}

	/* On touch the handle takes the pointer only on a selected image: a thumb
	   that begins a scroll over it scrolls, and writes no width. Its hit
	   area is a thumb's 44pt, grown around the 8px bar. */
	@media (pointer: coarse) {
		.doc-image-handle {
			pointer-events: none;
			touch-action: auto;
		}

		.doc-media.selected .doc-image-handle {
			pointer-events: auto;
			touch-action: none;
		}

		.doc-image-handle::after {
			content: "";
			position: absolute;
			top: 50%;
			left: 50%;
			width: 44px;
			height: 48px;
			transform: translate(-50%, -50%);
		}
	}

	.doc-audio {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 12px;
		background: var(--color-surface);
	}

	.doc-audio-head {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--color-foreground-muted);
	}

	.doc-audio audio {
		width: 100%;
	}

	video {
		display: block;
		max-width: 100%;
		border-radius: 12px;
	}

	.doc-file-card {
		display: inline-flex;
		align-items: center;
		gap: 12px;
		max-width: 100%;
		padding: 8px 12px;
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 12px;
		background: var(--color-surface);
		color: var(--color-foreground);
		text-decoration: none;
	}

	.doc-file-card:hover {
		background: var(--hover-bg);
	}

	.doc-file-info {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.doc-file-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.875rem;
	}

	.doc-file-ext {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	.doc-print-card {
		display: none;
	}

	@media print {
		.doc-media audio,
		.doc-media video,
		.doc-audio,
		.doc-image-handle {
			display: none;
		}

		.doc-print-card {
			display: block;
		}
	}
</style>
