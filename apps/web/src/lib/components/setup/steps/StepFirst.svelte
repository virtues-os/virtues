<!--
	Names, the third beat: what excites you most about Virtues (StepNames
	runs it after the two names, before home).

	THE STEP'S OWN GRAMMAR. A StepFrame title and one line, then five rows
	in the same choice style as the Subscription step: a hairline row, a
	medium name, one muted line and a radio, the navy edge and wash when
	chosen. Each row opens with a small square of dots, one navy mark on
	what that answer is about. The answers and their copy live in
	`$lib/settings/startWith`.

	IT IS OPTIONAL. Nothing is chosen when the beat opens, and "Skip for now"
	moves on and saves nothing. Continue waits for a choice.

	WHAT THE ANSWER DOES. It is saved on the profile and is not in the
	assistant's prompt (`api::profile::StartWith` says why). While the server
	is linked to an account, the account service keeps the same one answer;
	where that goes is written in the manual and
	agents/build/privacy-boundary.md, not here.

	Keys: the arrows move the choice through the rows, as in Subscription.
-->
<script lang="ts">
	import { onMount, tick } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { updateProfile } from "$lib/api/client";
	import { START_WITH, type StartWith } from "$lib/settings/startWith";
	import StepFrame from "../StepFrame.svelte";
	import { setup } from "../setup.svelte";

	let { eyebrow, onnext, onback }: { eyebrow?: string; onnext: () => void; onback?: () => void } = $props();

	let picked = $state<StartWith | null>(null);
	const choice = $derived(START_WITH.find((c) => c.id === picked) ?? null);
	let busy = $state(false);
	let error = $state<string | null>(null);
	let group = $state<HTMLElement | null>(null);

	onMount(async () => {
		// The store may still be reading when the beat mounts.
		if (!setup.loaded) await setup.refresh();
		picked = setup.profile?.start_with ?? null;
	});

	function choose(id: StartWith) {
		if (busy) return;
		picked = id;
		error = null;
		setup.hear();
	}

	async function onKey(e: KeyboardEvent) {
		const step = e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : e.key === "ArrowUp" || e.key === "ArrowLeft" ? -1 : 0;
		if (!step) return;
		e.preventDefault();
		const at = START_WITH.findIndex((c) => c.id === picked);
		const next = START_WITH[(at + step + START_WITH.length) % START_WITH.length];
		choose(next.id);
		await tick();
		group?.querySelector<HTMLElement>(`[data-pick="${next.id}"]`)?.focus();
	}

	async function go() {
		if (!choice || busy) return;
		busy = true;
		error = null;
		try {
			await updateProfile({ start_with: choice.id });
		} catch (e) {
			console.error("[setup] saving what excites them most failed", e);
			error = "Your server couldn't save that. Try again.";
			busy = false;
			return;
		}
		// The save is done. A refresh that fails only leaves the store a
		// moment behind, which the next step's own refresh repairs.
		try {
			await setup.refresh();
		} catch (e) {
			console.error("[setup] after saving what excites them most, the refresh failed", e);
		}
		onnext();
	}
</script>

<StepFrame
	{eyebrow}
	title="What excites you most about Virtues?"
	subtitle="Pick the one closest to why you are here. You get all of it either way."
