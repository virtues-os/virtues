<!--
	Light or dark, as one small icon in Welcome's top corner beside the
	sound: a moon on a light page, a sun on a dark one. Most people never
	touch it, so it stays out of the lockup's way; anyone who wants the
	other look finds it where apps keep it.

	Pressing it turns the theme through the ∴ (themeReveal.ts): the mark's
	three dots breathe in and swell until they cover the window in the new
	colors. It changes only the look.

	WAS a Light / Dark segmented control under Begin (2026-09-25, same day),
	which put a preference in the middle of the cold open; before that,
	"Continue in light / Continue in dark", which bundled the preference into
	the way forward and read as fine print.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { DEFAULT_THEMES, getTheme, isThemeDark } from "$lib/utils/theme";
	import { revealTheme } from "./themeReveal";

	/** The ∴ the new theme is pushed out of, and the box it is drawn on. */
	let { mark }: { mark: () => { el: Element; origin: number; span: number } | null } = $props();

	let dark = $state(false);
	let busy = $state(false);

	onMount(() => {
		dark = isThemeDark(getTheme());
		const on = () => (dark = isThemeDark(getTheme()));
		window.addEventListener("themechange", on);
		return () => window.removeEventListener("themechange", on);
	});

	async function flip() {
		if (busy) return;
		busy = true;
		const toDark = !dark;
		dark = toDark;
		try {
			await revealTheme(DEFAULT_THEMES[toDark ? "dark" : "light"], mark());
		} finally {
			busy = false;
		}
	}
</script>

<button
	type="button"
	class="flip"
	class:dark
	onclick={flip}
	aria-label={dark ? "Switch to light" : "Switch to dark"}
	title={dark ? "Light" : "Dark"}
>
	<svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
		<!-- The moon, shown on a light page. -->
		<g class="moon"><path d="M19.5 14.2A7.8 7.8 0 0 1 9.8 4.5a7.8 7.8 0 1 0 9.7 9.7z" /></g>
		<!-- The sun, shown on a dark one. -->
		<g class="sun">
			<circle cx="12" cy="12" r="3.8" />
			<path d="M12 2.8v2M12 19.2v2M2.8 12h2M19.2 12h2M5.5 5.5l1.4 1.4M17.1 17.1l1.4 1.4M5.5 18.5l1.4-1.4M17.1 6.9l1.4-1.4" />
		</g>
	</svg>
</button>

<style>
	.flip {
		display: grid;
		place-content: center;
		width: 34px;
		height: 34px;
		border: none;
		border-radius: 50%;
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition: color 0.15s ease;
	}
	.flip:hover {
		color: var(--color-foreground);
	}
	.flip:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
	svg {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.6;
		stroke-linecap: round;
		stroke-linejoin: round;
		overflow: visible;
	}
	/* The two glyphs trade places with a quarter turn. */
	.moon,
	.sun {
		transform-origin: 12px 12px;
		transition:
			opacity 300ms ease,
			transform 500ms cubic-bezier(0.3, 1.3, 0.5, 1);
	}
	.sun {
		opacity: 0;
		transform: rotate(-90deg) scale(0.6);
	}
	.dark .moon {
		opacity: 0;
		transform: rotate(90deg) scale(0.6);
	}
	.dark .sun {
		opacity: 1;
		transform: none;
	}
	@media (prefers-reduced-motion: reduce) {
		.moon,
		.sun {
			transition: none;
		}
	}
</style>
