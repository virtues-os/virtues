<script lang="ts">
	/**
	 * What I've learned — the machine's memory, visible.
	 *
	 * Every note here rides in the system prompt of every conversation. That
	 * is exactly why this surface exists: a machine channel about the person
	 * that the person cannot read is the disease the observed-data portrait
	 * was deleted for. Edit a note and it becomes yours — in your words, and
	 * the machine can no longer revise or retire it. Remove one and it is
	 * gone from every future conversation.
	 */
	import { onMount } from 'svelte';
	import {
		listAssistantMemories,
		editAssistantMemory,
		retireAssistantMemory,
		type AssistantMemory,
	} from '$lib/api/client';

	const LANES: { key: string; label: string; hint: string }[] = [
		{ key: 'facts', label: 'Your world', hint: 'durable facts it has picked up' },
		{ key: 'manner', label: 'Your manner', hint: 'how it should speak with you' },
		{ key: 'practices', label: 'Your practices', hint: 'what you are holding to' },
	];

	let memories = $state<AssistantMemory[]>([]);
	let loading = $state(true);
	let editingId = $state<number | null>(null);
	let draft = $state('');

	onMount(load);

	async function load() {
		try {
			memories = await listAssistantMemories();
		} catch (error) {
			console.error('Failed to load memories:', error);
		} finally {
			loading = false;
		}
	}

	function startEdit(m: AssistantMemory) {
		editingId = m.id;
		draft = m.body;
	}

	async function saveEdit(m: AssistantMemory) {
		const body = draft.trim();
		editingId = null;
		if (!body || body === m.body) return;
		try {
			const updated = await editAssistantMemory(m.id, body);
			memories = memories.map((x) => (x.id === m.id ? updated : x));
		} catch (error) {
			console.error('Failed to edit memory:', error);
		}
	}

	async function remove(m: AssistantMemory) {
		const previous = memories;
		memories = memories.filter((x) => x.id !== m.id); // optimistic
		try {
			await retireAssistantMemory(m.id);
		} catch (error) {
			memories = previous;
			console.error('Failed to remove memory:', error);
		}
	}
</script>

<div class="bg-surface border border-border rounded-lg">
	<div class="px-4 py-3 border-b border-border">
		<h2 class="text-sm font-medium text-foreground">What I've learned</h2>
		<p class="text-xs text-foreground-subtle mt-0.5">
			Notes your assistant keeps from living alongside you. Every note is read
			before every conversation — edit one to put it in your words (it becomes
			yours and the assistant can't rewrite it), or remove it for good.
		</p>
	</div>

	<div class="p-4 space-y-4">
		{#if loading}
			<div class="text-sm text-foreground-subtle">Loading…</div>
		{:else if memories.length === 0}
			<div class="text-sm text-foreground-subtle">
				Nothing yet — it writes things down as you work together.
			</div>
		{:else}
			{#each LANES as lane (lane.key)}
				{@const inLane = memories.filter((m) => m.lane === lane.key)}
				{#if inLane.length > 0}
					<div>
						<div class="text-sm font-medium text-foreground mb-1.5">
							{lane.label}
							<span class="font-normal text-foreground-subtle">· {lane.hint}</span>
						</div>
						<ul class="space-y-1">
							{#each inLane as m (m.id)}
								<li class="group flex items-start gap-2 text-sm text-foreground">
									{#if editingId === m.id}
										<!-- svelte-ignore a11y_autofocus -->
										<input
											type="text"
											bind:value={draft}
											onblur={() => saveEdit(m)}
											onkeydown={(e) => {
												if (e.key === 'Enter') (e.target as HTMLInputElement).blur();
												if (e.key === 'Escape') editingId = null;
											}}
											autofocus
											maxlength="500"
											class="flex-1 px-2 py-1 bg-background border border-border-strong rounded text-sm focus:outline-none"
										/>
									{:else}
										<button
											type="button"
											class="flex-1 text-left px-2 py-1 rounded hover:bg-background transition-colors"
											title="Edit — it becomes yours, in your words"
											onclick={() => startEdit(m)}
										>
											{m.body}
											{#if m.author === 'human'}
												<span class="text-xs text-foreground-subtle ml-1">yours</span>
											{/if}
										</button>
										<button
											type="button"
											class="opacity-0 group-hover:opacity-100 text-xs text-foreground-subtle hover:text-foreground px-1 py-1 transition-opacity"
											title="Remove from every future conversation"
											onclick={() => remove(m)}
										>
											Remove
										</button>
									{/if}
								</li>
							{/each}
						</ul>
					</div>
				{/if}
			{/each}
		{/if}
	</div>
</div>
