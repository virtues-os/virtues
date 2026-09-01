<!--
	IntroductionsCard.svelte — the few things the record cannot supply,
	asked on Home instead of in a flow.

	Ported from onboarding's Introductions step when onboarding shrank to the
	letter (2026-08-31, agents/plan/getting-started-plan.md). Same four fields,
	same criterion: the record cannot reliably supply them. What you like to be
	called, what you'll call it, when you were born — everything else the
	profile could hold the box infers from the record itself. The timezone is
	the boundary case: the browser supplies a guess, and a guess made while
	traveling records the wrong home for every daily rhythm after it — so the
	guess is shown prefilled and correcting it costs one tap.

	No field is required, and the whole card can be waved away — a box that
	will not proceed until you have named it has misunderstood which of you is
	in charge. In Home's grammar this is a card: the person answering.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import UniversalPicker from "$lib/components/UniversalPicker.svelte";
	import { getProfile, updateProfile, getAssistantProfile, updateAssistantProfile } from "$lib/api/client";
	import { onMount } from "svelte";

	let {
		ondone,
		ondismiss,
	}: {
		/** Answered — the section retires. */
		ondone: () => void;
		/** Waved away — same retirement, recorded as a dismissal. */
		ondismiss: () => void;
	} = $props();

	let you = $state("");
	let assistant = $state("");
	/** "YYYY-MM-DD" — the date input's native value, and the wire form. */
	let born = $state("");
	/** IANA zone — prefilled from the profile, else this browser's guess. */
	let tz = $state("");
	const ZONES: string[] = (() => {
		try {
			return Intl.supportedValuesOf("timeZone");
		} catch {
			return [];
		}
	})();

	function cityOf(z: string): string {
		const last = z.split("/").pop() ?? z;
		return last.replace(/_/g, " ");
	}
	function regionOf(z: string): string {
		return z.includes("/") ? z.split("/")[0] : "Other";
	}

	let assistantDefault = $state("Ari");
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);

	// NAMES ON A PLATE. Naming something you just met is the hardest ask on
	// the card — a blank field demands invention, a tap does not. Three at a
	// time plus a reroll, so it stays an offer, never a menu to exhaust.
	const NAME_POOL = [
		"Verity", "Honor", "Mercy", "Amity", "Merit", "Ever", "True", "Clem",
		"Juno", "Iris", "Clio", "Echo", "Thea", "Atlas", "Lyra", "Vesper",
		"Eos", "Rhea", "Selene", "Orion", "Nyx", "Vesta", "Freya", "Bran",
		"Milo", "Remy", "Cass", "Piper", "Ada", "Otto", "Arlo", "Theo",
		"Nico", "Nell", "Ivy", "Hazel", "Gus", "Ida", "Fay", "Ruby",
		"Wren", "Sage", "Fern", "Basil", "Sol", "Nova", "Ember", "Rook",
		"Alba", "Orin", "Lumen", "Aster", "Calla", "Sorrel", "Larkin",
		"Marlow", "Onyx", "Quill", "Moss", "Vale",
	];
	let suggested = $state<string[]>([]);

	function reroll() {
		const pool = NAME_POOL.filter((n) => n !== assistant.trim() && !suggested.includes(n));
		const next: string[] = [];
		while (next.length < 3 && pool.length) {
			next.push(pool.splice(Math.floor(Math.random() * pool.length), 1)[0]);
		}
		suggested = next;
	}

	onMount(async () => {
		try {
			const [p, a] = await Promise.all([
				getProfile().catch(() => null),
				getAssistantProfile<{ assistant_name?: string | null }>().catch(() => null),
			]);
			you = p?.preferred_name || "";
			born = p?.birth_date || "";
			const guess = Intl.DateTimeFormat().resolvedOptions().timeZone || "";
			tz = p?.home_timezone && p.home_timezone !== "UTC" ? p.home_timezone : guess;
			if (a?.assistant_name) assistantDefault = a.assistant_name;
			assistant = a?.assistant_name ?? "";
		} catch {
			// A failed read costs a pre-filled field, nothing more.
		}
		reroll();
		loading = false;
	});

	async function save() {
		saving = true;
		error = null;
		try {
			const jobs: Promise<unknown>[] = [];
			const profile: Record<string, string> = {};
			if (you.trim()) profile.preferred_name = you.trim();
			if (born) profile.birth_date = born;
			// Written whenever shown: the person has seen the value, so what the
			// field says IS the answer — including an untouched correct guess.
			if (tz.trim()) profile.home_timezone = tz.trim();
			if (Object.keys(profile).length) jobs.push(updateProfile(profile));
			if (assistant.trim()) jobs.push(updateAssistantProfile({ assistant_name: assistant.trim() }));
			await Promise.all(jobs);
		} catch (e) {
			// Say so, and carry on regardless. Being unable to record a nickname
			// is not a reason to keep asking.
			error = e instanceof Error ? e.message : String(e);
		}
		saving = false;
		ondone();
	}
</script>

