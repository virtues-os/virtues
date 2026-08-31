<!--
  Introductions — the few things the record cannot supply.

  BETWEEN THE LETTER AND THE WORK. Sync latency is the only irreversible cost in
  onboarding, so the sources want to start as early as possible — but this page
  is thirty seconds, and being asked your own name right after a letter about
  trust is a gentler on-ramp than a Full Disk Access prompt.

  FOUR FIELDS, ONE CRITERION: the record cannot reliably supply them. What you
  like to be called, what you'll call it, when you were born — everything
  else the profile could hold (occupation, employer, home city) the box infers
  from the record itself, and asking for it here would make the screen after
  the privacy letter a form. The timezone is the boundary case: the browser
  supplies a GUESS (captured silently on mount by the route), and a guess made
  while traveling records the wrong home for every daily rhythm after it — so
  the guess is shown prefilled, and correcting it costs one tap.

  No field is required. A box that will not proceed until you have named it has
  misunderstood which of you is in charge.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import UniversalPicker from "$lib/components/UniversalPicker.svelte";
	import { getProfile, updateProfile, getAssistantProfile, updateAssistantProfile } from "$lib/api/client";
	import { onMount } from "svelte";

	// Shell and progress strip belong to the route; this is only the leaf.
	let { onnext }: { onnext: () => void } = $props();

	let you = $state("");
	let assistant = $state("");
	/** "YYYY-MM-DD" — the date input's native value, and the wire form. */
	let born = $state("");
	/** IANA zone — prefilled from the profile, else this browser's guess. */
	let tz = $state("");
	/** Every zone the browser can name; empty on engines without the API,
	 *  where the field degrades to a text input. */
	const ZONES: string[] = (() => {
		try {
			return Intl.supportedValuesOf("timeZone");
		} catch {
			return [];
		}
	})();

	/** "America/New_York" → "New York" — the city is the answer; the region
	 *  renders once as a group header, so repeating it per row is noise. */
	function cityOf(z: string): string {
		const last = z.split("/").pop() ?? z;
		return last.replace(/_/g, " ");
	}
	function regionOf(z: string): string {
		return z.includes("/") ? z.split("/")[0] : "Other";
	}
	/** What it is already called, so the field can suggest rather than demand. */
	let assistantDefault = $state("Ari");
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);

	// NAMES ON A PLATE. Naming something you met ninety seconds ago is the
	// hardest ask on the page — a blank field demands invention, a tap does
	// not. Three at a time plus a reroll, so it stays an offer, never a menu
	// to exhaust. Every name is short and pronounceable on sight; none is a
	// product's, an assistant's, or a figure with baggage. Four registers,
	// mixed so a deal usually crosses them:
	const NAME_POOL = [
		// virtue words — the house register
		"Verity", "Honor", "Mercy", "Amity", "Merit", "Ever", "True", "Clem",
		// mythological — old enough to be nobody's trademark
		"Juno", "Iris", "Clio", "Echo", "Thea", "Atlas", "Lyra", "Vesper",
		"Eos", "Rhea", "Selene", "Orion", "Nyx", "Vesta", "Freya", "Bran",
		// plain human — for whoever finds the poetic ones twee
		"Milo", "Remy", "Cass", "Piper", "Ada", "Otto", "Arlo", "Theo",
		"Nico", "Nell", "Ivy", "Hazel", "Gus", "Ida", "Fay", "Ruby",
		// word-names and near-inventions
		"Wren", "Sage", "Fern", "Basil", "Sol", "Nova", "Ember", "Rook",
		"Alba", "Orin", "Lumen", "Aster", "Calla", "Sorrel", "Larkin",
		"Marlow", "Onyx", "Quill", "Moss", "Vale",
	];
	let suggested = $state<string[]>([]);

	/** Deal three fresh names — never the current value, never a repeat of the hand showing. */
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
			// `preferred_name` is the only name the profile API exposes — the
			// table also holds full_name, but it is not on the wire, and reaching
			// past the API for it would be borrowing trouble for a placeholder.
			you = p?.preferred_name || "";
			born = p?.birth_date || "";
			// The profile's zone when it's a real one; the browser's guess when
			// the box has nothing or the datacenter default (same rule as the
			// route's silent capture — see captureTimezone in +page.svelte).
			const guess = Intl.DateTimeFormat().resolvedOptions().timeZone || "";
			tz = p?.home_timezone && p.home_timezone !== "UTC" ? p.home_timezone : guess;
			if (a?.assistant_name) assistantDefault = a.assistant_name;
			assistant = a?.assistant_name ?? "";
		} catch {
			// A failed read costs a pre-filled field, nothing more.
		}
		// After the reads, so the deal never offers the name it already has.
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
			// is not a reason to hold someone at a door.
			error = e instanceof Error ? e.message : String(e);
		}
		saving = false;
		onnext();
	}
