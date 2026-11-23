<!--
	MenuSheet.svelte

	The context menu on the phone: the same items the store holds, shown in a
	bottom sheet instead of a floating menu. Rows are full width and 52px,
	grouped into cards where the menu draws its dividers, and a destructive
	row stands in a card of its own. A row with a submenu opens it in place,
	with a way back at the top. A light haptic tap marks each pick.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import BottomSheet from "$lib/components/mobile/BottomSheet.svelte";
	import { contextMenu, type ContextMenuItem } from "$lib/stores/contextMenu.svelte";
	import { tapHaptic } from "$lib/utils/haptics";

	// The submenus opened, deepest last.
	let trail = $state<ContextMenuItem[]>([]);
	const current = $derived(trail.length ? (trail[trail.length - 1].submenu ?? []) : contextMenu.items);

	/** Cards: a divider starts a new one, and a destructive row gets its own. */
	const groups = $derived.by(() => {
		const out: ContextMenuItem[][] = [];
		let group: ContextMenuItem[] = [];
		const close = () => {
			if (group.length) out.push(group);
			group = [];
		};
		for (const item of current) {
			const destructive = item.variant === "destructive";
			if (item.dividerBefore || destructive) close();
			group.push(item);
			if (item.dividerAfter || destructive) close();
		}
		close();
		return out;
	});

	function pick(item: ContextMenuItem) {
		if (item.disabled) return;
		tapHaptic();
		if (item.submenu) {
			trail = [...trail, item];
			return;
		}
		void contextMenu.executeAction(item);
	}
</script>

<BottomSheet label={trail.length ? trail[trail.length - 1].label : "Menu"} onClose={() => contextMenu.hide()}>
	{#if trail.length}
		<button class="back" onclick={() => (trail = trail.slice(0, -1))}>
			<Icon icon="mynaui:chevron-left" width={18} />
			<span>{trail[trail.length - 1].label}</span>
		</button>
	{/if}
	<div class="groups" role="menu">
		{#each groups as group, g (g)}
			<div class="card">
				{#each group as item (item.id)}
					<button
						class="row"
						class:destructive={item.variant === "destructive"}
						role={item.role ?? "menuitem"}
						aria-checked={item.role ? !!item.checked : undefined}
						disabled={item.disabled}
						onclick={() => pick(item)}
					>
						{#if item.icon}
							<span class="icon" style:color={item.iconColor}><Icon icon={item.icon} width={20} /></span>
						{/if}
						<span class="text">
							<span class="label">{item.label}</span>
							{#if item.description}<span class="description">{item.description}</span>{/if}
						</span>
						{#if contextMenu.loadingItemId === item.id}
							<Icon icon="mynaui:spinner" width={16} class="animate-spin" />
						{:else if item.submenu}
							<Icon icon="mynaui:chevron-right" width={16} />
						{:else if item.checked}
							<span class="check"><Icon icon="mynaui:check" width={18} /></span>
						{/if}
					</button>
				{/each}
			</div>
		{/each}
	</div>
</BottomSheet>

<style>
	.back {
		display: flex;
		align-items: center;
		gap: 4px;
		min-height: 44px;
		padding: 0 4px;
		border: 0;
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		font-size: 1rem;
		cursor: pointer;
	}

	.groups {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.card {
		display: flex;
		flex-direction: column;
		overflow: hidden;
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
	}

	.row {
		display: flex;
		align-items: center;
		gap: 16px;
		min-height: 52px;
		padding: 8px 16px;
		border: 0;
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		font-size: 1.0625rem;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: background-color 0.15s ease-out;
	}
	.row + .row {
		border-top: 1px solid var(--color-border);
	}
	.row:active {
		background: color-mix(in srgb, var(--wash-ink) 6%, transparent);
	}
	.row:disabled {
		opacity: 0.45;
	}
	.row.destructive {
		color: var(--color-error);
	}

	.icon {
		flex: none;
		display: flex;
	}
	.text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.description {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.check {
		display: flex;
		color: var(--color-primary);
	}
</style>