>
	<div class="rows" role="radiogroup" aria-label="What excites you most" bind:this={group}>
		{#each START_WITH as c, i (c.id)}
			<button
				type="button"
				class="row"
				role="radio"
				data-pick={c.id}
				aria-checked={picked === c.id}
				tabindex={picked === c.id || (!picked && i === 0) ? 0 : -1}
				disabled={busy}
				onclick={() => choose(c.id)}
				onkeydown={onKey}
			>
				<span class="glyph {c.id}" aria-hidden="true">
					{#if c.id === "days"}
						<i style="left:10px;top:13px;width:24px;height:3px" class="line on"></i>
						<i style="left:10px;top:20px;width:20px;height:3px" class="line"></i>
						<i style="left:10px;top:27px;width:23px;height:3px" class="line"></i>
					{:else if c.id === "patterns"}
						<i style="left:9px;top:18px"></i>
						<i style="left:14px;top:15px"></i>
						<i style="left:19px;top:19px"></i>
						<i style="left:24px;top:16px"></i>
						<i style="left:29px;top:18px"></i>
						<i style="left:33px;top:28px" class="on big"></i>
					{:else if c.id === "memory"}
						<i style="left:10px;top:19px;height:6px" class="bar"></i>
						<i style="left:15px;top:15px;height:14px" class="bar"></i>
						<i style="left:20px;top:12px;height:20px" class="bar on"></i>
						<i style="left:25px;top:16px;height:12px" class="bar on"></i>
						<i style="left:30px;top:18px;height:8px" class="bar"></i>
					{:else if c.id === "habit"}
						<i style="left:9px;top:29px"></i>
						<i style="left:15px;top:25px"></i>
						<i style="left:21px;top:21px"></i>
						<i style="left:27px;top:16px"></i>
						<i style="left:32px;top:10px" class="on big"></i>
					{:else}
						<i style="left:8px;top:12px"></i>
						<i style="left:14px;top:10px"></i>
						<i style="left:11px;top:17px"></i>
						<i style="left:17px;top:15px"></i>
						<i style="left:8px;top:22px"></i>
						<i style="left:15px;top:23px"></i>
						<i style="left:31px;top:27px" class="on apart"></i>
					{/if}
				</span>
				<span class="words">
					<span class="name">{c.title}</span>
					<span class="line-text">{c.line}</span>
				</span>
				<span class="radio" aria-hidden="true"></span>
			</button>
		{/each}
	</div>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#snippet actions()}
		<button type="button" class="setup-go" disabled={!choice || busy} onclick={go}>
			Continue
			<Icon icon="mynaui:arrow-right" width="16" />
		</button>
		<button type="button" class="setup-past" disabled={busy} onclick={onnext}>Skip for now</button>
		{#if onback}
			<button type="button" class="setup-past" disabled={busy} onclick={onback}>Back</button>
		{/if}
	{/snippet}
</StepFrame>

<style>
	.rows {
		display: flex;
		flex-direction: column;
		gap: 8px;
		width: 100%;
		text-align: left;
		animation: setup-rise var(--m-slow) var(--m-ease) both;
		animation-delay: 120ms;
	}

	/* The Subscription step's choice, as a row. */
	.row {
		display: flex;
		align-items: center;
		gap: 16px;
		width: 100%;
		padding: 16px 20px 16px 16px;
		box-sizing: border-box;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		background: transparent;
		font: inherit;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		transition:
			border-color var(--m-quick) ease,
			background-color var(--m-quick) ease,
			transform var(--m-base) var(--m-spring);
	}
	.row:hover {
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
	}
	.row:active {
		transform: scale(0.985);
		transition-duration: 90ms;
	}
	.row[aria-checked="true"] {
		border-color: var(--color-primary);
		outline: 3px solid color-mix(in srgb, var(--color-primary) 10%, transparent);
		background: color-mix(in srgb, var(--color-primary) 3%, transparent);
	}

	/* A small square of dots, one navy mark on what the answer is about. */
	.glyph {
		position: relative;
		flex: none;
		width: 44px;
		height: 44px;
		border-radius: var(--radius-lg);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	.glyph i {
		position: absolute;
		width: 4px;
		height: 4px;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
		opacity: 0.6;
	}
	.glyph i.line,
	.glyph i.bar {
		border-radius: var(--radius-xs);
	}
	.glyph i.bar {
		width: 3px;
	}
	.glyph i.big {
		width: 5px;
		height: 5px;
	}
	.glyph i.apart {
		width: 6px;
		height: 6px;
	}
	.glyph i.on {
		background: var(--color-primary);
		opacity: 1;
	}

	.words {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.name {
		font-size: 16px;
		font-weight: 500;
	}
	.line-text {
		font-size: 14px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	/* On a phone the glyph takes a quarter of the row and every line wraps
	   twice, so the rows drop it and keep the words. */
	@media (max-width: 560px) {
		.row {
			gap: 12px;
			padding: 12px 16px;
		}
		.glyph {
			display: none;
		}
		.name {
			font-size: 15px;
		}
		.line-text {
			font-size: 13px;
			line-height: 1.4;
		}
	}

	.radio {
		flex: none;
		width: 18px;
		height: 18px;
		box-sizing: border-box;
		border-radius: 50%;
		border: 1.5px solid var(--color-border);
		transition:
			border-width var(--m-base) var(--m-spring),
			border-color var(--m-quick) ease;
	}
	.row[aria-checked="true"] .radio {
		border: 5px solid var(--color-primary);
	}

	.error {
		margin: 1rem 0 0;
		font-size: 13px;
		color: var(--color-error);
	}
</style>
