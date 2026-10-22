<script lang="ts">
	import { untrack } from "svelte";
	import Button from "$lib/components/Button.svelte";
	import { linkEditor } from "$lib/stores/linkEditor.svelte";
	import { inComposition } from "$lib/utils/ime";

	let labelInput = $state<HTMLInputElement | null>(null);
	let hrefInput = $state<HTMLInputElement | null>(null);

	/** On touch the panel's buttons are a thumb's 44pt (design-grammar §6): `lg` is 48px, `sm` 32px. */
	const touch = typeof window !== "undefined" && !!window.matchMedia?.("(pointer: coarse)").matches;

	// A new link (⌘K on words) has its words and needs an address: the focus
	// goes there, so a pasted URL lands in it, not over the words. An existing
	// link's label is what the reader sees, so it is what someone usually came
	// to fix; it is selected so a correction can just be typed over.
	$effect(() => {
		// Read once, as the panel opens: not again as the address is typed.
		const input = untrack(() => linkEditor.href) ? labelInput : hrefInput;
		if (input) {
			input.focus();
			input.select();
		}
	});

	function onKeydown(e: KeyboardEvent) {
		if (inComposition(e)) return;
		if (e.key === "Enter") {
			e.preventDefault();
			linkEditor.save();
		} else if (e.key === "Escape") {
			e.preventDefault();
			linkEditor.hide();
		}
	}
</script>

<div
	class="link-editor"
	onkeydown={onKeydown}
	role="dialog"
	tabindex="-1"
	aria-label="Edit link"
>
	<label class="field">
		<span class="field-label">Text</span>
		<input
			bind:this={labelInput}
			bind:value={linkEditor.label}
			class="field-input"
			type="text"
			spellcheck="false"
		/>
	</label>

	<label class="field">
		<span class="field-label">Link</span>
		<input
			bind:this={hrefInput}
			bind:value={linkEditor.href}
			class="field-input"
			type="text"
			spellcheck="false"
		/>
	</label>

	<div class="actions">
		<Button variant="secondary" size={touch ? "lg" : "sm"} onclick={() => linkEditor.hide()}>Cancel</Button>
		<Button variant="primary" size={touch ? "lg" : "sm"} onclick={() => linkEditor.save()}>Save</Button>
	</div>
</div>

<style>
	.link-editor {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px;
		min-width: 300px;
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: var(--card-radius);
		box-shadow: var(--shadow-float);
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.field-label {
		font-size: 11px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}

	.field-input {
		padding: 6px 8px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
		font-size: 13px;
		font-family: var(--font-sans);
	}

	.field-input:focus {
		outline: none;
		border-color: var(--color-primary);
	}

	.actions {
		display: flex;
		justify-content: flex-end;
		gap: 6px;
		margin-top: 2px;
	}
</style>
