<script lang="ts">
	import "iconify-icon";

	let {
		type = "text",
		value = $bindable(""),
		placeholder = "",
		disabled = false,
		required = false,
		delight = true,
		autoSave = false,
		onSave,
		label,
		helperText,
		warning,
		success = false,
		loading = false,
		clearable = false,
		id,
		name,
		step,
		min,
		max,
		autocomplete,
		class: className = "",
		oninput,
		onchange,
		onblur,
		onfocus,
		onkeydown,
	} = $props<{
		type?:
			| "text"
			| "number"
			| "date"
			| "email"
			| "password"
			| "tel"
			| "url";
		value?: string | number;
		placeholder?: string;
		disabled?: boolean;
		required?: boolean;
		delight?: boolean;
		autoSave?: boolean;
		onSave?: (value: string | number) => Promise<void>;
		label?: string;
		helperText?: string;
		warning?: boolean | string;
		success?: boolean;
		loading?: boolean;
		clearable?: boolean;
		id?: string;
		name?: string;
		step?: string | number;
		min?: string | number;
		max?: string | number;
		autocomplete?: string;
		class?: string;
		oninput?: (e: Event) => void;
		onchange?: (e: Event) => void;
		onblur?: (e: Event) => void;
		onfocus?: (e: Event) => void;
		onkeydown?: (e: KeyboardEvent) => void;
	}>();

	// Internal state
	let isFocused = $state(false);
	let isHovered = $state(false);
	let isTyping = $state(false);
	let isSaving = $state(false);
	let saveError = $state(false);
	let hasSaved = $state(false);
	let typingTimeout: ReturnType<typeof setTimeout> | null = null;
	let initialValue: string | number = "";

	// Derived state
	let hasContent = $derived(
		value !== undefined && value !== null && String(value).length > 0,
	);

	// Generate ID for label association if not provided
	let inputId = $derived(
		id ||
			(label
				? `input-${Math.random().toString(36).substr(2, 9)}`
				: undefined),
	);

	// Warning message (if warning is a string, use it; otherwise use helperText)
	let displayHelperText = $derived(
		typeof warning === "string" ? warning : helperText,
	);

	function handleClear() {
		value = "";
		// Focus the input after clearing
		setTimeout(() => {
			const input = document.getElementById(inputId || "");
			input?.focus();
		}, 0);
	}

	function handleInput(e: Event) {
		// Reset error state when user starts typing again
		if (saveError) saveError = false;
		if (hasSaved) hasSaved = false;

		if (delight) {
			isTyping = true;
			if (typingTimeout) clearTimeout(typingTimeout);
			typingTimeout = setTimeout(() => {
				isTyping = false;
			}, 1000);
		}
		oninput?.(e);
	}

	function handleFocus(e: Event) {
		isFocused = true;
		initialValue = value;
		// Reset states on focus
		saveError = false;
		hasSaved = false;
		onfocus?.(e);
	}

	async function handleBlur(e: Event) {
		isFocused = false;
		isTyping = false;
		if (typingTimeout) clearTimeout(typingTimeout);

		// Auto-save if enabled and value changed
		if (autoSave && onSave && value !== initialValue) {
			isSaving = true;
			saveError = false;
			try {
				await onSave(value);
				hasSaved = true;
			} catch (err) {
				saveError = true;
				console.error("Auto-save failed:", err);
			} finally {
				isSaving = false;
			}
		}

		onblur?.(e);
	}
</script>

