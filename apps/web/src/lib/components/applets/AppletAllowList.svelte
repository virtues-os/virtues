<script lang="ts">
	/**
	 * What an applet's file asks for and the owner allows: hosts its code may
	 * reach (`net(host)`), or connections whose tools its runs may use
	 * (`linear__*`). Each opens only when it is switched on here. This is the
	 * one place either can be allowed: not the applet's file, not the
	 * assistant's applet tool. That is what keeps an applet that reads your
	 * records from also sending them somewhere on its own.
	 *
	 * Shown only when the applet asks for at least one.
	 */
	type Grant = { requested: string[]; allowed: string[]; connected?: string[] };

	let {
		appletId,
		label,
		hint,
		what,
		load,
		save,
		unavailable = () => null
	}: {
		appletId: string;
		label: string;
		hint: string;
		/** What the list holds, for an error: "hosts", "connections". */
		what: string;
		load: (id: string) => Promise<Grant>;
		save: (id: string, allowed: string[]) => Promise<Grant>;
		/** Why an item cannot be allowed yet, or null when it can. */
		unavailable?: (item: string, grant: Grant) => string | null;
	} = $props();

	let grant = $state<Grant | null>(null);
	let saving = $state<string | null>(null);
	let error = $state<string | null>(null);

	$effect(() => {
		const id = appletId;
		grant = null;
		error = null;
		load(id)
			.then((g) => {
				if (id === appletId) grant = g;
			})
			.catch((e) => {
				error = `Your server couldn't read this applet's ${what}: ${e instanceof Error ? e.message : e}`;
			});
	});

	async function toggle(item: string) {
		if (!grant) return;
		const on = grant.allowed.includes(item);
		const next = on ? grant.allowed.filter((h) => h !== item) : [...grant.allowed, item];
		saving = item;
		error = null;
		try {
			grant = await save(appletId, next);
		} catch (e) {
			error = `Your server didn't save that change: ${e instanceof Error ? e.message : e}. Try again.`;
		} finally {
			saving = null;
		}
	}
</script>

{#if error}
	<p class="error">{error}</p>
{/if}
{#if grant && grant.requested.length > 0}
	{@const current = grant}
	<div class="field">
		<span class="label">{label}</span>
		<ul class="items">
			{#each current.requested as item (item)}
				{@const on = current.allowed.includes(item)}
				{@const why = unavailable(item, current)}
				<li>
					<span class="item">
						<code>{item}</code>
						{#if why}<span class="hint">{why}</span>{/if}
					</span>
					<button
						type="button"
						class="switch"
						class:on
						role="switch"
						aria-checked={on}
						aria-label="Allow {item}"
						disabled={saving !== null || (why !== null && !on)}
						onclick={() => toggle(item)}
					></button>
				</li>
			{/each}
		</ul>
		<span class="hint">{hint}</span>
	</div>
{/if}

<style>
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.label {
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.items {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.items li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.item {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.items code {
		font-size: 13px;
	}
	.hint {
		font-size: 12px;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}
	.error {
		margin: 0;
		font-size: 13px;
		color: var(--color-error);
	}
	.switch {
		width: 44px;
		height: 26px;
		border-radius: var(--radius-full);
		background: var(--color-border);
		position: relative;
		flex: none;
		border: 0;
		cursor: pointer;
		transition: background 150ms ease;
		padding: 0;
	}
	.switch.on {
		background: var(--color-success);
	}
	.switch::after {
		content: '';
		position: absolute;
		top: 3px;
		left: 3px;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: var(--color-background);
		transition: transform 150ms ease;
	}
	.switch.on::after {
		transform: translateX(18px);
	}
	.switch:focus-visible {
		outline: 2px solid var(--color-foreground);
		outline-offset: 2px;
	}
	.switch:disabled {
		opacity: 0.5;
		cursor: default;
	}
	@media (prefers-reduced-motion: reduce) {
		.switch,
		.switch::after {
			transition: none;
		}
	}
</style>
