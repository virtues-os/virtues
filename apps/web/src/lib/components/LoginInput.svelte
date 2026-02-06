<script lang="ts">
	import { tick, onMount } from "svelte";
	import Icon from "./Icon.svelte";

	interface Props {
		value?: string;
		placeholder?: string;
		type?: "text" | "email";
		disabled?: boolean;
		onsubmit?: () => void;
	}

	let {
		value = $bindable(""),
		placeholder = "",
		type = "email",
		disabled = false,
		onsubmit,
	}: Props = $props();

	// Internal state for input - like the original
	let inputValue = $state("");
	let inputVisible = $state(true);
	let firstKeystroke = $state(true);
	let inputElement: HTMLInputElement | null = $state(null);
	let isAnimatingCursor = $state(false);

	let caretX = $state(47);
	let offset = $state(39);
	let isFocused = $state(false);

	let canvasRef: HTMLCanvasElement | null = $state(null);
	let particles: any[] = $state([]);
	let sweepX = $state(0);
	let isSweeping = $state(false);

	// Sync internal state to parent prop
	$effect(() => {
		value = inputValue;
	});

	// Sync parent prop to internal state (for initial value)
	onMount(() => {
		inputValue = value;
		updateCaretPosition();
	});

	async function handleKeyPress(event: KeyboardEvent) {
		if (event.key === "Enter") {
			if (inputElement && inputValue.trim() && !isSweeping) {
				event.preventDefault();

				isAnimatingCursor = true;

				// Start the sweep animation
				generateParticles();

				// Call onsubmit immediately so SENT appears with the vanish
				onsubmit?.();
			}
		} else if (event.key === "Backspace" && inputValue === "") {
			firstKeystroke = true;
		} else if (firstKeystroke && event.key !== "Backspace") {
			firstKeystroke = false;
		}
	}

	function handleInput(event: Event) {
		const input = event.target as HTMLInputElement;
		if (input.value === "") {
			firstKeystroke = true;
		}
		// Read directly from event target
		setTimeout(() => {
			const caretPos = input.selectionStart ?? input.value.length;
			const { left } = getCaretCoordinates(input, caretPos);
			caretX = left + offset;
		}, 0);
	}

	function updateCaretPosition() {
		if (inputElement) {
			const caretPos = inputElement.selectionStart ?? inputElement.value.length;
			const { left } = getCaretCoordinates(inputElement, caretPos);
			caretX = left + offset;
		}
	}

	const SCALE = 2;

	function generateParticles() {
		const canvas = canvasRef;
		const ctx = canvas?.getContext("2d");
		if (!canvas || !ctx) return;

		// Render at 2x for denser particles, coordinates scaled back to 1x for display
		canvas.width = 300 * SCALE;
		canvas.height = 48 * SCALE;

		ctx.clearRect(0, 0, canvas.width, canvas.height);

		const style = inputElement ? window.getComputedStyle(inputElement) : null;
		const fontSize = style ? parseFloat(style.fontSize) : 16;
		ctx.font = `${fontSize * SCALE}px ${style?.fontFamily ?? "sans-serif"}`;

		const computedStyle = getComputedStyle(document.documentElement);
		const textColor = computedStyle.getPropertyValue("--foreground").trim() || "#333";
		ctx.fillStyle = textColor;
		// Position text to match input: 12px padding + 16px icon + 10px margin = 38px
		// Baseline Y adjusted to vertically center in 48px container
		ctx.fillText(inputValue, 38 * SCALE, 28 * SCALE);

		const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
		particles = [];
		let maxX = 0;

		for (let y = 0; y < canvas.height; y += SCALE) {
			for (let x = 0; x < canvas.width; x += SCALE) {
				const index = (y * canvas.width + x) * 4;
				const r = imageData.data[index];
				const g = imageData.data[index + 1];
				const b = imageData.data[index + 2];
				const a = imageData.data[index + 3];

				if (a > 0) {
					// Scale coordinates back to 1x display space
					const dx = x / SCALE;
					const dy = y / SCALE;
					if (dx > maxX) maxX = dx;
					particles.push({
						x: dx,
						y: dy,
						originX: dx,
						originY: dy,
						r: 1,
						color: `rgba(${r}, ${g}, ${b}, ${a / 255})`,
						velocityX: Math.random() * -0.4 - 0.1,
						velocityY: Math.random() * 0.6 - 0.3,
						active: false,
					});
				}
			}
		}

		// Start sweep from rightmost particle
		sweepX = maxX + 5;
		isSweeping = true;
		animateSweep();
	}

	function animateSweep() {
		const canvas = canvasRef;
		const ctx = canvas?.getContext("2d");
		if (!canvas || !ctx) return;

		// Move sweep cursor left
		sweepX -= 3;

		// Update caret position to follow sweep, but stop at home position
		// During sweep, caret should be at sweepX (canvas coordinates match container)
		const homeX = 47;
		caretX = Math.max(homeX, sweepX);

		// Activate particles the cursor has passed
		particles.forEach((p: any) => {
			if (!p.active && p.originX >= sweepX) {
				p.active = true;
			}
		});

		// Update active particles (move + fade)
		particles = particles
			.map((p: any) => {
				if (p.active) {
					p.x += p.velocityX * 0.5;
					p.y += p.velocityY * 0.5;
					p.r -= 0.008;
				}
				return p.r > 0 ? p : null;
			})
			.filter(Boolean);

		// Render
		ctx.clearRect(0, 0, canvas.width, canvas.height);

		particles.forEach((p: any) => {
			if (p.active) {
				// Draw as dissolving particle
				ctx.beginPath();
				ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
				ctx.fillStyle = p.color;
				ctx.fill();
			} else {
				// Draw as solid pixel (static text)
				ctx.fillStyle = p.color;
				ctx.fillRect(p.originX, p.originY, 1, 1);
			}
		});

		// Continue until all particles gone
		if (particles.length > 0) {
			requestAnimationFrame(animateSweep);
		} else {
			isSweeping = false;
			// Reset input after animation completes
			inputVisible = false;
			isAnimatingCursor = false;
			inputValue = "";
			setTimeout(() => {
				inputVisible = true;
				firstKeystroke = true;
				caretX = 47;
			}, 100);
		}
	}

	function getCaretCoordinates(input: HTMLInputElement, position: number) {
		const div = document.createElement("div");
		const style = window.getComputedStyle(input);
		for (const prop of style) {
			div.style.setProperty(prop, style.getPropertyValue(prop));
		}
		div.style.position = "absolute";
		div.style.visibility = "hidden";
		div.style.whiteSpace = "pre";
		div.style.width = "auto";

		document.body.appendChild(div);

		const val = input.value;
		// Use innerHTML with &nbsp; to preserve trailing spaces
		const before = val.substring(0, position).replace(/ /g, "&nbsp;");
		const after = val.substring(position) || ".";
		div.innerHTML = `${before}<span id="caret-marker">${after.replace(/ /g, "&nbsp;")}</span>`;
		const span = div.querySelector("#caret-marker") as HTMLSpanElement;
		div.appendChild(span);

		const { offsetLeft: left } = span;
		document.body.removeChild(div);

		return { left };
	}
