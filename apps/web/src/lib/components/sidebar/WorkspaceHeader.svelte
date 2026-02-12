<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import {
		DEFAULT_FOLDER_PATH,
		VIRTUES_LOGO_PATH,
		getIconPath,
	} from "$lib/utils/svgPaths";
	import { onMount, untrack } from "svelte";

	interface Props {
		collapsed?: boolean;
		onTitleAction?: (e: MouseEvent) => void;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		onTitleAction,
		animationDelay = 0,
	}: Props = $props();

	// GSAP (dynamically loaded)
	let gsapLib: typeof import("gsap").default | null = $state(null);
	let morphReady = $state(false);

	// Morph icon ref — GSAP owns the `d` attribute after first render
	let morphPathEl: SVGPathElement | null = $state(null);
	let prevSpaceId = $state("");
	let prevTargetPath = $state<string | null>(null);
	let tweenRef: { kill: () => void } | null = null;

	// Track emoji state for crossfade
	let isEmojiActive = $state(false);
	let emojiValue = $state("");

	onMount(async () => {
		try {
			const gsapModule = await import("gsap");
			const { MorphSVGPlugin } = await import("gsap/MorphSVGPlugin");
			gsapLib = gsapModule.default;
			if (gsapLib && MorphSVGPlugin) {
				gsapLib.registerPlugin(MorphSVGPlugin);
				morphReady = true;
			}
		} catch (e) {
			console.warn("[WorkspaceHeader] GSAP/MorphSVG load failed:", e);
		}
	});

	function isEmoji(val: string | null): boolean {
		if (!val) return false;
		return !val.includes(":");
	}

	function getTargetPath(
		space: (typeof spaceStore.spaces)[0] | undefined | null,
	): string | null {
		if (!space) return DEFAULT_FOLDER_PATH;
		if (space.is_system) return VIRTUES_LOGO_PATH;
		if (space.icon && !isEmoji(space.icon)) {
			return getIconPath(space.icon) || DEFAULT_FOLDER_PATH;
		}
		if (!space.icon) return DEFAULT_FOLDER_PATH;
		return null; // Emoji — no SVG path
	}

	// Active space label
	const activeLabel = $derived.by(() => {
		const space = spaceStore.activeSpace;
		if (!space) return "Workspace";
		return space.is_system ? "Virtues" : space.name;
	});

	// Accent color from active space
	const accentColor = $derived(spaceStore.activeSpace?.accent_color ?? null);

	// Derive target path reactively (triggers the $effect below)
	const targetPath = $derived.by(() => {
		const space = spaceStore.activeSpace;
		return getTargetPath(space);
	});

	// Morph the title icon when active space changes.
	// CRITICAL: GSAP owns the <path d="..."> attribute — we never bind it reactively.
	// NOTE: morphPathEl is read OUTSIDE untrack() so bind:this triggers a re-run.
	$effect(() => {
		const newSpaceId = spaceStore.activeSpaceId;
		const newTargetPath = targetPath;
		const el = morphPathEl; // Subscribe to element binding

		untrack(() => {
			if (!newSpaceId || !el) return;

			const spaceChanged = newSpaceId !== prevSpaceId;
			const pathChanged = newTargetPath !== prevTargetPath;

			if (!spaceChanged && !pathChanged) return;

			const space = spaceStore.activeSpace;
			const isFirstRender = !prevSpaceId;

			if (tweenRef) {
				tweenRef.kill();
				tweenRef = null;
			}

			if (newTargetPath === null) {
				// Emoji — crossfade
				isEmojiActive = true;
				emojiValue = space?.icon || "";
				if (morphReady && gsapLib) {
					tweenRef = gsapLib.to(el, {
						opacity: 0,
						duration: 0.15,
						ease: "power2.out",
					});
				}
			} else {
				const wasEmoji = isEmojiActive;
				isEmojiActive = false;
				emojiValue = "";

				if (isFirstRender || !spaceChanged) {
					// First render or late-arriving space data — set immediately
					el.setAttribute("d", newTargetPath);
				} else if (morphReady && gsapLib) {
					if (wasEmoji) {
						el.setAttribute("d", newTargetPath);
						tweenRef = gsapLib.fromTo(
							el,
							{ opacity: 0 },
							{ opacity: 1, duration: 0.15 },
						);
					} else {
						tweenRef = gsapLib.to(el, {
							morphSVG: newTargetPath,
							duration: 0.4,
							ease: "power2.inOut",
						});
					}
				} else {
					el.setAttribute("d", newTargetPath);
				}
			}

			prevSpaceId = newSpaceId;
			prevTargetPath = newTargetPath;
		});

		return () => {
			if (tweenRef) {
				tweenRef.kill();
				tweenRef = null;
			}
		};
	});
