<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page, Textarea } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { slide } from "svelte/transition";
	import { onMount } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let loading = $state(true);
	let content = $state("");
	let updatedAt = $state<string | null>(null);
	let charCount = $derived(content.length);
	let showExamples = $state(false);

	const MIN_CHARS = 100;
	const MAX_CHARS = 800;
	let tooShort = $derived(charCount > 0 && charCount < MIN_CHARS);
	let tooLong = $derived(charCount > MAX_CHARS);

	const examples = [
		{
			label: "A builder learning patience",
			content: `I'm a software engineer and father of two young kids. I believe in craft, privacy, and building things that respect people's attention. My faith is important to me but I hold it privately. I'm a workaholic — I know this about myself and I'm actively trying to close the laptop by 6pm and be present with my family. I tend toward urgency and I'm learning that the important things usually aren't urgent. I want to build something meaningful but I'm in a season of slowing down on purpose, not speeding up. I run to think. I read theology and philosophy before bed. I'm more interested in wisdom than productivity right now but the pull toward output is still strong every day.`,
		},
		{
			label: "A nurse reclaiming herself",
			content: `Twelve years in emergency medicine. I care deeply about people but I've neglected myself — I drink more than I should, I isolate when I'm stressed, and I've gained weight I'm not happy about. I don't want to be reminded about any of that. I'm starting a public health masters because I think I can help more people at the systems level than one patient at a time. I'm Catholic, it matters to me, and I pray most mornings even when I don't feel like it. I'm introverted but people assume I'm not because I'm good in a crisis. I want a life that has room for slowness. I want to cook real meals and call my sister more. I'm trying to become someone who asks for help instead of just being the person everyone else calls.`,
		},
		{
			label: "A student between worlds",
			content: `Second year of law school. My parents immigrated from Guatemala and I'm the first in my family to go to graduate school. I carry that weight proudly but it makes it hard to admit when I'm struggling. I care about housing justice and I want to practice law that actually helps communities like the one I grew up in. I'm not religious but I'm spiritual in a way I can't fully articulate — I believe in something bigger. My vice is perfectionism. I overwork, I over-prepare, I don't rest until I crash. I want to become someone who can do important work without destroying herself in the process. I journal most mornings. I'm learning to sit with not knowing.`,
		},
	];

	onMount(async () => {
		await load();
	});

	async function load() {
		loading = true;
		try {
			const res = await fetch("/api/wiki/narrative-identity");
			if (res.ok) {
				const data = await res.json();
				content = data.content || "";
				updatedAt = data.updated_at;
			}
		} catch (err) {
			console.error("Failed to load narrative identity:", err);
		} finally {
			loading = false;
		}
	}

	async function save(value: string) {
		const res = await fetch("/api/wiki/narrative-identity", {
			method: "PUT",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ content: value }),
		});
		if (res.ok) {
			const data = await res.json();
			updatedAt = data.updated_at;
		} else {
			throw new Error("Failed to save");
		}
	}

	function warningMessage(): string | false {
		if (tooLong) return `${charCount - MAX_CHARS} over the ${MAX_CHARS} character limit`;
		if (tooShort) return `${MIN_CHARS - charCount} more characters needed`;
		return false;
	}
</script>

<Page>
	<div class="mx-auto max-w-2xl py-12">
		<h1 class="text-3xl font-serif font-medium text-foreground mb-3">
			Narrative Identity
		</h1>
		<p class="text-foreground-muted leading-relaxed" style="font-size: 16px; margin-top: 4px; margin-bottom: 40px;">
			Who you are right now — what you believe, what you're working on
			in yourself, what direction you're facing. Your assistant reads
			this before every conversation. It shapes understanding silently —
			never repeated back, never used to lecture you.
		</p>

		{#if loading}
			<div class="flex items-center justify-center text-foreground-muted" style="padding: 64px 0;">
				Loading...
			</div>
		{:else}
			<Textarea
				bind:value={content}
				placeholder="What do you believe? What are you working on in yourself?"
				rows={8}
				autoResize
				maxRows={16}
				autoSave
				onSave={save}
				warning={warningMessage()}
				delight
			/>

			<div class="mt-3 flex items-center justify-between text-xs text-foreground-subtle">
				<span class:text-warning={tooLong || tooShort}>
					{charCount} / {MAX_CHARS}
				</span>
				{#if updatedAt}
					<span>
						Last updated {new Date(updatedAt).toLocaleDateString(undefined, {
							month: "short",
							day: "numeric",
							year: "numeric",
						})}
					</span>
				{/if}
			</div>

			<!-- Examples -->
			<div style="margin-top: 48px;">
				<button
					class="flex items-center gap-2 text-sm text-foreground-subtle hover:text-foreground-muted hover:bg-surface-elevated cursor-pointer rounded-md px-2 py-1.5 -ml-2 transition-colors"
					onclick={() => showExamples = !showExamples}
				>
					<Icon
						icon={showExamples ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"}
						width="16"
						height="16"
					/>
					<span>See examples</span>
				</button>

				{#if showExamples}
					<div transition:slide={{ duration: 200 }} class="mt-4 flex flex-col gap-5">
						{#each examples as example}
							<div>
								<p class="text-sm font-medium text-foreground-muted mb-1.5">{example.label}</p>
								<p class="text-sm text-foreground-subtle leading-relaxed italic">"{example.content}"</p>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		{/if}
	</div>
</Page>
