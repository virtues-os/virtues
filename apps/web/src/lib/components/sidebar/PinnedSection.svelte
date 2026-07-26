<!--
	PinnedSection.svelte

	The "Pinned" rail at the top of the sidebar — a flat list of URLs the user
	has pinned. Each pin is a thing/page/day/person/project/external URL.
	Click navigates; right-click → unpin. Drag-reorder via the API's
	`PUT /api/pins/reorder`.

	Distinct from project pins — these are user-global ("always visible"),
	not scoped to any project.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import type { Pin } from '$lib/api/client';
	import { pinsStore } from '$lib/stores/pins.svelte';

	interface Props {
		collapsed?: boolean;
	}

	let { collapsed = false }: Props = $props();

	const pins = $derived(pinsStore.pins);

	onMount(() => {
		void pinsStore.load();
	});

	function open(pin: Pin) {
		if (pin.url.startsWith('http://') || pin.url.startsWith('https://')) {
			window.open(pin.url, '_blank', 'noopener,noreferrer');
			return;
		}
		windowShellStore.openTabFromRoute(pin.url);
	}

	function showContextMenu(e: MouseEvent, pin: Pin) {
		e.preventDefault();
		e.stopPropagation();
		contextMenu.show(
			{ x: e.clientX, y: e.clientY },
			[
				{
					id: 'unpin',
					label: 'Unpin',
					icon: 'ri:pushpin-fill',
					action: async () => {
						await pinsStore.remove(pin.id);
					}
				}
			]
		);
	}
</script>

{#if pins.length > 0}
	<div class="pinned-section" class:collapsed>
		{#if !collapsed}
			<div class="section-header">
				<span class="header-label">Pinned</span>
			</div>
		{/if}

		<ul class="pin-list">
			{#each pins as pin (pin.id)}
				<li>
					<button
						type="button"
						class="pin-row"
						class:collapsed
						title={pin.label ?? pin.url}
						onclick={() => open(pin)}
						oncontextmenu={(e) => showContextMenu(e, pin)}
					>
						<Icon icon={pin.icon ?? 'ri:pushpin-line'} width="14" />
						{#if !collapsed}
							<span class="pin-label">{pin.label ?? pin.url}</span>
						{/if}
					</button>
				</li>
			{/each}
		</ul>
	</div>
{/if}

<style>
	.pinned-section {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		padding: 0 0.375rem 0.375rem;
	}
	.pinned-section.collapsed {
		padding: 0 0.25rem 0.25rem;
	}

	.section-header {
		display: flex;
		align-items: center;
		padding: 0.375rem 0.5rem 0.125rem;
		font-size: 0.6875rem;
		font-weight: 600;
		color: var(--color-foreground-subtle, #9ca3af);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.pin-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.pin-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		width: 100%;
		padding: 0.3125rem 0.5rem;
		background: transparent;
		border: none;
		border-radius: 4px;
		cursor: pointer;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground, inherit);
		text-align: left;
	}
	.pin-row.collapsed {
		justify-content: center;
		padding: 0.3125rem 0.375rem;
	}
	.pin-row:hover {
		background: var(--color-background-hover);
	}
	.pin-label {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
