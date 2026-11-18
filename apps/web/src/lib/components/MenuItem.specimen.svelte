<script lang="ts">
	/** MenuItem rows on a floating menu surface, in the states a menu uses. */
	import MenuItem from './MenuItem.svelte';

	let checked = $state(true);
</script>

<div class="menu" role="menu" aria-label="Page actions">
	<MenuItem icon="mynaui:pencil" label="Rename" shortcut="↵" />
	<MenuItem icon="mynaui:copy" label="Duplicate" shortcut="⌘D" />
	<MenuItem icon="mynaui:pin" label="Pin to sidebar" role="menuitemcheckbox" {checked} onclick={() => (checked = !checked)} />
	<MenuItem icon="mynaui:archive" label="Move to thread" submenu />
	<MenuItem label="Export" description="A Markdown file with the page's text" />
	<MenuItem icon="mynaui:inbox" label="Send to inbox" disabled />
	<MenuItem icon="mynaui:trash" label="Delete" destructive />
</div>

<style>
	.menu {
		width: 260px;
		padding: 4px;
		border-radius: var(--radius-md);
		background: var(--page);
		box-shadow: var(--shadow-float);
	}
</style>
