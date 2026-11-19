<script lang="ts">
	/**
	 * Location, on Profile: the city you're usually in, which sets the time
	 * zone your days are counted in. The same city list setup's location step
	 * uses (`setup/cities.ts`), each city carrying its real zone.
	 *
	 * Typing lists the cities that match; choosing one saves the city and its
	 * zone together. A name the list doesn't have is kept as you typed it, on
	 * the zone you already had, as setup does. Only the name and the zone are
	 * stored, never coordinates.
	 */
	import { CITIES, cityForZone, zoneLeaf, zoneName, type City } from '$lib/components/setup/cities';

	let {
		city,
		zone,
		disabled = false,
		onsave,
	}: {
		/** The saved city, if any. */
		city: string;
		/** The saved IANA zone, if any. */
		zone: string;
		disabled?: boolean;
		onsave: (change: { home_city: string; home_timezone?: string }) => Promise<void>;
	} = $props();

	/** What the field shows: the saved city, else the city the zone names. */
	const shownCity = $derived(city || cityForZone(zone)?.name || (zone.includes('/') ? zoneLeaf(zone) : ''));

	let text = $state('');
	let open = $state(false);
	let active = $state(-1);
	let error = $state<string | null>(null);
	$effect(() => {
		text = shownCity;
	});

	const matches = $derived.by(() => {
		const q = text.trim().toLowerCase();
		if (!q || q === shownCity.toLowerCase()) return [];
		const starts = CITIES.filter((c) => c.name.toLowerCase().startsWith(q));
		const inside = CITIES.filter((c) => !starts.includes(c) && `${c.name} ${c.country}`.toLowerCase().includes(q));
		return [...starts, ...inside].slice(0, 6);
	});

	async function save(change: { home_city: string; home_timezone?: string }) {
		error = null;
		try {
			await onsave(change);
		} catch {
			error = "Your server couldn't save that. It kept the one before.";
			text = shownCity;
		}
	}

	function choose(c: City) {
		open = false;
		active = -1;
		text = c.name;
		void save({ home_city: c.name, home_timezone: c.zone });
	}

	function onblur() {
		// A choice from the list has already saved; a name typed and left is kept as typed.
		setTimeout(() => {
			open = false;
			const typed = text.trim();
			if (typed && typed !== shownCity) void save({ home_city: typed });
			else if (!typed) text = shownCity;
		}, 120);
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' && matches.length) {
			open = true;
			active = Math.min(matches.length - 1, active + 1);
			e.preventDefault();
		} else if (e.key === 'ArrowUp' && matches.length) {
			active = Math.max(0, active - 1);
			e.preventDefault();
		} else if (e.key === 'Enter') {
			e.preventDefault();
			if (active >= 0 && matches[active]) choose(matches[active]);
			else (e.currentTarget as HTMLInputElement).blur();
		} else if (e.key === 'Escape') {
			open = false;
			text = shownCity;
		}
	}
</script>

<div class="location">
	<div class="field" class:disabled>
		<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><circle cx="11" cy="11" r="7" fill="none" stroke="currentColor" stroke-width="1.875" /><path d="m20 20l-4-4" stroke="currentColor" stroke-width="1.875" stroke-linecap="round" /></svg>
		<input
			id="profile-home"
			type="text"
			role="combobox"
			aria-expanded={open && matches.length > 0}
			aria-controls="location-matches"
			aria-autocomplete="list"
			aria-activedescendant={active >= 0 ? `location-match-${active}` : undefined}
			autocomplete="off"
			placeholder="Your city"
			bind:value={text}
			{disabled}
			oninput={() => {
				open = true;
				active = -1;
			}}
			{onblur}
			{onkeydown}
		/>
	</div>
	{#if open && matches.length}
		<ul id="location-matches" class="matches" role="listbox" aria-label="Cities">
			{#each matches as c, i (c.name + c.country)}
				<li
					id="location-match-{i}"
					role="option"
					aria-selected={i === active}
					class:active={i === active}
					onmousedown={(e) => {
						e.preventDefault();
						choose(c);
					}}
				>
					<span>{c.name}</span>
					<span class="sub">{c.country} · {zoneName(c.zone)}</span>
				</li>
			{/each}
		</ul>
	{/if}
	{#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
	.location {
		position: relative;
		width: 100%;
	}

	.field {
		display: flex;
		align-items: center;
		gap: 8px;
		height: var(--control-h);
		box-sizing: border-box;
		padding: 0 12px;
		border: 1px solid var(--color-border-field);
		border-radius: var(--radius-lg);
		background: var(--color-surface);
		color: var(--color-foreground-muted);
		transition: border-color 0.25s ease;
	}

	.field:hover:not(.disabled):not(:focus-within) {
		border-color: color-mix(in srgb, var(--color-border-field) 70%, var(--color-foreground));
	}

	.field:focus-within {
		border-color: var(--color-primary);
	}

	@media (pointer: coarse) {
		.field {
			height: var(--control-touch-h);
		}

		input {
			font-size: 16px;
		}
	}

	.field.disabled {
		opacity: 0.6;
	}

	input {
		flex: 1;
		min-width: 0;
		border: none;
		outline: none;
		background: none;
		font-family: var(--font-sans);
		font-size: var(--text-label);
		color: var(--color-foreground);
	}

	.matches {
		position: absolute;
		z-index: 5;
		top: calc(100% + 4px);
		left: 0;
		right: 0;
		margin: 0;
		padding: 4px;
		list-style: none;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface-overlay, var(--color-surface));
		box-shadow: var(--shadow-float);
	}

	li {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 12px;
		border-radius: var(--radius-sm);
		font-size: 14px;
		color: var(--color-foreground);
		cursor: pointer;
	}

	li.active,
	li:hover {
		background: var(--color-surface-elevated);
	}

	.sub {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.error {
		margin: 6px 0 0;
		font-size: 12px;
		color: var(--color-error);
	}
</style>
