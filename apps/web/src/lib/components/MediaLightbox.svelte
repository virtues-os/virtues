<script lang="ts">
	import { portal } from "$lib/actions/portal";
	import { reducedMotion } from "$lib/stores/motion.svelte";
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";

	interface Props {
		src: string;
		alt?: string;
		/** Bounding rect of the thumbnail that was clicked (the shared-element origin). */
		originRect: DOMRect;
		onClose: () => void;
	}

	let { src, alt = "image", originRect, onClose }: Props = $props();

	let backdropEl: HTMLDivElement;
	let imgEl: HTMLImageElement;
	let closing = false;

	const reduceMotion =
		reducedMotion();

	const EASE = "cubic-bezier(0.22, 1, 0.36, 1)";

	// The browser's own animations: the lightbox opens from the chat, and the
	// motion library cost every chat home ~50 KB to animate four properties.
	// `fill: "forwards"` holds the last frame, so a closing image stays where
	// it ended until onClose removes it.
	function animate(el: Element, keyframes: Keyframe[], ms: number, easing = "ease") {
		return el.animate(keyframes, { duration: ms, easing, fill: "forwards" });
	}

	// FLIP open: paint the full image at its final centered size, then start it
	// transformed back onto the thumbnail's footprint and release it.
	onMount(() => {
		const run = () => {
			const target = imgEl.getBoundingClientRect();
			if (!target.width || !target.height) return;
			const dx = originRect.left - target.left;
			const dy = originRect.top - target.top;
			const sx = originRect.width / target.width;
			const sy = originRect.height / target.height;

			animate(backdropEl, [{ opacity: 0 }, { opacity: 1 }], 220, "linear");
			if (reduceMotion) {
				animate(imgEl, [{ opacity: 0 }, { opacity: 1 }], 180);
				return;
			}
			animate(
				imgEl,
				[
					{ transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})` },
					{ transform: "translate(0px, 0px) scale(1, 1)" },
				],
				340,
				EASE,
			);
		};
		// Wait for the image to have layout (natural size known).
		if (imgEl.complete) run();
		else imgEl.addEventListener("load", run, { once: true });
	});

	async function close() {
		if (closing) return;
		closing = true;
		const fade = animate(backdropEl, [{ opacity: 0 }], 180, "linear");
		if (!reduceMotion) {
			const target = imgEl.getBoundingClientRect();
			const dx = originRect.left - target.left;
			const dy = originRect.top - target.top;
			const sx = originRect.width / target.width;
			const sy = originRect.height / target.height;
			await animate(
				imgEl,
				[{ transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`, opacity: 0.6 }],
				260,
				EASE,
			).finished;
		} else {
			await animate(imgEl, [{ opacity: 0 }], 140).finished;
		}
		await fade.finished;
		onClose();
	}

	$effect(() => {
		const handler = (e: KeyboardEvent) => {
			if (e.key === "Escape") {
				e.preventDefault();
				close();
			}
		};
		window.addEventListener("keydown", handler);
		return () => window.removeEventListener("keydown", handler);
	});
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div
	class="lightbox"
	bind:this={backdropEl}
	role="presentation"
	onclick={(e) => {
		if (e.target === e.currentTarget) close();
	}}
	use:portal
>
	<button class="lightbox-close" onclick={close} aria-label="Close">
		<Icon icon="mynaui:x" width="20" />
	</button>
	<img
		bind:this={imgEl}
		{src}
		{alt}
		class="lightbox-img"
		style="transform-origin: top left;"
		onclick={close}
		draggable="false"
	/>
</div>

<style>
	.lightbox {
		position: fixed;
		inset: 0;
		z-index: var(--z-lightbox);
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 4vmin;
		background: color-mix(in srgb, var(--color-background) 78%, #000 22%);
		backdrop-filter: blur(14px) saturate(115%);
		-webkit-backdrop-filter: blur(14px) saturate(115%);
		opacity: 0;
	}

	.lightbox-img {
		max-width: 92vw;
		max-height: 92vh;
		object-fit: contain;
		border-radius: var(--radius-xl);
		box-shadow:
			0 24px 60px -12px rgba(0, 0, 0, 0.45),
			0 0 0 1px color-mix(in srgb, var(--color-foreground) 8%, transparent);
		cursor: zoom-out;
		user-select: none;
	}

	.lightbox-close {
		position: fixed;
		top: max(1rem, env(safe-area-inset-top));
		right: max(1rem, env(safe-area-inset-right));
		display: flex;
		align-items: center;
		justify-content: center;
		width: 2.25rem;
		height: 2.25rem;
		border-radius: var(--radius-full);
		border: none;
		background: color-mix(in srgb, var(--color-surface) 70%, transparent);
		color: var(--color-foreground);
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.lightbox-close:hover {
		background: var(--color-surface);
	}
</style>
