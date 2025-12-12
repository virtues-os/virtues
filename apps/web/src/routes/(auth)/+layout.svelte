<script lang="ts">
	import "../../app.css";
	import { Toaster } from "svelte-sonner";
	import { onMount } from "svelte";
	import { initTheme } from "$lib/utils/theme";

	import libraryImg from "$lib/assets/library.png";
	import MatrixOverlay from "$lib/components/MatrixOverlay.svelte";

	let { children } = $props();

	onMount(() => {
		initTheme();
	});
</script>

<Toaster position="top-center" />

<div class="flex min-h-screen overscroll-none">
	<!-- Left: Visual Panel (33%) - Fixed -->
	<aside
		class="w-1/3 fixed top-0 left-0 h-screen overflow-hidden max-md:relative max-md:w-full max-md:h-auto max-md:min-h-[200px]"
		style="background-color: var(--color-background-inverse)"
	>
		<img
			src={libraryImg}
			alt=""
			class="absolute inset-0 w-full h-full object-cover object-center z-0"
		/>
		<div
			class="absolute inset-0 bg-linear-to-b from-black/30 to-black/60 z-1"
		></div>
		<MatrixOverlay
			layout="bottom"
			cols={24}
			rows={4}
			cellSize={6}
			cellColor="rgba(255, 255, 255, 0.5)"
		/>
		<div
			class="relative z-2 h-full flex flex-col justify-between p-12 text-white max-md:p-8"
		>
			<span
				class="font-serif text-xl font-normal tracking-wide opacity-90"
				>Virtues</span
			>

			<blockquote class="max-w-xs">
				<p
					class="font-serif text-[22px] font-normal leading-snug tracking-tight mb-4 opacity-95 max-md:text-lg"
				>
					"Know thyself."
				</p>
				<footer class="flex flex-col gap-0.5">
					<cite
						class="font-sans text-[13px] not-italic font-medium tracking-wide opacity-80"
					>
						Delphic maxim
					</cite>
					<span class="font-sans text-xs opacity-50"
						>Temple of Apollo at Delphi</span
					>
				</footer>
			</blockquote>

			<div class="h-5"></div>
		</div>
	</aside>

	<!-- Right: Action Panel (67%) -->
	<main
		class="w-2/3 ml-[33.333%] h-screen overflow-y-auto overscroll-none bg-surface flex flex-col max-md:w-full max-md:ml-0 max-md:h-auto max-md:min-h-0"
	>
		<div
			class="flex-1 flex items-center justify-center w-full px-12 pb-16 max-md:px-6 max-md:pb-12"
		>
			{@render children()}
		</div>
	</main>
</div>