<div class="input-container">
	{#if label}
		<label for={inputId} class="input-label">
			{label}
			{#if required}<span class="required-indicator">*</span>{/if}
		</label>
	{/if}

	<div
		class="input-wrapper {className}"
		class:delight
		class:hovered={isHovered}
		class:focused={isFocused}
		class:has-content={hasContent}
		class:saving={isSaving}
		class:saved={hasSaved || success}
		class:error={saveError}
		class:warning={!!warning}
		class:disabled
		onmouseenter={() => (isHovered = true)}
		onmouseleave={() => (isHovered = false)}
		role="none"
	>
		<input
			{type}
			bind:value
			placeholder={delight ? "" : placeholder}
			{disabled}
			{required}
			id={inputId}
			{name}
			{step}
			{min}
			{max}
			{autocomplete}
			class="input-field"
			oninput={handleInput}
			{onchange}
			onblur={handleBlur}
			onfocus={handleFocus}
			{onkeydown}
		/>

		{#if delight}
			<span class="placeholder" class:hidden={isFocused || hasContent}>
				{placeholder}
			</span>

			<!-- Icons container -->
			<div class="icons">
				<!-- Clear button (clearable, has content, not disabled) -->
				{#if clearable && hasContent && !disabled}
					<button
						type="button"
						class="icon icon-clear"
						onclick={handleClear}
						aria-label="Clear input"
					>
						<iconify-icon
							icon="mdi:close-circle"
							width="14"
							height="14"
						></iconify-icon>
					</button>
				{/if}

				<!-- Hover hint (unfocused, not saved/error/saving) -->
				<div
					class="icon icon-hover"
					class:visible={isHovered &&
						!isFocused &&
						!hasSaved &&
						!saveError &&
						!isSaving &&
						!warning}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="none">
						<circle
							cx="8"
							cy="8"
							r="6"
							stroke="currentColor"
							stroke-width="1.5"
							stroke-dasharray="3 2"
						/>
					</svg>
				</div>

				<!-- Waiting dot (focused, empty or paused) -->
				<div
					class="icon icon-dot"
					class:visible={isFocused && (!hasContent || !isTyping)}
				></div>

				<!-- Typing indicator (focused, has content, actively typing) -->
				<div
					class="icon icon-typing"
					class:visible={isFocused && hasContent && isTyping}
				>
					<svg width="14" height="14" viewBox="0 0 16 16" fill="none">
						<text
							x="1"
							y="12"
							font-family="var(--font-serif)"
							font-size="11"
							font-weight="500"
							fill="currentColor">Aa</text
						>
					</svg>
				</div>

				<!-- Warning icon (warning state, not focused) -->
				<div
					class="icon icon-warning"
					class:visible={!!warning &&
						!isFocused &&
						!saveError &&
						!isSaving}
				>
					<iconify-icon icon="mdi:alert" width="14" height="14"
					></iconify-icon>
				</div>

				<!-- Saving/loading spinner -->
				<div
					class="icon icon-saving"
					class:visible={isSaving || loading}
				>
					<iconify-icon
						icon="mdi:loading"
						width="14"
						height="14"
						class="spinner"
					></iconify-icon>
				</div>

				<!-- Saved check (unfocused, saved successfully or success prop) -->
				<div
					class="icon icon-check"
					class:visible={!isFocused &&
						(hasSaved || success) &&
						!saveError &&
						!isSaving &&
						!loading &&
						!warning}
				>
					<iconify-icon icon="mdi:check" width="14" height="14"
					></iconify-icon>
				</div>

				<!-- Error X -->
				<div
					class="icon icon-error"
					class:visible={saveError && !isFocused}
				>
					<iconify-icon icon="mdi:close" width="14" height="14"
					></iconify-icon>
				</div>
			</div>

			<div class="focus-ring"></div>
		{/if}
	</div>

	{#if displayHelperText}
		<p class="input-helper" class:warning={typeof warning === "string"}>
			{displayHelperText}
		</p>
	{/if}
</div>

<style>
	/* ===================================
	   INPUT CONTAINER
	   =================================== */
	.input-container {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.input-label {
		font-family: var(--font-sans);
		font-size: 14px;
		font-weight: 500;
		color: var(--color-foreground);
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.required-indicator {
		color: var(--color-error);
	}

	.input-helper {
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-subtle);
		margin: 0;
		line-height: 1.4;
	}

	.input-helper.warning {
		color: var(--color-warning);
	}

	/* ===================================
	   INPUT WRAPPER
	   =================================== */
	.input-wrapper {
		position: relative;
		border-radius: 8px;
	}

	.input-field {
		width: 100%;
		padding: 10px 14px;
		font-family: var(--font-sans);
		font-size: 15px;
		color: var(--color-foreground);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		outline: none;
		transition: border-color 0.25s ease;
		position: relative;
		z-index: 1;
	}

	/* Remove number input spinners */
	.input-field[type="number"] {
		-moz-appearance: textfield;
		appearance: textfield;
	}

	.input-field[type="number"]::-webkit-outer-spin-button,
	.input-field[type="number"]::-webkit-inner-spin-button {
		-webkit-appearance: none;
		margin: 0;
	}

	/* Extra right padding when delight is on (for icons) */
	.input-wrapper.delight .input-field {
		padding-right: 36px;
	}

	/* Extra padding when clearable (clear button takes space) */
	.input-wrapper.delight:has(.icon-clear) .input-field {
		padding-right: 50px;
	}

	.input-field::placeholder {
		color: var(--color-foreground-subtle);
	}

	/* Disabled state */
	.input-wrapper.disabled .input-field {
		opacity: 0.8;
		cursor: not-allowed;
		background: var(--color-surface-elevated);
	}

	/* Disabled hover state */
	.input-wrapper.disabled.hovered .input-field {
		border-color: var(--color-border);
		opacity: 0.7;
	}

	/* Hover state (works even with content) */
	.input-wrapper.hovered:not(.focused):not(.disabled):not(.saved):not(
			.error
		):not(.warning)
		.input-field {
		border-color: var(--color-border-strong);
	}

	/* Focus state */
	.input-wrapper.focused .input-field {
		border-color: var(--color-primary);
	}

	/* Saved state (success border tint) */
	.input-wrapper.delight.saved:not(.focused):not(.error) .input-field {
		border-color: color-mix(
			in srgb,
			var(--color-border-strong) 70%,
			var(--color-success) 30%
		);
	}

	/* Warning state */
	.input-wrapper.warning:not(.focused):not(.error) .input-field {
		border-color: color-mix(
			in srgb,
			var(--color-border-strong) 70%,
			var(--color-warning) 30%
		);
	}

	/* Error state */
	.input-wrapper.error:not(.focused) .input-field {
		border-color: color-mix(
			in srgb,
			var(--color-border-strong) 50%,
			var(--color-error) 50%
		);
		animation: shake 0.4s ease-out;
	}

	@keyframes shake {
		0%,
		100% {
			transform: translateX(0);
		}
		20% {
			transform: translateX(-4px);
		}
		40% {
			transform: translateX(4px);
		}
		60% {
			transform: translateX(-2px);
		}
		80% {
			transform: translateX(2px);
		}
	}

	/* ===================================
	   PLACEHOLDER
	   =================================== */
	.placeholder {
		position: absolute;
		left: 15px;
		top: 50%;
		transform: translateY(-50%);
		font-family: var(--font-sans);
		font-size: 15px;
		color: var(--color-foreground-subtle);
		pointer-events: none;
		z-index: 2;
		transition:
			opacity 0.3s ease,
			transform 0.3s cubic-bezier(0.16, 1, 0.3, 1);
	}

	.placeholder.hidden {
		opacity: 0;
		transform: translateY(-50%) translateX(-8px);
	}

	/* ===================================
	   ICONS
	   =================================== */
	.icons {
		position: absolute;
		right: 12px;
		top: 50%;
		transform: translateY(-50%);
		z-index: 2;
		width: 16px;
		height: 16px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.icon {
		position: absolute;
		opacity: 0;
		transform: scale(0.6);
		transition:
			opacity 0.25s ease,
			transform 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	.icon.visible {
		opacity: 1;
		transform: scale(1);
	}

	/* Clear button */
	.icon-clear {
		cursor: pointer;
		background: none;
		border: none;
		padding: 0;
		color: var(--color-foreground-subtle);
		transition:
			color 0.2s ease,
			opacity 0.2s ease;
	}

	.icon-clear:hover {
		color: var(--color-foreground);
	}

	.icon-clear.visible {
		opacity: 1;
	}

	/* Hover hint */
	.icon-hover {
		color: var(--color-foreground-subtle);
	}

	.icon-hover.visible {
		opacity: 0.6;
	}

	/* Waiting dot */
	.icon-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
	}

	.icon-dot.visible {
		opacity: 0.6;
	}

	/* Typing */
	.icon-typing {
		color: var(--color-foreground-subtle);
	}

	.icon-typing.visible {
		opacity: 0.8;
	}

	/* Warning icon */
	.icon-warning {
		color: var(--color-warning);
	}

	.icon-warning.visible {
		opacity: 0.8;
	}

	/* Saving spinner */
	.icon-saving {
		color: var(--color-foreground-subtle);
	}

	.icon-saving.visible {
		opacity: 0.8;
	}

	.spinner {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	/* Saved check */
	.icon-check {
		color: var(--color-success);
	}

	.icon-check.visible {
		opacity: 1;
	}

	/* Error X */
	.icon-error {
		color: var(--color-error);
	}

	.icon-error.visible {
		opacity: 1;
	}

	/* ===================================
	   FOCUS RING
	   =================================== */
	.focus-ring {
		position: absolute;
		inset: 0;
		border-radius: 8px;
		box-shadow: 0 0 0 2px
			color-mix(in srgb, var(--color-primary) 25%, transparent);
		opacity: 0;
		pointer-events: none;
		z-index: 0;
		transition: opacity 0.25s ease;
	}

	.input-wrapper.focused .focus-ring {
		opacity: 1;
	}
</style>