<div class="card">
	{#if loading}
		<p class="quiet">One moment…</p>
	{:else}
		<div class="fields">
			<label>
				<span class="q">What should it call you?</span>
				<input type="text" bind:value={you} placeholder="Whatever your friends call you" autocomplete="given-name" />
			</label>

			<div class="group">
				<label>
					<span class="q">And what will you call it?</span>
					<input type="text" bind:value={assistant} placeholder={assistantDefault} />
				</label>
				{#if suggested.length}
					<div class="names">
						<button type="button" class="chip roll" onclick={reroll} aria-label="Deal three more names" title="More names">
							<Icon icon="ri:shuffle-line" width="13" />
						</button>
						{#each suggested as name (name)}
							<button
								type="button"
								class="chip"
								class:chosen={assistant.trim() === name}
								onclick={() => (assistant = name)}
							>
								{name}
							</button>
						{/each}
					</div>
				{/if}
			</div>

			<label>
				<span class="q">When were you born?</span>
				<input type="date" bind:value={born} />
			</label>

			<div class="group tz-field">
				<span class="q">What time zone is home?</span>
				{#if ZONES.length}
					<UniversalPicker
						items={ZONES}
						value={tz}
						getKey={(z) => z}
						getValue={(z) => z}
						onSelect={(z) => (tz = z)}
						width="w-full"
						maxHeight="max-h-48"
						searchable={true}
						getSearchText={(z) => z.replace(/[_/]/g, " ")}
						getGroup={regionOf}
						searchPlaceholder="Search for a city…"
					>
						{#snippet trigger(current, _disabled, open)}
							<div class="tz-trigger">
								<span>{current ? cityOf(current) : "Choose a time zone"}</span>
								<span class="tz-caret" class:open>
									<Icon icon="ri:arrow-down-s-line" width="16" />
								</span>
							</div>
						{/snippet}
						{#snippet item(z, selected)}
							<span class="tz-item" class:selected>{cityOf(z)}</span>
						{/snippet}
					</UniversalPicker>
				{:else}
					<input type="text" bind:value={tz} placeholder="America/Chicago" />
				{/if}
			</div>
		</div>

		{#if error}
			<p class="err"><Icon icon="ri:error-warning-line" width="14" /> {error}</p>
		{/if}

		<div class="row">
			<button class="go" type="button" onclick={save} disabled={saving}>
				{saving ? "Saving…" : "Done"}
			</button>
			<button class="skip" type="button" onclick={ondismiss} disabled={saving}>Not now</button>
		</div>
	{/if}
</div>

<style>
	.card {
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 14px;
		padding: clamp(18px, 3vw, 24px);
	}

	.fields { display: flex; flex-direction: column; gap: 1.35rem; }
	label, .group { display: flex; flex-direction: column; gap: 0.45rem; }

	/* The questions are prose, so they are set as prose — same voice as the
	   keep card's question. */
	.q { font-family: var(--font-serif); font-size: 16px; line-height: 1.4; color: var(--color-foreground); }

	input, .tz-trigger {
		width: 100%;
		border: 1px solid var(--color-border);
		border-radius: 9px;
		background: var(--color-surface, transparent);
		padding: 0.55rem 0.75rem;
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--color-foreground);
	}
	input:focus { outline: none; border-color: color-mix(in srgb, var(--color-primary) 45%, var(--color-border)); }
	input::placeholder { color: var(--color-foreground-subtle); }

	.names { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; }
	.chip {
		padding: 0.24rem 0.8rem;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		font-family: var(--font-serif);
		font-size: 0.9rem;
		color: var(--color-foreground-muted);
		background: none;
		cursor: pointer;
		transition: color 0.15s ease, background 0.15s ease;
	}
	.chip:hover { color: var(--color-foreground); background: color-mix(in srgb, var(--color-foreground) 5%, transparent); }
	.chip.chosen { color: var(--color-foreground); background: color-mix(in srgb, var(--color-foreground) 8%, transparent); }
	.chip.roll { display: inline-flex; align-items: center; justify-content: center; padding: 0.32rem; color: var(--color-foreground-subtle); }
	.chip.roll:hover { color: var(--color-foreground); }

	.tz-trigger { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; cursor: pointer; text-align: left; }
	.tz-caret { display: inline-flex; color: var(--color-foreground-subtle); transition: transform 0.15s ease; }
	.tz-caret.open { transform: rotate(180deg); }
	.tz-item { display: block; padding: 0.5rem 0.65rem; font-size: 0.9rem; }
	.tz-item.selected { font-weight: 500; }
	.tz-field :global(button:focus-visible) { outline: none; }
	.tz-field :global(button:focus-visible .tz-trigger) { border-color: color-mix(in srgb, var(--color-primary) 60%, transparent); }

	.row { display: flex; align-items: center; gap: 16px; margin-top: 1.25rem; }
	.go {
		font-family: var(--font-sans); font-size: 13px; font-weight: 500;
		color: var(--color-primary); background: none; border: 0; padding: 0; cursor: pointer;
	}
	.go:hover:not(:disabled) { text-decoration: underline; text-underline-offset: 3px; }
	.go:disabled { color: var(--color-foreground-disabled); cursor: default; }
	.skip {
		font-family: var(--font-sans); font-size: 12.5px;
		color: var(--color-foreground-subtle); background: none; border: 0; padding: 0; cursor: pointer;
	}
	.skip:hover:not(:disabled) { color: var(--color-foreground); }

	.err { display: flex; align-items: center; gap: 6px; margin-top: 0.9rem; font-size: 12.5px; color: var(--color-error); }
	.quiet { font-size: 14px; color: var(--color-foreground-subtle); margin: 0; }
</style>
