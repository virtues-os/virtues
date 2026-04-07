<!--
	DayHeaderPolaroid.svelte

	Renders the "image of the day" as a small pinned polaroid to the right of
	the day title. MVP: pen & ink line art on white, composited with
	mix-blend-mode so it works in light AND dark modes without per-theme
	regeneration.

	Props let us audition different aspects and frame treatments before we
	commit to a single look.
-->

<script lang="ts">
	interface Props {
		/** Image URL or data URI. Falls back to inline placeholder SVG if absent. */
		imageUrl?: string;
		/** Aspect ratio of the image area (polaroid frame scales around it). */
		aspect?: "1:1" | "4:3" | "16:9";
		/** Frame treatment. */
		variant?: "polaroid" | "postcard" | "card" | "naked";
		/** Fixed pixel width of the card. Height is derived from aspect + frame. */
		width?: number;
		/** Rotation in degrees. Default randomized per day for physicality. */
		rotation?: number;
		/** Optional caption/subject line under the image (e.g. "kayak, dusk"). */
		caption?: string;
		/** Which placeholder sketch to use when no imageUrl. */
		placeholder?: "kayak" | "coffee" | "bungalow" | "bike";
	}

	let {
		imageUrl,
		aspect = "1:1",
		variant = "polaroid",
		width = 148,
		rotation = -2,
		caption,
		placeholder = "kayak",
	}: Props = $props();

	const aspectRatio = $derived(
		aspect === "16:9" ? 16 / 9 : aspect === "4:3" ? 4 / 3 : 1,
	);
	const imgW = $derived(width - (variant === "polaroid" ? 14 : variant === "postcard" ? 12 : variant === "card" ? 8 : 0));
	const imgH = $derived(Math.round(imgW / aspectRatio));

	// Placeholder pen-and-ink sketches (simple, iconic, testable)
	const placeholders: Record<string, string> = {
		kayak: `<svg viewBox="0 0 120 120" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
			<path d="M18 78 Q60 92 102 78 Q96 86 60 88 Q24 86 18 78 Z" />
			<path d="M26 78 L34 70 M86 70 L94 78" />
			<path d="M46 74 L50 66 M70 66 L74 74" stroke-width="1" opacity="0.7"/>
			<line x1="58" y1="72" x2="58" y2="82" />
			<path d="M14 62 L48 28 M72 28 L106 62" stroke-width="1"/>
			<path d="M14 62 L18 58 M106 62 L102 58" stroke-width="1"/>
			<path d="M10 98 Q30 96 60 98 Q90 100 110 98" stroke-width="0.8" opacity="0.4"/>
			<path d="M6 104 Q40 102 60 104 Q80 106 114 104" stroke-width="0.8" opacity="0.3"/>
		</svg>`,
		coffee: `<svg viewBox="0 0 120 120" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
			<path d="M32 42 L38 96 Q38 102 44 102 L70 102 Q76 102 76 96 L82 42 Z" />
			<ellipse cx="57" cy="42" rx="25" ry="5" />
			<path d="M82 52 Q96 54 96 68 Q96 82 82 84" />
			<path d="M46 28 Q44 22 48 18 M56 28 Q54 22 58 18 M66 28 Q64 22 68 18" stroke-width="1" opacity="0.6"/>
		</svg>`,
		bungalow: `<svg viewBox="0 0 120 120" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
			<path d="M12 68 L60 32 L108 68" />
			<path d="M22 64 L22 96 L98 96 L98 64" />
			<rect x="50" y="72" width="20" height="24" />
			<line x1="60" y1="72" x2="60" y2="96" stroke-width="0.8"/>
			<rect x="30" y="74" width="12" height="10" stroke-width="1"/>
			<rect x="78" y="74" width="12" height="10" stroke-width="1"/>
			<path d="M4 96 L116 96" stroke-width="0.8"/>
			<path d="M84 40 L84 26 L94 26 L94 48" stroke-width="1"/>
		</svg>`,
		bike: `<svg viewBox="0 0 120 120" xmlns="http://www.w3.org/2000/svg" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
			<circle cx="30" cy="80" r="18" />
			<circle cx="90" cy="80" r="18" />
			<path d="M30 80 L54 80 L68 52 L90 80" />
			<path d="M54 80 L68 52 L78 52" />
			<path d="M68 52 L72 42" />
			<line x1="30" y1="80" x2="30" y2="80" stroke-width="3"/>
			<line x1="90" y1="80" x2="90" y2="80" stroke-width="3"/>
		</svg>`,
	};

	const placeholderSvg = $derived(placeholders[placeholder] ?? placeholders.kayak);

	// Track image load failures → fall back to placeholder
	let imgFailed = $state(false);
	$effect(() => {
		// reset failure flag when url changes
		imgUrl;
		imgFailed = false;
	});
	const imgUrl = $derived(imageUrl);
	const showImage = $derived(!!imgUrl && !imgFailed);
</script>

<div
	class="polaroid-wrap"
	class:variant-polaroid={variant === "polaroid"}
	class:variant-postcard={variant === "postcard"}
	class:variant-card={variant === "card"}
	class:variant-naked={variant === "naked"}
	style="--rotation: {rotation}deg; --card-width: {width}px; --img-width: {imgW}px; --img-height: {imgH}px;"