</script>

<div class="header-container" class:collapsed>
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="title-row animate-row"
		style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
		onclick={(e) => onTitleAction?.(e)}
		oncontextmenu={(e) => {
			e.preventDefault();
			onTitleAction?.(e);
		}}
	>
		<!-- Morphing icon -->
		<div class="title-icon" style:color={accentColor}>
			{#if isEmojiActive}
				<span class="title-emoji">{emojiValue}</span>
			{/if}
			<svg
				class="title-svg"
				class:hidden={isEmojiActive}
				width="16"
				height="16"
				viewBox="0 0 24 24"
			>
				<!-- d is set imperatively by GSAP — NOT a reactive binding -->
				<path bind:this={morphPathEl} fill="currentColor" />
			</svg>
		</div>

		<span class="title-label" style:color={accentColor}>{activeLabel}</span>

		<!-- Hover action: ... opens space context menu -->
		<div class="title-actions">
			<button
				class="title-action"
				onclick={(e) => {
					e.stopPropagation();
					onTitleAction?.(e);
				}}
				title="Space options"
			>
				<Icon icon="ri:more-2-fill" width="14" />
			</button>
		</div>
	</div>
</div>

<style>
	@reference "../../../app.css";

	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	@keyframes fadeSlideIn {
		from {
			opacity: 0;
			transform: translateX(-8px);
		}
		to {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.header-container {
		display: flex;
		flex-direction: column;
		padding: 16px 0 10px 8px;
	}

	.header-container.collapsed {
		opacity: 0;
		transform: translateX(-8px);
		transition:
			opacity 150ms var(--ease-premium),
			transform 150ms var(--ease-premium);
	}

	.animate-row {
		animation: fadeSlideIn 200ms var(--ease-premium) backwards;
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--ease-premium) var(--stagger-delay, 0ms),
			transform 200ms var(--ease-premium) var(--stagger-delay, 0ms);
	}

	.title-row {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px var(--sidebar-padding-left-base, 10px);
		height: 32px;
		box-sizing: border-box;
		cursor: pointer;
		border-radius: 6px;
	}

	.title-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
		color: var(--color-foreground);
		position: relative;
	}

	.title-svg {
		display: block;
		transition: opacity 0.15s ease;
	}

	.title-svg.hidden {
		opacity: 0;
		position: absolute;
	}

	.title-emoji {
		font-size: 14px;
		line-height: 1;
	}

	.title-label {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 17px;
		font-weight: 400;
		font-family: var(--font-serif, serif);
		color: var(--color-foreground);
		line-height: 1.4;
	}

	/* Hidden by default, shown on title row hover */
	.title-actions {
		display: flex;
		align-items: center;
		gap: 2px;
		opacity: 0;
		transition: opacity 150ms ease;
	}

	.title-row:hover .title-actions {
		opacity: 1;
	}

	.title-action {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		border-radius: 4px;
		border: none;
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		padding: 0;
		transition:
			background-color 150ms ease,
			color 150ms ease;
	}

	.title-action:hover {
		background: color-mix(
			in srgb,
			var(--color-foreground) 7%,
			transparent
		);
		color: var(--color-foreground);
	}

	.title-action:active {
		transform: scale(0.95);
	}
</style>
