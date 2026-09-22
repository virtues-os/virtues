<!--
	PlaceAsk.svelte — the one question the record asks back.

	The box names a place it cannot identify after its coordinates, and then
	files every later visit under that name. Nothing in the product ever asks
	you to fix it, so an archive that should get better with age instead
	accumulates rows filed under "Location 41.8781, -87.6298" — and one answer
	here renames every one of them at once, backwards through the record.

	One question at a time, and only for places visited often enough that the
	answer is worth having. Places seen once or twice are noise; asking about
	them teaches you to ignore the question.
-->
<script lang="ts">
	import TextAction from "$lib/components/TextAction.svelte";
	import { getUnnamedPlaces, updatePlace, type UnnamedPlace } from "$lib/wiki/api";

	/** Below this the answer is not worth the interruption. */
	const MIN_VISITS = 3;

	let queue = $state<UnnamedPlace[]>([]);
	let idx = $state(0);
	let name = $state("");
	let saving = $state(false);
	let error = $state<string | null>(null);

	const current = $derived(queue[idx] ?? null);
	const int = new Intl.NumberFormat();

	$effect(() => {
		let dropped = false;
		getUnnamedPlaces(6)
			.then((rows) => {
				if (dropped) return;
				queue = rows.filter((p) => p.ref_count >= MIN_VISITS);
			})
			.catch(() => {});
		return () => {
			dropped = true;
		};
	});

	function coords(p: UnnamedPlace): string {
		if (p.latitude == null || p.longitude == null) return p.name;
		return `${p.latitude.toFixed(4)}, ${p.longitude.toFixed(4)}`;
	}

	function skip() {
		name = "";
		error = null;
		idx += 1;
	}

	async function save() {
		const label = name.trim();
		const p = current;
		if (!label || !p || saving) return;
		saving = true;
		error = null;
		try {
			const ok = await updatePlace(p.id, { name: label });
			if (!ok) throw new Error("rejected");
			skip();
		} catch {
			error = "That didn't save. Your server may be offline. Try again.";
		} finally {
			saving = false;
		}
	}
</script>

{#if current}
	<section class="ask">
		<h2 class="q">
			You've stopped here {int.format(current.ref_count)} times. What is this place?
		</h2>
		<p class="where mono">{coords(current)}</p>
		<div class="row">
			<!-- svelte-ignore a11y_autofocus -->
			<input
				type="text"
				bind:value={name}
				disabled={saving}
				placeholder="Name it"
				aria-label="Name for this place"
				onkeydown={(e) => {
					if (e.key === "Enter") {
						e.preventDefault();
						save();
					}
				}}
			/>
			<!-- `shrink-0` is a Tailwind utility, not a scoped class: the primitive
			     sets no flex-shrink of its own, so nothing here is silently eaten. -->
			<TextAction
				class="shrink-0"
				loading={saving}
				loadingLabel="Saving…"
				disabled={!name.trim()}
				onclick={save}
			>
				Save
			</TextAction>
			<TextAction quiet class="shrink-0" disabled={saving} onclick={skip}>Not this one</TextAction>
		</div>
		{#if error}<p class="err">{error}</p>{/if}
	</section>
{/if}

<style>
	.mono { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }

	/* A hairline, not a card. Throughout this page a rule is the box talking
	   and a card is you answering; the ask belongs to the box. */
	.ask {
		/* Enough to read as its own turn, less than the keep's below it: the
		   question and the answer belong nearer each other than either does to
		   the list above. */
		margin-top: 48px;
		max-width: 40em;
	}
	.q {
		font-family: var(--font-serif); font-size: 18px; font-weight: 400; line-height: 1.45;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground); margin: 0;
	}
	.where { font-size: 13px; color: var(--color-foreground-subtle); margin: 4px 0 0; }

	.row { display: flex; align-items: center; gap: 14px; margin-top: 14px; flex-wrap: wrap; }
	.row input {
		flex: 1 1 220px; min-width: 0;
		background: none; border: 0; border-bottom: 1px solid var(--color-border);
		padding: 5px 0;
		font-family: var(--font-sans); font-size: 15px; color: var(--color-foreground);
	}
	.row input::placeholder { color: var(--color-foreground-subtle); }
	.row input:focus { outline: none; border-bottom-color: var(--color-primary); }

	.err { font-family: var(--font-sans); font-size: 13px; color: var(--color-error); margin: 12px 0 0; }
</style>