>
	{#if variant === "polaroid"}
		<div class="pin" aria-hidden="true"></div>
	{/if}
	<div class="card">
		<div class="image-area">
			{#if showImage && variant === "naked"}
				<!-- Naked variant: use alpha channel as a mask, fill with theme color.
				     Works in any theme, no mode-specific CSS. -->
				<div class="mask-wrap" style:--mask-url="url({imgUrl})">
					<img src={imgUrl} alt={caption ?? "Day illustration"} onerror={() => (imgFailed = true)} aria-hidden="true" />
				</div>
			{:else if showImage}
				<img src={imgUrl} alt={caption ?? "Day illustration"} onerror={() => (imgFailed = true)} />
			{:else}
				<div class="placeholder">
					{@html placeholderSvg}
				</div>
			{/if}
		</div>
		{#if variant === "polaroid" && caption}
			<div class="polaroid-caption">{caption}</div>
		{/if}
	</div>
</div>

<style>
	.polaroid-wrap {
		position: relative;
		width: var(--card-width);
		flex-shrink: 0;
		transform: rotate(var(--rotation));
		transition: transform 0.25s ease, filter 0.25s ease;
		filter: drop-shadow(0 2px 6px rgba(20, 14, 6, 0.18))
			drop-shadow(0 10px 24px rgba(20, 14, 6, 0.14));
	}

	:global(.dark) .polaroid-wrap,
	:global([data-theme="dark"]) .polaroid-wrap {
		filter: drop-shadow(0 2px 8px rgba(0, 0, 0, 0.55))
			drop-shadow(0 14px 32px rgba(0, 0, 0, 0.45));
	}

.card {
		background:
			/* very faint paper grain */
			radial-gradient(circle at 20% 30%, rgba(120, 90, 40, 0.025), transparent 60%),
			radial-gradient(circle at 80% 70%, rgba(120, 90, 40, 0.03), transparent 55%),
			#fbf8ef; /* warm cream, does NOT invert in dark mode */
		position: relative;
	}

	.image-area {
		width: var(--img-width);
		height: var(--img-height);
		margin: 0 auto;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		color: #2a2a2a; /* ink color on the cream card */
	}

	/* Seamless ink-on-cream for framed variants:
	   grayscale first (kills any rogue color from the model),
	   then multiply so white bg drops out onto the card. */
	.variant-polaroid .image-area,
	.variant-card .image-area,
	.variant-postcard .image-area {
		mix-blend-mode: multiply;
		filter: grayscale(1) contrast(1.05);
	}

	.image-area img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.placeholder {
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.placeholder :global(svg) {
		width: 100%;
		height: 100%;
	}

	/* ─── Polaroid variant ─── */
	.variant-polaroid .card {
		/* Classic polaroid: even top/sides, extra bottom for "writing area" */
		padding: 7px 7px 22px 7px;
		border-radius: 1px;
	}
	.variant-polaroid .card:has(.polaroid-caption) {
		padding-bottom: 0;
	}

	.polaroid-caption {
		font-family: "Caveat", "Snell Roundhand", cursive;
		font-size: 14px;
		color: #3a3530;
		text-align: center;
		padding: 10px 4px 14px;
		letter-spacing: 0.02em;
	}

	.pin {
		position: absolute;
		top: -5px;
		left: 50%;
		transform: translateX(-50%);
		width: 10px;
		height: 10px;
		border-radius: 50%;
		/* Brass / antique pin — warm, journal-ish, works on any bg */
		background: radial-gradient(
			circle at 35% 30%,
			#e8cc8c 0%,
			#b89968 45%,
			#7a5e2e 85%,
			#4a3818 100%
		);
		box-shadow:
			0 1px 2px rgba(0, 0, 0, 0.5),
			inset -1px -1px 2px rgba(0, 0, 0, 0.3),
			inset 1px 1px 1px rgba(255, 230, 180, 0.4);
		z-index: 2;
	}

	/* ─── Postcard variant (horizontal, no pin) ─── */
	.variant-postcard .card {
		padding: 6px;
		border-radius: 2px;
		border: 1px solid rgba(0, 0, 0, 0.08);
	}

	/* ─── Card variant (minimal cream card, small padding) ─── */
	.variant-card .card {
		padding: 4px;
		border-radius: 3px;
		border: 1px solid rgba(0, 0, 0, 0.06);
	}

	/* ─── Naked variant (no frame, blend into page) ─── */
	.variant-naked {
		filter: none;
	}
	.variant-naked .card {
		background: transparent;
		padding: 0;
	}
	.variant-naked .image-area {
		/* Image is alpha-transparent PNG (white pre-keyed to alpha, auto-cropped
		   to content bbox). Let natural aspect dictate height. */
		width: var(--card-width);
		height: auto;
	}
	/* Mask-image technique: alpha channel becomes a stencil, fill with theme color.
	   Perfectly adapts to any theme (light/dark/custom). */
	.mask-wrap {
		position: relative;
		width: 100%;
	}
	.mask-wrap img {
		display: block;
		width: 100%;
		height: auto;
		opacity: 0; /* invisible — only there to set the intrinsic aspect ratio */
	}
	.mask-wrap::after {
		content: "";
		position: absolute;
		inset: 0;
		background-color: var(--color-foreground);
		-webkit-mask-image: var(--mask-url);
		mask-image: var(--mask-url);
		-webkit-mask-size: 100% 100%;
		mask-size: 100% 100%;
		-webkit-mask-repeat: no-repeat;
		mask-repeat: no-repeat;
		/* Slight transparency makes it feel like ink on paper, not solid fill */
		opacity: 0.85;
	}
</style>
