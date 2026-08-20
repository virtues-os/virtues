<script lang="ts">
	/**
	 * AliasEditor — the surfaces an entity also answers to.
	 *
	 * Migration 0037 calls an alias "the record of a human decision", and built
	 * the column for exactly this: a mention resolves iff its normalized surface
	 * matches EXACTLY ONE entity by canonical name, nickname, or an alias a
	 * human put here. Linking "Sarah" once backfills every past mention of that
	 * surface and resolves every future one without ever asking again.
	 *
	 * Then nothing ever wrote it. On a real box 3 of 573 people have an alias.
	 * This is the missing half — the decision had a home and no door.
	 *
	 * Deliberately plain: a row of chips and one input. An alias is a short
	 * string a person types when they notice the record calling someone by the
	 * wrong name, and anything more elaborate would make that moment heavier
	 * than the correction deserves.
	 */
	import Icon from '$lib/components/Icon.svelte';

	interface Props {
		aliases: string[];
		/** Persist. Receives the full next list; the server normalizes again. */
		onSave: (next: string[]) => Promise<void>;
		/** Shown as the reason this entity might need one. */
		canonicalName?: string;
	}

	let { aliases, onSave, canonicalName }: Props = $props();

	let draft = $state('');
	let saving = $state(false);
	let failed = $state<string | null>(null);

	// Optimistic overlay, so a chip disappears the instant you dismiss it rather
	// than after a round trip. `null` means "nothing pending — show the server's
	// list", which is also the state after the parent hands us a different
	// entity, so switching people cannot leave the previous one's chips on
	// screen.
	let pending = $state<string[] | null>(null);
	const local = $derived(pending ?? aliases);
	$effect(() => {
		aliases;
		pending = null;
	});

	/**
	 * Lowercased, trimmed, deduped — matching what the server stores.
	 *
	 * 0037 matches with `aliases ? lower(surface)`, so a mixed-case alias is
	 * invisible to the resolver: it would look saved and quietly resolve
	 * nothing. The server normalizes too; doing it here as well means the chip
	 * you see is the string that will actually match.
	 */
	function normalize(v: string): string {
		return v.trim().toLowerCase();
	}

	async function commit(next: string[]) {
		saving = true;
		failed = null;
		const previous = local;
		pending = next;
		try {
			await onSave(next);
		} catch (e) {
			pending = previous;
			failed = e instanceof Error ? e.message : 'Could not save';
		} finally {
			saving = false;
		}
	}

	async function add() {
		const a = normalize(draft);
		if (!a) return;
		// Adding a name the entity already answers to is a no-op, not an error.
		if (local.includes(a) || normalize(canonicalName ?? '') === a) {
			draft = '';
			return;
		}
		draft = '';
		await commit([...local, a]);
	}

	async function remove(a: string) {
		await commit(local.filter((x) => x !== a));
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			void add();
		} else if (e.key === 'Backspace' && draft === '' && local.length) {
			e.preventDefault();
			void remove(local[local.length - 1]);
		}
	}
</script>

<div class="aliases" class:saving>
	{#each local as alias (alias)}
		<span class="chip">
			{alias}
			<button
				type="button"
				class="chip-x"
				aria-label="Remove alias {alias}"
				disabled={saving}
				onclick={() => remove(alias)}
			>
				<Icon icon="ri:close-line" width="12" />
			</button>
		</span>
	{/each}

	<input
		class="alias-input"
		type="text"
		bind:value={draft}
		onkeydown={onKeydown}
		onblur={add}
		disabled={saving}
		placeholder={local.length ? 'Add another…' : 'Also known as…'}
		aria-label="Add an alias"
	/>
</div>

{#if failed}
	<p class="alias-error">{failed}</p>
{/if}

<style>
	@reference "../../../app.css";

	.aliases {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.375rem;
	}

	.aliases.saving {
		opacity: 0.6;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.125rem 0.25rem 0.125rem 0.5rem;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		font-size: 12px;
		color: var(--color-foreground);
		white-space: nowrap;
	}

	.chip-x {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		border-radius: 999px;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.chip-x:hover:not(:disabled) {
		color: var(--color-foreground);
		background: var(--color-surface-hover);
	}

	/* Borderless until focused: a row of empty boxes reads as a form to fill
	   in, and most entities never need an alias at all. */
	.alias-input {
		flex: 1;
		min-width: 8ch;
		padding: 0.125rem 0.25rem;
		border: none;
		background: transparent;
		font-size: 12px;
		color: var(--color-foreground);
	}

	.alias-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.alias-input:focus {
		outline: none;
		border-bottom: 1px solid var(--color-border);
	}

	.alias-error {
		margin-top: 0.25rem;
		font-size: 12px;
		color: var(--color-danger, #b00);
	}
</style>
