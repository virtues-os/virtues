<script lang="ts">
	import { onMount } from "svelte";
	import { animate } from "motion";
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
		typeof window !== "undefined" &&
		window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

	const EASE: [number, number, number, number] = [0.22, 1, 0.36, 1];

	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return { destroy: () => node.remove() };
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

			animate(backdropEl, { opacity: [0, 1] }, { duration: 0.22, ease: "linear" });
			if (reduceMotion) {
				animate(imgEl, { opacity: [0, 1] }, { duration: 0.18 });
				return;
			}
			animate(
				imgEl,
				{
					transform: [
						`translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`,
						"translate(0px, 0px) scale(1, 1)",
					],
				},
				{ duration: 0.34, ease: EASE },
			);
		};
		// Wait for the image to have layout (natural size known).
		if (imgEl.complete) run();
		else imgEl.addEventListener("load", run, { once: true });
	});

	async function close() {
		if (closing) return;
		closing = true;
		const fade = animate(backdropEl, { opacity: 0 }, { duration: 0.18, ease: "linear" });
		if (!reduceMotion) {
			const target = imgEl.getBoundingClientRect();
			const dx = originRect.left - target.left;
			const dy = originRect.top - target.top;
			const sx = originRect.width / target.width;
			const sy = originRect.height / target.height;
			await animate(
				imgEl,
				{ transform: `translate(${dx}px, ${dy}px) scale(${sx}, ${sy})`, opacity: 0.6 },
				{ duration: 0.26, ease: EASE },
			).finished;
		} else {
			await animate(imgEl, { opacity: 0 }, { duration: 0.14 }).finished;
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
		<Icon icon="ri:close-line" width="20" />
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
		border-radius: 0.75rem;
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
