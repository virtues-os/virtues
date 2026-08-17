<script lang="ts">
	// Root layout - minimal, delegates to route group layouts
	// This is kept minimal as (onboarding) and (app) groups have their own layouts
	import { onMount } from "svelte";

	let { children } = $props();

	// The one thing that cannot live in a route group's layout: swallowing a
	// file dropped somewhere nothing claims.
	//
	// `(app)/+layout.svelte` calls preventDefault on every `dragover`, which
	// makes the WHOLE document a drop target — necessary, since a drop is only
	// delivered to a canceled dragover. But the matching `drop` is only handled
	// by the components that want files (chat composer, Drive, notebooks). Drop
	// a PDF anywhere else and it reaches the browser's default action, which is
	// to NAVIGATE to the file. In a tab that merely opens the PDF. In the
	// desktop shell — one window, no back button, no address bar — it replaces
	// the app with WebKit's PDF viewer, and there is no way back short of
	// relaunching (2026-08-17).
	//
	// Bubble phase, so component handlers have already seen the event and read
	// its files; by the time this runs, preventDefault only cancels the
	// navigation. So an unclaimed drop lands nowhere instead of taking the app
	// with it — which is what dropping a file on the sidebar should do.
	//
	// It is HERE rather than in `(app)` because the hazard is not app-specific:
	// onboarding and the auth pages are equally one-way, and they are exactly
	// where a new user is most likely to be dragging a file hopefully at the
	// screen. src-tauri/src/main.rs blocks `file:` navigation too, for the
	// pages the SPA never boots at all (connect.html, and the window before
	// mount).
	onMount(() => {
		const swallowStrayDrop = (e: DragEvent) => e.preventDefault();
		document.addEventListener("drop", swallowStrayDrop);
		return () => document.removeEventListener("drop", swallowStrayDrop);
	});
</script>

{@render children()}
