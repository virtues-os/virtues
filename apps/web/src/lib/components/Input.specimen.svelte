<script lang="ts">
	/** Input in each state, and Textarea. */
	import Input from './Input.svelte';
	import Textarea from './Textarea.svelte';

	let name = $state('David Okafor');
	let note = $state('Ask about the boat on Saturday.');
</script>

<div class="grid">
	<Input label="Name" bind:value={name} helperText="How your assistant addresses you." delight={false} />
	<Input label="Email" value="nick@example" error="Add the part after the @." delight={false} />
	<Input label="City" value="Lisbon" warning="We could not find this on the map." delight={false} />
	<Input label="Server address" value="192.168.1.20" success delight={false} />
	<Input label="Disabled" value="Read only" disabled delight={false} />
	<Textarea label="Note" bind:value={note} />
</div>

<style>
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
		gap: 24px;
	}
</style>
