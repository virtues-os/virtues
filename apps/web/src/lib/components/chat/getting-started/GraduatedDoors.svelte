<!--
	Doors under the room's last line.

	`graduated_line` (getting_started.rs) already names the right things — the
	story written down, the record being written from Google and this Mac, the
	first page on Home — and it names them per branch, so the sentence is
	already true for this particular box. What it could not do was let anyone
	TOUCH them. Adam, having finished the walk: "the end of the chat was
	uneventful and i didnt know where to go next as a user." The server line
	answered the first half of that; this answers the second.

	NOT a trophy. The room's prompt says "nothing ceremonial" and the airlock's
	ceremony was reverted for the same reason: this product's register is
	understatement, and confetti here would be its one false note. So the close
	is not a badge saying you finished — it is the things you now have, with a
	way in to each. The doors repeat no copy: the line above says what exists,
	these say where it is.

	They mirror the branches of `graduated_line` exactly, so the prose and the
	doors can never disagree:

	  first_day        → the page itself, the promise's payoff
	  no sources       → Settings, because the honest next act is connecting one
	  interview done   → the document, in their own words
	  always           → Home, which is where the record is read

	Home is last and quiet on purpose. It is the default destination, and a
	primary-weight button on it would read as "leave", when the sentence above
	has just said this room stays.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import Icon from "$lib/components/Icon.svelte";

	const state = $derived(gettingStarted.state);
	const firstDay = $derived(state?.first_day ?? null);
	const world = $derived(gettingStarted.step("connect_world"));
	/** Anything actually feeding the record. The settled line names these, so
	 *  an empty list is the branch that says "connect something in Settings". */
	const flowing = $derived((world?.sources?.length ?? 0) > 0);
	const storyDone = $derived(gettingStarted.step("interview")?.status === "done");

	type Door = { label: string; note: string; go: () => void };

	const doors = $derived.by(() => {
		const out: Door[] = [];
		if (firstDay) {
			out.push({
				label: "Your first page",
				note: new Date(`${firstDay}T00:00:00`).toLocaleDateString("en-US", {
					weekday: "long",
					month: "long",
					day: "numeric",
				}),
				go: () => void goto(`/day/day_${firstDay}`),
			});
		}
		if (storyDone) {
			out.push({
				label: "In your own words",
				note: "Your account, in the first person.",
				go: () => windowShellStore.openRouteBeside("/wiki/identity", "In your own words"),
			});
		}
		if (!flowing) {
			out.push({
				label: "Connect something",
				note: "The record is only as full as what feeds it.",
				go: () => windowShellStore.navigate("/sources", { label: "Sources" }),
			});
		}
		out.push({
			label: "Home",
			note: firstDay ? "Every morning, a page." : "Where the record is read.",
			go: () => void goto("/home"),
		});
		return out;
	});
</script>

<nav class="doors" aria-label="Where to go from here">
	{#each doors as door (door.label)}
		<button type="button" class="door" onclick={door.go}>
			<span class="line">
				<span class="label">{door.label}</span>
				<Icon icon="ri:arrow-right-up-line" width="14" />
			</span>
			<span class="note">{door.note}</span>
		</button>
	{/each}
</nav>

<style>
	/* A row that wraps, not a grid: the branches mean this is two doors on one
	   box and four on another, and a fixed grid leaves a hole at three. */
	.doors {
		display: flex;
		flex-wrap: wrap;
		gap: 0.5rem;
		margin: 1rem 0 0;
	}

	.door {
		flex: 1 1 13rem;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		padding: 0.625rem 0.75rem;
		text-align: left;
		background: none;
		border: 1px solid var(--color-border-subtle);
		border-radius: 8px;
		cursor: pointer;
		transition:
			border-color 150ms var(--ease-premium, ease),
			background-color 150ms var(--ease-premium, ease);
	}

	.door:hover {
		border-color: var(--color-border);
		background: color-mix(in srgb, var(--wash-ink) 3%, transparent);
	}

	.door:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.line {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.5rem;
	}

	/* The serif names the thing, the way the closed card's two doors do — the
	   two cards sit in one thread and must read as one family. */
	.label {
		font-family: var(--font-serif-ui);
		font-size: 1rem;
		color: var(--color-foreground);
	}

	.line :global(svg) {
		flex: none;
		color: var(--color-foreground-subtle);
	}

	.note {
		font-size: 0.8125rem;
		line-height: 1.4;
		color: var(--color-foreground-muted);
	}
</style>
