<!--
	What `record_introductions` wrote, shown under the model's turn as a
	receipt. NO BUTTON: the room has one composer and one thing to do in it,
	and a "That's right" button beside a text box makes the person stop and
	work out which of the two they are meant to use. The tool writes; this
	shows what it wrote; a correction is a reply, which calls the tool again.
-->
<script lang="ts">
	interface Fields {
		full_name?: string | null;
		preferred_name?: string | null;
		assistant_name?: string | null;
		home_place?: string | null;
		home_timezone?: string | null;
		birth_date?: string | null;
	}
	let { fields }: { fields: Fields } = $props();

	/** A date the way a person writes one: 6 June 1996, never 1996-06-06. */
	function readableDate(iso: string): string {
		const [y, m, d] = iso.split("-").map(Number);
		if (!y || !m || !d) return iso;
		return new Date(Date.UTC(y, m - 1, d)).toLocaleDateString("en-US", {
			year: "numeric",
			month: "long",
			day: "numeric",
			timeZone: "UTC",
		});
	}

	// The same five, in the order the room asked for them.
	const rows = $derived(
		(
			[
				["Name", fields.full_name],
				["Goes by", fields.preferred_name],
				["Assistant", fields.assistant_name],
				["Home", fields.home_place ?? fields.home_timezone],
				["Born", fields.birth_date ? readableDate(fields.birth_date) : null],
			] as [string, string | null | undefined][]
		).filter((r): r is [string, string] => !!r[1]),
	);
</script>

<dl class="recorded">
	{#each rows as [label, value] (label)}
		<dt>{label}</dt>
		<dd>{value}</dd>
	{/each}
</dl>

<style>
	.recorded {
		display: grid;
		grid-template-columns: max-content 1fr;
		gap: 0 1.25rem;
		margin: 0.75rem 0 0;
		padding-left: 0.875rem;
		/* A quiet rule down the side: this is a note in the margin of the
		   conversation, not a panel sitting on top of it. */
		border-left: 1px solid var(--color-border-subtle, var(--color-border));
		width: fit-content;
	}
	dt,
	dd {
		font-size: 0.9375rem;
		line-height: 1.7;
		margin: 0;
	}
	dt {
		color: var(--color-foreground-tertiary, #8a8a8a);
	}
	dd {
		color: var(--color-foreground);
	}
</style>