</script>

<div class="login-input-container group relative" class:disabled>
	<Icon
		icon="ri:at-line"
		class="input-icon text-foreground-muted transition-all duration-300 group-focus-within:text-primary"
	/>

	<canvas bind:this={canvasRef} class="absolute left-0 top-0 pointer-events-none"></canvas>

	{#if inputVisible}
		<input
			onfocus={() => {
				isFocused = true;
				updateCaretPosition();
			}}
			autocomplete="off"
			autocorrect="off"
			autocapitalize="off"
			spellcheck="false"
			data-1p-ignore
			data-lpignore="true"
			data-form-type="other"
			name="notapassword"
			onblur={() => (isFocused = false)}
			bind:this={inputElement}
			class="login-input peer outline-none"
			type="text"
			inputmode="email"
			bind:value={inputValue}
			placeholder={firstKeystroke ? placeholder : ""}
			onkeydown={handleKeyPress}
			oninput={(e) => handleInput(e)}
			{disabled}
			style="opacity: {isAnimatingCursor ? 0 : 1};"
		/>
	{/if}

	{#if (isFocused || isSweeping) && !disabled}
		<div
			class="absolute top-1/2 z-30 h-5 w-[2px] -translate-y-1/2 rounded-full bg-primary {isSweeping ? '' : 'animate-pulse'}"
			style="left: {caretX}px; transition: left {isSweeping ? '0ms' : '115ms'};"
		></div>
	{/if}
</div>

<style>
	.login-input-container {
		position: relative;
		display: flex;
		align-items: center;
		width: 100%;
		height: 48px;
		border: 1px solid color-mix(in srgb, var(--foreground) 20%, var(--border));
		background: var(--surface-alt);
		padding: 0 12px;
		border-radius: 8px;
		transition: all 0.15s ease;
		overflow: hidden;
		cursor: text;
	}

	.login-input-container:hover {
		border-color: var(--primary);
	}

	.login-input-container:focus-within {
		border-color: var(--primary);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--primary) 15%, transparent);
	}

	.login-input-container.disabled {
		opacity: 0.5;
		pointer-events: none;
	}

	:global(.input-icon) {
		flex-shrink: 0;
		margin-right: 10px;
		z-index: 2;
		font-size: 16px;
	}

	.login-input {
		flex: 1;
		border: none;
		outline: none;
		font-size: 1rem;
		font-family: var(--font-sans);
		color: var(--foreground);
		background: transparent;
		caret-color: transparent;
		position: relative;
		z-index: 2;
	}

	.login-input::placeholder {
		color: var(--foreground-muted);
		opacity: 0.6;
	}

	@keyframes pulse {
		50% {
			opacity: 0.5;
		}
	}

	.animate-pulse {
		animation: pulse 1s cubic-bezier(0.4, 0, 0.6, 1) infinite;
	}
</style>