</script>

<div>
	<div>
		<h1 class="ob-h1">Introductions</h1>
		<p class="ob-lede">The few things it cannot learn by reading.</p>

		{#if loading}
			<p class="quiet">One moment…</p>
		{:else}
			<div class="fields">
				<label>
					<span class="label">What should it call you?</span>
					<input
						class="ob-input"
						type="text"
						bind:value={you}
						placeholder="Whatever your friends call you"
						autocomplete="given-name"
					/>
				</label>

				<div class="field-group">
					<label>
						<span class="label">And what will you call it?</span>
						<input class="ob-input" type="text" bind:value={assistant} placeholder={assistantDefault} />
					</label>
					{#if suggested.length}
						<!-- The reroll leads the row: it is the button clicked repeatedly,
						     and leftmost is the one position the varying pill widths can
						     never shift out from under the cursor. -->
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
					<span class="label">When were you born?</span>
					<input class="ob-input" type="date" bind:value={born} />
				</label>

				<div class="field-group tz-field">
					<span class="label">What time zone is home?</span>
					{#if ZONES.length}
						<UniversalPicker
							items={ZONES}
							value={tz}
							getKey={(z) => z}
							getValue={(z) => z}
							onSelect={(z) => (tz = z)}
							width="w-full"
							maxHeight="max-h-64"
							searchable={true}
							getSearchText={(z) => z.replace(/[_/]/g, " ")}
							getGroup={regionOf}
							searchPlaceholder="Search for a city…"
						>
							{#snippet trigger(current, _disabled, open)}
								<div class="ob-input tz-trigger">
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
						<input class="ob-input" type="text" bind:value={tz} placeholder="America/Chicago" />
					{/if}
				</div>
			</div>

			{#if error}
				<p class="ob-err"><Icon icon="ri:error-warning-line" width="14" /> {error}</p>
			{/if}

			<button class="ob-btn" onclick={save} disabled={saving}>
				{saving ? "Saving…" : you.trim() || assistant.trim() || born ? "Continue" : "Skip for now"}
				<Icon icon="ri:arrow-right-line" width="15" />
			</button>
		{/if}
	</div>
</div>

<style>
	/* Only what belongs to this screen. The shell, type scale, button, input and
	   error style all come from onboarding.css. */
	.fields {
		margin-top: 2.25rem;
		display: flex;
		flex-direction: column;
		gap: 1.75rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.field-group {
		display: flex;
		flex-direction: column;
		gap: 0.65rem;
	}

	/* The dealt names: quiet serif pills, an offer under the field rather than
	   a control panel beside it. The shuffle sits in the row as one of them —
	   rerolling is the same gesture as choosing. */
	.names {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
	}

	.chip {
		padding: 0.28rem 0.85rem;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.95rem;
		color: var(--color-foreground-muted);
		transition:
			color 0.15s ease,
			background 0.15s ease;
	}

	.chip:hover {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.chip.chosen {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	/* The reroll is an icon, so it gets to be round. */
	.chip.roll {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0.34rem;
		color: var(--color-foreground-subtle);
	}

	.chip.roll:hover {
		color: var(--color-foreground);
	}

	/* The questions themselves are prose, so they are set as prose. */
	.label {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.15rem;
		color: var(--color-foreground);
	}

	.quiet {
		font-size: 14px;
		color: var(--color-foreground-subtle);
	}

	/* The timezone trigger wears the same coat as the inputs above it; the
	   caret is the only hint it opens instead of types. */
	.tz-trigger {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		cursor: pointer;
		text-align: left;
	}
	.tz-caret {
		display: inline-flex;
		color: var(--color-foreground-subtle);
		transition: transform 0.15s ease;
	}
	.tz-caret.open {
		transform: rotate(180deg);
	}
	/* The picker delegates row padding to this snippet (see ModelSettings for
	   the same convention) — without it the list sets solid. 0.5rem left plus
	   the picker's own px-1 lands rows on the header's 12px indent. */
	.tz-item {
		display: block;
		padding: 0.5rem 0.65rem;
		font-size: 0.9rem;
	}
	.tz-item.selected {
		font-weight: 500;
	}

	/* The wrapping button's default focus ring is the loudest blue on the
	   page; focus speaks the same way the text inputs do instead. */
	.tz-field :global(button:focus-visible) {
		outline: none;
	}
	.tz-field :global(button:focus-visible .tz-trigger) {
		border-color: color-mix(in srgb, var(--color-primary) 60%, transparent);
	}
</style>
