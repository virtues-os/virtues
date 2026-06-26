<script lang="ts">
	import { onMount } from 'svelte';

	let name = $state('');
	let savedName = $state('');
	let loading = $state(true);
	let saving = $state(false);

	onMount(load);

	async function load() {
		try {
			const res = await fetch('/api/assistant-profile');
			if (res.ok) {
				const profile = await res.json();
				savedName = profile.assistant_name || 'Ari';
				name = savedName;
			}
		} catch (error) {
			console.error('Failed to load assistant name:', error);
		} finally {
			loading = false;
		}
	}

	async function save() {
		const trimmed = name.trim();
		if (!trimmed || trimmed === savedName || saving) return;

		saving = true;
		const previous = savedName;
		savedName = trimmed; // optimistic

		try {
			const res = await fetch('/api/assistant-profile', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ assistant_name: trimmed })
			});
			if (!res.ok) {
				savedName = previous;
				name = previous;
				console.error('Failed to save assistant name');
			}
		} catch (error) {
			savedName = previous;
			name = previous;
			console.error('Failed to save assistant name:', error);
		} finally {
			saving = false;
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			(e.target as HTMLInputElement)?.blur();
		}
	}
</script>

<div class="bg-surface border border-border rounded-lg">
	<div class="flex items-center justify-between px-4 py-3 border-b border-border">
		<h2 class="text-sm font-medium text-foreground">Name</h2>
	</div>

	<div class="p-4">
		<div class="text-sm font-medium text-foreground mb-2">
			Assistant name
			<span class="font-normal text-foreground-subtle">· what you call your assistant</span>
		</div>
		<input
			type="text"
			bind:value={name}
			onblur={save}
			onkeydown={onKeydown}
			disabled={loading || saving}
			maxlength="100"
			placeholder="Ari"
			class="w-full px-3 py-2 bg-background border border-border rounded-md text-sm text-foreground placeholder:text-foreground-subtle hover:border-border-strong focus:border-border-strong focus:outline-none transition-colors disabled:opacity-60"
		/>
	</div>
</div>
