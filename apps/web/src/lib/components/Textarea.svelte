<script lang="ts">
	import "iconify-icon";

	let {
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
		id,
		name,
		rows = 4,
		maxRows,
		autoResize = false,
		class: className = "",
		oninput,
		onchange,
		onblur,
		onfocus,
	} = $props<{
		value?: string;
		placeholder?: string;
		disabled?: boolean;
		required?: boolean;
		delight?: boolean;
		autoSave?: boolean;
		onSave?: (value: string) => Promise<void>;
		label?: string;
		helperText?: string;
		warning?: boolean | string;
		id?: string;
		name?: string;
		rows?: number;
		maxRows?: number;
		autoResize?: boolean;
		class?: string;
		oninput?: (e: Event) => void;
		onchange?: (e: Event) => void;
		onblur?: (e: Event) => void;
		onfocus?: (e: Event) => void;
	}>();

	// Internal state
	let isFocused = $state(false);
	let isHovered = $state(false);
	let isTyping = $state(false);
	let isSaving = $state(false);
	let saveError = $state(false);
	let hasSaved = $state(false);
	let typingTimeout: ReturnType<typeof setTimeout> | null = null;
	let initialValue: string = "";
	let textareaEl: HTMLTextAreaElement | null = null;

	// Derived state
	let hasContent = $derived(
		value !== undefined && value !== null && String(value).length > 0,
	);

	// Generate ID for label association if not provided
	let textareaId = $derived(
		id ||
			(label
				? `textarea-${Math.random().toString(36).substr(2, 9)}`
				: undefined),
	);

	// Warning message (if warning is a string, use it; otherwise use helperText)
	let displayHelperText = $derived(
		typeof warning === "string" ? warning : helperText,
	);

	function handleAutoResize() {
		if (!autoResize || !textareaEl) return;

		// Reset height to auto to get the correct scrollHeight
		textareaEl.style.height = "auto";

		// Calculate line height from computed styles
		const computedStyle = getComputedStyle(textareaEl);
		const lineHeight = parseFloat(computedStyle.lineHeight) || 22;
		const paddingTop = parseFloat(computedStyle.paddingTop) || 10;
		const paddingBottom = parseFloat(computedStyle.paddingBottom) || 10;
		const borderTop = parseFloat(computedStyle.borderTopWidth) || 1;
		const borderBottom = parseFloat(computedStyle.borderBottomWidth) || 1;

		const minHeight =
			rows * lineHeight +
			paddingTop +
			paddingBottom +
			borderTop +
			borderBottom;
		const maxHeight = maxRows
			? maxRows * lineHeight +
				paddingTop +
				paddingBottom +
				borderTop +
				borderBottom
			: Infinity;

		const newHeight = Math.min(
			Math.max(textareaEl.scrollHeight, minHeight),
			maxHeight,
		);
		textareaEl.style.height = `${newHeight}px`;
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

		handleAutoResize();
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

	// Initial auto-resize on mount
	$effect(() => {
		if (autoResize && textareaEl) {
			handleAutoResize();
		}
	});
</script>

<div class="textarea-container">
	{#if label}
		<label for={textareaId} class="textarea-label">
			{label}
			{#if required}<span class="required-indicator">*</span>{/if}
		</label>
	{/if}

	<div
		class="textarea-wrapper {className}"
		class:delight
		class:hovered={isHovered}
		class:focused={isFocused}
		class:has-content={hasContent}
		class:saving={isSaving}
		class:saved={hasSaved}
		class:error={saveError}
		class:warning={!!warning}
		class:disabled
		onmouseenter={() => (isHovered = true)}
		onmouseleave={() => (isHovered = false)}
		role="none"
	>
		<textarea
			bind:this={textareaEl}
			bind:value
			placeholder={delight ? "" : placeholder}
			{disabled}
			{required}
			id={textareaId}
			{name}
			{rows}
			class="textarea-field"
			oninput={handleInput}
			{onchange}
			onblur={handleBlur}
			onfocus={handleFocus}
		></textarea>

		{#if delight}
			<span class="placeholder" class:hidden={isFocused || hasContent}>
				{placeholder}
			</span>

			<!-- Icons container -->
			<div class="icons">
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

				<!-- Saving spinner -->
				<div class="icon icon-saving" class:visible={isSaving}>
					<iconify-icon
						icon="mdi:loading"
						width="14"
						height="14"
						class="spinner"
					></iconify-icon>
				</div>

				<!-- Saved check (unfocused, saved successfully) -->
				<div
					class="icon icon-check"
					class:visible={!isFocused &&
						hasSaved &&
						!saveError &&
						!isSaving &&
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
		<p class="textarea-helper" class:warning={typeof warning === "string"}>
			{displayHelperText}
		</p>
	{/if}
</div>

<style>
	/* ===================================
	   TEXTAREA CONTAINER
	   =================================== */
	.textarea-container {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.textarea-label {
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

	.textarea-helper {
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-subtle);
		margin: 0;
		line-height: 1.4;
	}

	.textarea-helper.warning {
		color: var(--color-warning);
	}

	/* ===================================
	   TEXTAREA WRAPPER
	   =================================== */
	.textarea-wrapper {
		position: relative;
		border-radius: 8px;
	}

	.textarea-field {
		width: 100%;
		padding: 10px 14px;
		font-family: var(--font-sans);
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		outline: none;
		transition: border-color 0.25s ease;
		position: relative;
		z-index: 1;
		resize: none;
		/* Prevent extra space at bottom from inline element baseline */
		display: block;
		vertical-align: top;
	}

	/* Extra right padding when delight is on (for icons) */
	.textarea-wrapper.delight .textarea-field {
		padding-right: 36px;
	}

	.textarea-field::placeholder {
		color: var(--color-foreground-subtle);
	}

	/* Disabled state */
	.textarea-wrapper.disabled .textarea-field {
		opacity: 0.8;
		cursor: not-allowed;
		background: var(--color-surface-elevated);
	}

	/* Disabled hover state */
	.textarea-wrapper.disabled.hovered .textarea-field {
		border-color: var(--color-border);
		opacity: 0.9;
	}

	/* Hover state (works even with content) */
	.textarea-wrapper.hovered:not(.focused):not(.disabled):not(.saved):not(
			.error
		):not(.warning)
		.textarea-field {
		border-color: var(--color-border-strong);
	}

	/* Focus state */
	.textarea-wrapper.focused .textarea-field {
		border-color: var(--color-primary);
	}

	/* Saved state (success border tint) */
	.textarea-wrapper.delight.saved:not(.focused):not(.error) .textarea-field {
		border-color: color-mix(
			in srgb,
			var(--color-border-strong) 70%,
			var(--color-success) 30%
		);
	}

	/* Warning state */
	.textarea-wrapper.warning:not(.focused):not(.error) .textarea-field {
		border-color: color-mix(
			in srgb,
			var(--color-border-strong) 70%,
			var(--color-warning) 30%
		);
	}

	/* Error state */
	.textarea-wrapper.error:not(.focused) .textarea-field {
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
		top: 11px;
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
		transform: translateX(-8px);
	}

	/* ===================================
	   ICONS
	   =================================== */
	.icons {
		position: absolute;
		right: 12px;
		top: 12px;
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

	.textarea-wrapper.focused .focus-ring {
		opacity: 1;
	}
</style>
