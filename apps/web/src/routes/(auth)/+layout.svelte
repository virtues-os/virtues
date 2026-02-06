<script lang="ts">
	import "../../app.css";
	import { onMount } from "svelte";
	import { Toaster } from "svelte-sonner";

	import MatrixOverlay from "$lib/components/MatrixOverlay.svelte";
	import { authMatrixMessage } from "$lib/stores/authMatrix.svelte";
	import { initTheme } from "$lib/utils/theme";

	let { children } = $props();

	let hostname = $state("");

	onMount(() => {
		initTheme();
		hostname = window.location.hostname;
	});

	const displayHost = $derived(
		hostname === "localhost" || hostname === "127.0.0.1"
			? "virtues.local"
			: hostname
	);

	const isSent = $derived($authMatrixMessage === "SENT");
</script>

<Toaster
	position="top-center"
	toastOptions={{
		style: `
			background: var(--surface);
			color: var(--foreground);
			border: 1px solid var(--border);
			font-family: var(--font-sans);
		`,
	}}
/>

<div class="min-h-screen overscroll-none bg-surface flex items-center justify-center px-6">
	<div class="w-full max-w-sm">
		<!-- Header with hostname, form label, and matrix -->
		<div class="flex items-end justify-between gap-6 mb-6">
			<div class="flex flex-col gap-2">
				<p class="font-serif text-lg text-foreground-muted">
					{displayHost}
				</p>
				<div class="flip-container">
					<div class="flip-card" class:flipped={isSent}>
						<p class="flip-front text-foreground-muted text-sm whitespace-nowrap">
							Sign in with your email
						</p>
						<p class="flip-back text-primary text-sm whitespace-nowrap">
							Check your email
						</p>
					</div>
				</div>
			</div>
			<div class="mb-1.5">
				<MatrixOverlay
					layout="inline"
					cols={16}
					rows={5}
					cellSize={3}
					cellColor="var(--foreground-muted)"
					fillColor="var(--primary)"
					padding={0}
				/>
			</div>
		</div>

		<!-- Login form -->
		<main>
			{@render children()}
		</main>
	</div>
</div>

<style>
	.flip-container {
		perspective: 200px;
		height: 1.25rem; /* text-sm line height */
	}

	.flip-card {
		position: relative;
		width: 100%;
		height: 100%;
		transform-style: preserve-3d;
		transition: transform 0.4s ease-out;
	}

	.flip-card.flipped {
		transform: rotateX(-180deg);
	}

	.flip-front,
	.flip-back {
		position: absolute;
		width: 100%;
		backface-visibility: hidden;
		margin: 0;
	}

	.flip-back {
		transform: rotateX(180deg);
	}
</style>
