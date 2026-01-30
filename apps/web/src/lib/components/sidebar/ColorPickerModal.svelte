<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import Modal from "$lib/components/Modal.svelte";

	interface Props {
		open: boolean;
		value: string | null;
		onSelect: (color: string | null) => void;
		onClose: () => void;
	}

	let { open, value, onSelect, onClose }: Props = $props();

	// Theme-derived colors (rendered with CSS vars, resolved to hex on select)
	const themeColors: { name: string; cssVar?: string; hex?: string }[] = [
		{ name: "White", hex: "#ffffff" },
		{ name: "Black", hex: "#000000" },
		{ name: "Foreground", cssVar: "--color-foreground-muted" },
		{ name: "Primary", cssVar: "--color-primary" },
		{ name: "Secondary", cssVar: "--color-secondary" },
		{ name: "Success", cssVar: "--color-success" },
		{ name: "Warning", cssVar: "--color-warning" },
		{ name: "Error", cssVar: "--color-error" },
		{ name: "Info", cssVar: "--color-info" },
	];

	// Fixed spectrum colors
	const spectrumColors = [
		{ name: "Ruby", hex: "#dc2626" },
		{ name: "Coral", hex: "#f97316" },
		{ name: "Amber", hex: "#f59e0b" },
		{ name: "Emerald", hex: "#10b981" },
		{ name: "Teal", hex: "#14b8a6" },
		{ name: "Sky", hex: "#0ea5e9" },
		{ name: "Indigo", hex: "#6366f1" },
		{ name: "Violet", hex: "#8b5cf6" },
		{ name: "Fuchsia", hex: "#d946ef" },
		{ name: "Rose", hex: "#f43f5e" },
		{ name: "Slate", hex: "#64748b" },
		{ name: "Stone", hex: "#78716c" },
	];

	// Custom color state
	let customHex = $state(value || "#6366f1");

	// Sync custom hex when modal opens
	$effect(() => {
		if (open && value) {
			customHex = value;
		}
	});

	// Resolve a CSS variable to a hex string
	function resolveVar(cssVar: string): string {
		const raw = getComputedStyle(document.documentElement).getPropertyValue(cssVar).trim();
		if (raw.startsWith("#")) return raw;
		const m = raw.match(/rgba?\(\s*(\d+),\s*(\d+),\s*(\d+)/);
		if (m) {
			return `#${[m[1], m[2], m[3]].map(v => parseInt(v).toString(16).padStart(2, "0")).join("")}`;
		}
		return raw;
	}

	function getHex(color: { cssVar?: string; hex?: string }): string {
		return color.hex ?? (color.cssVar ? resolveVar(color.cssVar) : "#000000");
	}

	function handleSelect(hex: string) {
		onSelect(hex);
		onClose();
	}

	function handleThemeSelect(color: typeof themeColors[0]) {
		handleSelect(getHex(color));
	}

	function handleNone() {
		onSelect(null);
		onClose();
	}

	function handleCustomApply() {
		if (/^#[0-9a-fA-F]{6}$/.test(customHex)) {
			onSelect(customHex);
			onClose();
		}
	}

	function isSelected(hex: string): boolean {
		if (!value) return false;
		return value.toLowerCase() === hex.toLowerCase();
	}

	function isThemeSelected(color: typeof themeColors[0]): boolean {
		if (!value) return false;
		try {
			return value.toLowerCase() === getHex(color).toLowerCase();
		} catch {
			return false;
		}
	}
</script>

<Modal {open} {onClose} title="Accent Color" width="sm">
	{#snippet children()}
		<!-- Theme Colors -->
		<div class="section">
			<div class="section-label">Theme</div>
			<div class="color-grid">
				<button
					class="swatch none-swatch"
					class:selected={!value}
					onclick={handleNone}
					title="None"
				>
					<Icon icon="ri:close-line" width="14" />
				</button>
				{#each themeColors as color}
					<button
						class="swatch"
						class:selected={isThemeSelected(color)}
						style="--c: {color.cssVar ? `var(${color.cssVar})` : color.hex}"
						onclick={() => handleThemeSelect(color)}
						title={color.name}
					>
						{#if isThemeSelected(color)}
							<span class="check"><Icon icon="ri:check-line" width="14" /></span>
						{/if}
					</button>
				{/each}
			</div>
		</div>

		<!-- Spectrum -->
		<div class="section">
			<div class="section-label">Spectrum</div>
			<div class="color-grid">
				{#each spectrumColors as color}
					<button
						class="swatch"
						class:selected={isSelected(color.hex)}
						style="--c: {color.hex}"
						onclick={() => handleSelect(color.hex)}
						title={color.name}
					>
						{#if isSelected(color.hex)}
							<span class="check"><Icon icon="ri:check-line" width="14" /></span>
						{/if}
					</button>
				{/each}
			</div>
		</div>

		<!-- Custom -->
		<div class="section">
			<div class="section-label">Custom</div>
			<div class="custom-row">
				<label class="custom-picker">
					<input
						type="color"
						bind:value={customHex}
						class="native-color-input"
					/>
					<span class="picker-preview" style="background: {customHex}"></span>
				</label>
				<input
					type="text"
					bind:value={customHex}
					class="hex-input"
					placeholder="#000000"
					maxlength="7"
					spellcheck="false"
					onkeydown={(e) => { if (e.key === "Enter") handleCustomApply(); }}
				/>
				<button
					class="apply-btn"
					onclick={handleCustomApply}
					disabled={!/^#[0-9a-fA-F]{6}$/.test(customHex)}
				>
					Apply
				</button>
			</div>
		</div>
	{/snippet}
</Modal>

<style>
	.section {
		margin-bottom: 16px;
	}

	.section:last-child {
		margin-bottom: 0;
	}

	.section-label {
		font-size: 11px;
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-subtle);
		margin-bottom: 8px;
	}

	.color-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(34px, 1fr));
		gap: 8px;
	}

	/* --- Swatches --- */

	.swatch {
		width: 34px;
		height: 34px;
		border-radius: 50%;
		background: var(--c);
		border: 2px solid transparent;
		cursor: pointer;
		transition: transform 150ms ease, border-color 150ms ease;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.1);
	}

	.swatch:hover {
		transform: scale(1.12);
	}

	.swatch.selected {
		border-color: var(--color-foreground);
		box-shadow:
			inset 0 0 0 1px rgba(0, 0, 0, 0.1),
			0 0 0 2px var(--color-background);
	}

	.check {
		display: flex;
		color: white;
		filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.5));
	}

	/* None swatch */

	.none-swatch {
		background: var(--color-surface-overlay);
		border: 1px dashed var(--color-border);
		color: var(--color-foreground-muted);
		box-shadow: none;
	}

	.none-swatch.selected {
		border-style: solid;
		border-width: 2px;
		border-color: var(--color-foreground);
	}

	/* --- Custom color row --- */

	.custom-row {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.custom-picker {
		position: relative;
		width: 34px;
		height: 34px;
		flex-shrink: 0;
		cursor: pointer;
	}

	.native-color-input {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		opacity: 0;
		cursor: pointer;
	}

	.picker-preview {
		display: block;
		width: 34px;
		height: 34px;
		border-radius: 50%;
		box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.1);
		pointer-events: none;
		/* Rainbow conic hint behind the color */
		background-image:
			conic-gradient(
				from 0deg,
				#f00 0deg, #ff0 60deg, #0f0 120deg,
				#0ff 180deg, #00f 240deg, #f0f 300deg, #f00 360deg
			);
		background-size: cover;
	}

	.hex-input {
		flex: 1;
		min-width: 0;
		padding: 6px 8px;
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--color-foreground);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		outline: none;
		transition: border-color 150ms ease;
	}

	.hex-input:focus {
		border-color: var(--color-border-focus);
	}

	.apply-btn {
		padding: 6px 12px;
		font-size: 12px;
		font-weight: 500;
		color: var(--color-foreground);
		background: var(--color-surface-overlay);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		cursor: pointer;
		transition: all 150ms ease;
		flex-shrink: 0;
	}

	.apply-btn:hover:not(:disabled) {
		background: var(--color-surface-elevated);
	}

	.apply-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
</style>
