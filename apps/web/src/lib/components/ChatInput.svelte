<script lang="ts">
	import { createEventDispatcher } from "svelte";
	import { animate } from "motion";

	let {
		value = $bindable(""),
		disabled = false,
		sendDisabled = false,
		placeholder = "Message...",
		maxWidth = "max-w-3xl",
		focused = $bindable(false),
	} = $props();

	const dispatch = createEventDispatcher<{ submit: string }>();

	let textarea: HTMLTextAreaElement;
	let isFocused = $state(false);
	let isMultiline = $state(false);

	// Sync internal focus state with external bindable prop
	$effect(() => {
		focused = isFocused;
	});

	// Svelte action to animate border-radius
	function animateRadius(node: HTMLElement) {
		let currentMultiline = false;

		return {
			update(multiline: boolean) {
				if (multiline === currentMultiline) return;
				currentMultiline = multiline;

				const targetRadius = multiline ? "12px" : "24px";

				animate(
					node,
					{ borderRadius: targetRadius },
					{ duration: 0.3, easing: [0.4, 0, 0.2, 1] },
				);
			},
		};
	}

	function syncSize() {
		if (!textarea) return;
		textarea.style.height = "auto";
		const nextHeight = Math.min(textarea.scrollHeight, 220);
		textarea.style.height = `${nextHeight}px`;
		isMultiline = nextHeight > 60 || value.includes("\n");
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Enter" && !e.shiftKey) {
			e.preventDefault();
			handleSubmit();
		}
	}

	function handleSubmit() {
		if (!value.trim() || disabled) return;
		dispatch("submit", value);
		value = "";
		isMultiline = false;
		if (textarea) {
			textarea.style.height = "auto";
		}
	}

	function handleWrapperClick(e: MouseEvent) {
		// Don't focus if clicking on a button, dropdown, or interactive element
		const target = e.target as HTMLElement;
		if (
			target.tagName === "BUTTON" ||
			target.closest("button") ||
			target.classList.contains("z-50") || // Don't focus when clicking dropdown menus
			target.closest(".z-50") // Don't focus when clicking inside dropdown menus
		) {
			return;
		}
		// Focus the textarea
		if (textarea) {
			textarea.focus();
		}
	}
</script>

<div class="chat-input-container {maxWidth} w-full">
	<div
		use:animateRadius={isMultiline}
		aria-label="Chat input"
		class="chat-input-wrapper bg-surface border border-border-strong hover:border-primary/60 cursor-text"
		class:focused={isFocused}
		onclick={handleWrapperClick}
		role="button"
		tabindex="-1"
	>
		<label for="chat-input" class="sr-only">Message</label>
		<div class="input-row relative flex items-center w-full">
			<textarea
				id="chat-input"
				bind:this={textarea}
				bind:value
				oninput={syncSize}
				onkeydown={handleKeydown}
				onfocus={() => {
					isFocused = true;
					syncSize();
				}}
				onblur={() => {
					isFocused = false;
				}}
				{placeholder}
				{disabled}
				rows="1"
				class="chat-textarea w-full resize-none outline-none text-foreground placeholder:text-foreground-subtle font-sans text-base bg-transparent pl-4 pr-12 py-3"
			></textarea>
			<button
				type="button"
				onclick={handleSubmit}
				disabled={!value.trim() || sendDisabled}
				class="send-button absolute right-0 bottom-2 w-8 h-8 btn-primary cursor-pointer rounded-full disabled:opacity-50 disabled:cursor-not-allowed transition-all flex items-center justify-center group"
			>
				{#if sendDisabled}
					<iconify-icon
						icon="ri:loader-4-line"
						class="animate-spin"
						style="color: inherit"
						width="16"
					></iconify-icon>
				{:else}
					<iconify-icon
						icon="ri:arrow-up-line"
						width="16"
						class="transition-transform duration-300 group-hover:rotate-45"
						style="color: inherit"
					></iconify-icon>
				{/if}
			</button>
		</div>
	</div>
</div>

<style>
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border-width: 0;
	}

	textarea {
		font-family: var(--font-sans);
	}

	/* Custom scrollbar for textarea */
	textarea::-webkit-scrollbar {
		width: 6px;
	}

	textarea::-webkit-scrollbar-track {
		background: transparent;
	}

	textarea::-webkit-scrollbar-thumb {
		background: var(--color-border-subtle);
		border-radius: 3px;
	}

	textarea::-webkit-scrollbar-thumb:hover {
		background: var(--color-border-strong);
	}

	.chat-input-wrapper {
		border-radius: 24px;
		transition:
			border-color 0.3s cubic-bezier(0.4, 0, 0.2, 1),
			box-shadow 0.3s cubic-bezier(0.4, 0, 0.2, 1);
		min-height: 48px;
		padding-right: 0.75rem;
	}

	.chat-input-wrapper.focused {
		border-color: var(--color-primary) !important;
		box-shadow:
			0 1px 2px 0 rgb(0 0 0 / 0.05),
			0 0 0 3px color-mix(in srgb, var(--color-primary) 40%, transparent) !important;
	}

	.chat-textarea {
		min-height: 48px;
		max-height: 220px;
		line-height: 1.5;
	}
</style>
