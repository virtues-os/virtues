<script lang="ts">
	/**
	 * A report's masthead, in two parts around the page's title. `top` is the
	 * thin line above it: "Virtues · <label>" on the left and the day the
	 * page was written on the right. `below` sits under the title: the
	 * byline with the last day its records cover, its subjects (each opens),
	 * and the abstract, set apart before the text begins.
	 *
	 * The abstract is edited in place where the page is editable. The rest
	 * comes from whoever wrote the report, and changes with the page's
	 * options.
	 */
	import type { Masthead } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import { identityOf } from '$lib/refs/identity.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let {
		masthead,
		part,
		writtenAt,
		editable = false,
		onAbstract,
	}: {
		masthead: Masthead;
		part: 'top' | 'below';
		/** When the page was created, for the date on the top line. */
		writtenAt: string;
		editable?: boolean;
		/** Save the abstract; resolves whether it saved, so a failed save keeps what was typed. */
		onAbstract?: (text: string) => Promise<boolean>;
	} = $props();

	function longDate(iso: string): string {
		const d = iso.length === 10 ? new Date(`${iso}T12:00:00`) : new Date(iso);
		return d.toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });
	}

	const subjects = $derived(
		(masthead.subjects ?? []).map((url) => ({ url, id: identityOf(url) })),
	);
	const byline = $derived(
		[masthead.byline, masthead.records_through ? `Records through ${longDate(masthead.records_through)}` : null]
			.filter(Boolean)
			.join(' · '),
	);

	let draft = $state('');
	let focused = $state(false);
	/** Typed but not saved: kept on show until it saves. */
	let unsaved = $state(false);
	$effect(() => {
		if (!focused && !unsaved) draft = masthead.abstract ?? '';
	});

	async function saveAbstract() {
		focused = false;
		const text = draft.trim();
		if (text === (masthead.abstract ?? '') || !onAbstract) return;
		unsaved = true;
		unsaved = !(await onAbstract(text));
	}
</script>

{#if part === 'top'}
	<div class="top">
		<span>Virtues · {masthead.label || 'Report'}</span>
		<span class="date">{longDate(writtenAt)}</span>
	</div>
{:else}
	<div class="below">
		{#if byline}<p class="byline">{byline}</p>{/if}
		{#if subjects.length > 0}
			<p class="subjects">
				<span class="subjects-label">Subjects</span>
				{#each subjects as s, i (s.url)}
					<button
						type="button"
						class="subject"
						onclick={() => windowShellStore.openTabFromRoute(s.url, { label: s.id?.title, focusExisting: true })}
					>
						{#if s.id?.icon}<Icon icon={s.id.icon} width="14" />{/if}
						<span>{s.id?.title ?? s.url}</span>
					</button>{i < subjects.length - 1 ? ' · ' : ''}
				{/each}
			</p>
		{/if}
		{#if masthead.abstract || editable}
			<section class="abstract" class:empty={!masthead.abstract} aria-label="Abstract">
				<h2 class="abstract-label">Abstract</h2>
				{#if editable}
					<textarea
						class="abstract-text"
						bind:value={draft}
						rows="3"
						placeholder="What was asked, how it was answered, and what was found."
						aria-label="Abstract"
						maxlength={2000}
						onfocus={() => (focused = true)}
						onblur={saveAbstract}
					></textarea>
				{:else}
					<p class="abstract-text">{masthead.abstract}</p>
				{/if}
			</section>
		{/if}
	</div>
{/if}

<style>
	/* The line above the title, as a journal's running head. */
	.top {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 16px;
		margin-bottom: 40px;
		padding-bottom: 8px;
		border-bottom: 1px solid var(--color-border);
		font-family: var(--font-sans);
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
	}
	.date {
		font-variant-numeric: tabular-nums;
	}
	.below {
		margin: 8px 0 40px;
		text-align: center;
	}
	.byline,
	.subjects {
		margin: 0;
		font-family: var(--font-sans);
		font-size: var(--text-small);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.subjects {
		margin-top: 4px;
	}
	.subjects-label {
		margin-right: 8px;
	}
	.subject {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0;
		border: 0;
		background: transparent;
		font: inherit;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.subject:hover {
		color: var(--color-primary);
	}
	/* The abstract: set in from both sides, its label above it, then the
	   rule the text begins under. */
	.abstract {
		margin: 32px auto 0;
		max-width: 85%;
		padding-bottom: 32px;
		border-bottom: 1px solid var(--color-border);
		text-align: left;
	}
	.abstract-label {
		margin: 0 0 8px;
		font-family: var(--font-sans);
		font-size: var(--text-small);
		font-weight: 500;
		line-height: 20px;
		text-align: center;
		color: var(--color-foreground);
	}
	.abstract-text {
		display: block;
		width: 100%;
		margin: 0;
		padding: 0;
		border: 0;
		background: transparent;
		resize: none;
		outline: none;
		field-sizing: content;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-body);
		line-height: 24px;
		color: var(--color-foreground);
		hyphens: auto;
		text-align: justify;
	}
	.abstract-text::placeholder {
		color: var(--color-foreground-subtle);
	}
	@media print {
		/* An abstract not yet written prints nothing, its label included. */
		.abstract.empty {
			display: none;
		}
		.top,
		.byline,
		.subjects,
		.abstract-label {
			color: black;
		}
		.subject {
			color: black;
		}
		.top,
		.abstract {
			border-color: var(--color-foreground-subtle);
		}
	}
</style>
