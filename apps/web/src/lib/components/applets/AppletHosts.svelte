<script lang="ts">
	import { getAppletHosts, putAppletHosts, type AppletHosts } from '$lib/api/client';

	/**
	 * The hosts an applet's code may reach. Its file asks for hosts with
	 * `net(host)`; a host opens only when you allow it here. This is the one
	 * place a host can be allowed: not the applet's file, not the assistant's
	 * applet tool. That is what keeps an applet that reads your records from
	 * also sending them somewhere on its own.
	 *
	 * Shown only when the applet asks for at least one host.
	 */
	let { appletId }: { appletId: string } = $props();

	let hosts = $state<AppletHosts | null>(null);
	let saving = $state<string | null>(null);
	let error = $state<string | null>(null);

	$effect(() => {
		const id = appletId;
		hosts = null;
		error = null;
		getAppletHosts(id)
			.then((h) => {
				if (id === appletId) hosts = h;
			})
			.catch((e) => {
				error = `Your server couldn't read this applet's hosts: ${e instanceof Error ? e.message : e}`;
			});
	});

	async function toggle(host: string) {
		if (!hosts) return;
		const on = hosts.allowed.includes(host);
		const next = on ? hosts.allowed.filter((h) => h !== host) : [...hosts.allowed, host];
		saving = host;
		error = null;
		try {
			hosts = await putAppletHosts(appletId, next);
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
{#if hosts && hosts.requested.length > 0}
	<div class="field">
		<span class="label">Hosts it can reach</span>
		<ul class="hosts">
			{#each hosts.requested as host (host)}
				{@const on = hosts.allowed.includes(host)}
				<li>
					<code>{host}</code>
					<button
						type="button"
						class="switch"
						class:on
						role="switch"
						aria-checked={on}
						aria-label="Allow {host}"
						disabled={saving !== null}
						onclick={() => toggle(host)}
					></button>
				</li>
			{/each}
		</ul>
		<span class="hint">
			Its code asks to reach these hosts. Nothing goes out until you allow a host here. A host you
			allow can receive what the applet reads, including your records.
		</span>
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
	.hosts {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.hosts li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.hosts code {
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
