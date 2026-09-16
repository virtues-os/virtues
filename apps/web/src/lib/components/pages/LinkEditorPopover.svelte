<script lang="ts">
	import Button from "$lib/components/Button.svelte";
	import { linkEditor } from "$lib/stores/linkEditor.svelte";

	let labelInput = $state<HTMLInputElement | null>(null);

	// The label is what the reader sees, so it is what someone usually came to
	// fix; select it so a correction can just be typed over.
	$effect(() => {
		if (labelInput) {
			labelInput.focus();
			labelInput.select();
		}
	});

	function onKeydown(e: KeyboardEvent) {
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
			bind:value={linkEditor.href}
			class="field-input"
			type="text"
			spellcheck="false"
		/>
	</label>

	<div class="actions">
		<Button variant="secondary" size="sm" onclick={() => linkEditor.hide()}>Cancel</Button>
		<Button variant="primary" size="sm" onclick={() => linkEditor.save()}>Save</Button>
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
		border-radius: 10px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.12);
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
