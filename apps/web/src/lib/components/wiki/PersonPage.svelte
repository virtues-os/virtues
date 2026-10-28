<!--
	PersonPage.svelte

	Personal CRM wiki page for a person.
	Structured like other wiki pages (DayPage, PlacePage, etc.)
-->

<script lang="ts">
	import { subjectHref } from "$lib/wiki/links";
	import { untrack } from "svelte";
	import type { WikiPersonApi } from "$lib/wiki/api";
	import { parseDateSlug } from "$lib/utils/dateUtils";
	import EntityArticleSection from "./EntityArticleSection.svelte";
	import WikiCrumbs from "./WikiCrumbs.svelte";
	import SubjectBacklinks from "./SubjectBacklinks.svelte";
	import NotesRail from "./NotesRail.svelte";
	import EntityRecordsSection from "./EntityRecordsSection.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import AliasEditor from "./AliasEditor.svelte";
	import Button from "$lib/components/Button.svelte";
	import { updatePerson, samePeople, mergePerson, type SamePersonApi } from "$lib/wiki/api";
	import { reloadApp } from "$lib/reload";
	import TextAction from "$lib/components/TextAction.svelte";
	import { uploadMedia } from "$lib/api/client";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import PersonPicture from "./PersonPicture.svelte";

	interface Props {
		/**
		 * The wire shape, not a converted one. There used to be a `PersonPage`
		 * type and a converter between; both are gone. The converter's real
		 * output was camelCase renames and four fields it never set —
		 * `company`, `role`, `location`, `connectionTier` — which this file
		 * read in nineteen places and rendered as nothing.
		 */
		page: WikiPersonApi;
	}

	let { page }: Props = $props();

	// Dates arrive as ISO strings. `parseDateSlug` reads them as LOCAL dates;
	// `new Date("1990-09-14")` is UTC midnight, which renders as the 13th for
	// everyone west of Greenwich — and a birthday is exactly the field where
	// being a day out is noticed.
	const birthday = $derived(page.birthday ? parseDateSlug(page.birthday) : null);
	const diedOn = $derived(page.died_on ? parseDateSlug(page.died_on) : null);
	const relationship = $derived(page.relationship_category ?? "Contact");

	function formatBirthday(date: Date): string {
		return date.toLocaleDateString("en-US", {
			month: "long",
			day: "numeric",
		});
	}

	// Sections render only when they have content — a wiki article never
	// prints an empty heading (the article stub is the one exception, as an
	// honest invitation).
	const hasContact = $derived(
		Boolean(
			page.emails?.length ||
				page.phones?.length ||
				page.linkedin ||
				page.x ||
				page.instagram
		)
	);
	const hasAbout = $derived(Boolean(birthday || diedOn));

	/** Died, at the precision the person gave — never more exact than they said. */
	function formatDeath(date: Date, precision?: string): string {
		if (precision === "day")
			return date.toLocaleDateString("en-US", { month: "long", day: "numeric", year: "numeric" });
		if (precision === "month")
			return date.toLocaleDateString("en-US", { month: "long", year: "numeric" });
		return String(date.getFullYear());
	}

	async function saveAliases(next: string[]) {
		const saved = await updatePerson(page.id, { aliases: next });
		if (!saved) throw new Error("Could not save aliases");
	}

	// ── the bond ──────────────────────────────────────────────────────────
	// The one AUTHORED line on this page: what this person means, in the
	// owner's words. Everything else here is observed; this is the field the
	// doctrine says only the person can write ("recency says who is around;
	// bonds say who matters"). Rendered above every stat, edited in place.
	// Seeded once from the prop, then locally owned (saves write through).
	let bond = $state(untrack(() => page.bond ?? ""));
	let bondEditing = $state(false);
	let bondDraft = $state("");
	let bondSaving = $state(false);

	function startBondEdit() {
		bondDraft = bond;
		bondEditing = true;
	}

	async function saveBond() {
		bondSaving = true;
		const saved = await updatePerson(page.id, { bond: bondDraft.trim() });
		bondSaving = false;
		if (saved) {
			bond = bondDraft.trim();
			bondEditing = false;
		}
	}

	// ── the picture ───────────────────────────────────────────────────────
	// Comes from their contact card until you choose one here. A picture you
	// choose, or your removing one, is yours: a contact photo never replaces
	// it (the server records whose it is). Seeded once, like the bond.
	let picture = $state(untrack(() => page.picture));
	let pictureBusy = $state(false);
	let pictureError = $state<string | null>(null);
	let pictureButton: HTMLButtonElement | null = $state(null);
	let pictureInput: HTMLInputElement | null = $state(null);

	/** The cover picker's limit, and the same media store. */
	const MAX_PICTURE_BYTES = 20 * 1024 * 1024;

	function choosePicture() {
		pictureInput?.click();
	}

	function pictureMenu() {
		if (!picture || !pictureButton) {
			choosePicture();
			return;
		}
		const r = pictureButton.getBoundingClientRect();
		contextMenu.show(
			{ x: r.left, y: r.bottom },
			[
				{ id: "choose", label: "Choose a new picture", icon: "mynaui:image", action: choosePicture },
				{
					id: "remove",
					label: "Remove picture",
					description: "A photo from your contacts won't take its place",
					icon: "mynaui:trash",
					action: () => savePicture(""),
				},
			],
			{ anchor: { x: r.left, y: r.top, width: r.width, height: r.height }, placement: "bottom-start" },
		);
	}

	/** Save a picture URL, or "" to remove it. */
	async function savePicture(next: string) {
		pictureBusy = true;
		pictureError = null;
		const saved = await updatePerson(page.id, { picture: next });
		pictureBusy = false;
		if (saved) picture = saved.picture;
		else pictureError = "Your server couldn't save the picture. Try again.";
	}

	async function onPicturePicked(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = ""; // so choosing the same file again still fires
		if (!file) return;
		if (!file.type.startsWith("image/")) {
			pictureError = "That file isn't an image. Choose a JPEG, PNG, or WebP.";
			return;
		}
		if (file.size > MAX_PICTURE_BYTES) {
			pictureError = "That image is over 20 MB. Choose a smaller one.";
			return;
		}
		pictureBusy = true;
		pictureError = null;
		let url: string;
		try {
			url = (await uploadMedia(file)).url;
		} catch {
			pictureBusy = false;
			pictureError = "Couldn't upload that image to your server. Try again.";
			return;
		}
		await savePicture(url);
	}

	// ── the same person ───────────────────────────────────────────────────
	// Someone else who holds one of this person's numbers or addresses,
	// almost always a second contact card for the same human. A number two
	// people hold reaches neither page, so its messages wait on this choice.
	// Only you merge: nothing is offered here that you did not confirm.
	let same = $state<SamePersonApi[]>([]);
	let confirming = $state<string | null>(null);
	let merging = $state(false);
	let mergeError = $state<string | null>(null);

	$effect(() => {
		const id = page.id;
		void samePeople(id).then((rows) => {
			if (page.id === id) same = rows;
		});
	});

	function sharedWhat(other: SamePersonApi): string {
		const numbers = other.handles.every((h) => h.startsWith("+"));
		const one = other.handles.length === 1;
		if (numbers) return one ? "number" : "numbers";
		return one ? "address" : "addresses";
	}

	async function confirmMerge(other: SamePersonApi) {
		merging = true;
		mergeError = null;
		const ok = await mergePerson(other.id, page.id);
		merging = false;
		if (ok) reloadApp();
		else mergeError = `Your server couldn't merge ${other.name} into ${page.name}. Try again.`;
	}

	/** "1948 – 2019" under the name once a death is recorded; birthday alone
	 *  stays where it was (the About list). */
	const lifespan = $derived.by(() => {
		if (!diedOn) return null;
		const died = diedOn.getFullYear();
		return birthday ? `${birthday.getFullYear()} – ${died}` : `died ${died}`;
	});

</script>

<div class="person-page-layout">
	<article class="person-article wiki-article">
		<div class="person-content">
			<WikiCrumbs bar parent={{ label: "People", href: "/wiki/people" }} current={page.name} />
			<!-- Header -->
			<header class="person-header">
				<button
					bind:this={pictureButton}
					type="button"
					class="picture-button"
					class:busy={pictureBusy}
					disabled={pictureBusy}
					onclick={pictureMenu}
					aria-label={picture ? `Change or remove ${page.name}'s picture` : `Add a picture of ${page.name}`}
					title={picture ? "Change picture" : "Add a picture"}
				>
					<PersonPicture name={page.name} {picture} size={64} />
				</button>
				<input bind:this={pictureInput} type="file" accept="image/*" hidden onchange={onPicturePicked} />
				<div class="person-heading">
					<h1 class="person-title">{page.name}</h1>
					{#if page.nickname && page.nickname !== page.name}
						<p class="person-subtitle">"{page.nickname}"</p>
					{/if}
					<div class="person-meta">
						<span class="meta-badge">{relationship}</span>
						{#if lifespan}
							<span class="meta-sep">·</span>
							<span class="meta-lifespan">{lifespan}</span>
						{/if}
					</div>
				</div>
			</header>
			{#if pictureError}
				<p class="picture-error" role="alert">{pictureError}</p>
			{/if}

			{#each same as other (other.id)}
				<div class="same-person">
					{#if confirming === other.id}
						<p>
							Merge {other.name} into {page.name}? Their messages, notes and pins move to
							this page, and {other.name}'s page goes away. You can't undo this.
						</p>
						<div class="same-actions">
							<Button variant="primary" size="sm" onclick={() => confirmMerge(other)} loading={merging}>
								Merge into {page.name}
							</Button>
							<Button variant="ghost" size="sm" onclick={() => (confirming = null)} disabled={merging}>
								Cancel
							</Button>
						</div>
						{#if mergeError}<p class="same-error" role="alert">{mergeError}</p>{/if}
					{:else}
						<p>
							{other.name} has the same {sharedWhat(other)} in your contacts.{#if other.messages > 0}
								{" "}{other.messages.toLocaleString()} messages from it show on neither page.{/if}
							<TextAction inline onclick={() => (confirming = other.id)}>Same person?</TextAction>
						</p>
					{/if}
				</div>
			{/each}

			<!-- The bond: authored, above everything observed. -->
			<div class="bond">
				{#if bondEditing}
					<!-- svelte-ignore a11y_autofocus -->
					<input
						class="bond-input"
						bind:value={bondDraft}
						placeholder="What {page.name} means, in a sentence of your own."
						autofocus
						onkeydown={(e) => {
							if (e.key === "Enter") void saveBond();
							if (e.key === "Escape") bondEditing = false;
						}}
					/>
					<Button variant="secondary" size="sm" onclick={saveBond} loading={bondSaving}>
						Save
					</Button>
				{:else if bond}
					<button class="bond-line" type="button" onclick={startBondEdit} title="Edit">
						{bond}
					</button>
				{:else}
					<button class="bond-invite" type="button" onclick={startBondEdit}>
						Your data shows how often you see {page.name}. Write a sentence about what
						this person means to you.
					</button>
				{/if}
			</div>

			<!-- Also known as. Sits directly under the name because that is what
			     it corrects: the record calling someone by a surface the
			     resolver does not recognise. Writing one here backfills every
			     past mention of it (migration 0037). -->
			<div class="person-aliases">
				<AliasEditor
					aliases={page.aliases ?? []}
					canonicalName={page.name}
					onSave={saveAliases}
				/>
			</div>

			<hr class="divider" />

			<!-- The article: machine-written prose about this entity -->
			<section class="section" id="article">
				<EntityArticleSection
					article={page.article}
					articleUpdatedAt={page.article_updated_at
						? new Date(page.article_updated_at)
						: null}
					name={page.name}
					subjectType="person"
					subjectId={page.id}
					maintained={page.article_maintained}
					onChanged={() => reloadApp()}
				/>
			</section>

			<!-- The record: every data point that references this person -->
			<section class="section" id="the-record">
				<h2 class="section-title">The record</h2>
				<EntityRecordsSection entityId={page.id} />
			</section>

			<SubjectBacklinks subjectType="person" subjectId={page.id} />
			<NotesRail subjectType="person" subjectId={page.id} />

			{#if hasContact}
			<!-- Contact -->
			<section class="section" id="contact">
				<h2 class="section-title">Contact</h2>
				{#if hasContact}
					<dl class="info-list">
						{#if page.emails && page.emails.length > 0}
							<div class="info-row">
								<dt>Email</dt>
								<dd>
									{#each page.emails as email, i}
										<a href="mailto:{email}" class="info-link">{email}</a>{#if i < page.emails.length - 1}, {/if}
									{/each}
								</dd>
							</div>
						{/if}
						{#if page.phones && page.phones.length > 0}
							<div class="info-row">
								<dt>Phone</dt>
								<dd>
									{#each page.phones as phone, i}
										<a href="tel:{phone}" class="info-link">{phone}</a>{#if i < page.phones.length - 1}, {/if}
									{/each}
								</dd>
							</div>
						{/if}
						{#if page.linkedin}
							<div class="info-row">
								<dt>LinkedIn</dt>
								<dd><a href="https://linkedin.com/in/{page.linkedin}" target="_blank" class="info-link">{page.linkedin}</a></dd>
							</div>
						{/if}
						{#if page.x}
							<div class="info-row">
								<dt>Twitter</dt>
								<dd><a href="https://x.com/{page.x}" target="_blank" class="info-link">@{page.x}</a></dd>
							</div>
						{/if}
						{#if page.instagram}
							<div class="info-row">
								<dt>Instagram</dt>
								<dd><a href="https://instagram.com/{page.instagram}" target="_blank" class="info-link">@{page.instagram}</a></dd>
							</div>
						{/if}
					</dl>
				{:else}
					<p class="empty-placeholder">No contact info</p>
				{/if}
			</section>
			{/if}

			{#if hasAbout}
			<!-- About -->
			<section class="section" id="about">
				<h2 class="section-title">About</h2>
				<!-- "Work" and "Location" rows lived here and could never draw:
				     `role`, `company` and `location` were optional fields on a
				     page type no converter ever filled. The wire has no such
				     columns, so the section is what the record can actually
				     say. -->
				<dl class="info-list">
					{#if birthday}
						<div class="info-row">
							<dt>Birthday</dt>
							<dd>{formatBirthday(birthday)}</dd>
						</div>
					{/if}
					{#if diedOn}
						<div class="info-row">
							<dt>Died</dt>
							<dd>{formatDeath(diedOn, page.died_precision ?? undefined)}</dd>
						</div>
					{/if}
				</dl>
			</section>
			{/if}


			<!-- There used to be a second "Notes" section here, rendering
			     `wiki_people.content`. Two headings with the same name on one
			     page, and the column is superseded twice over: prose about a
			     person is the ARTICLE, and a note about them is a `wiki_notes`
			     row shown in the rail above. It was empty on every box measured,
			     so the duplicate was latent rather than visible — which is why
			     it survived being added. -->
		</div>
	</article>
</div>

<style>
	.person-page-layout {
		display: flex;
		height: 100%;
		width: 100%;
		overflow: hidden;
	}

	.person-article {
		flex: 1;
		min-width: 0;
		overflow-y: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
		padding: 0.5rem 2rem 2rem;
	}

	.person-article::-webkit-scrollbar {
		display: none;
	}

	.person-content {
		max-width: 48rem;
		margin: 0 auto;
		/* The crumb row above the header takes the top space. */
		padding-top: 0.25rem;
		padding-bottom: 4rem;
	}

	/* Same person */
	.same-person {
		margin: 0 0 1rem;
		padding: 0.75rem 1rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}

	.same-person p {
		margin: 0;
	}

	.same-actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.625rem;
	}

	.same-error {
		margin-top: 0.5rem !important;
		color: var(--color-error);
	}

	/* Header */
	.person-header {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin-bottom: 1rem;
	}

	.person-heading {
		min-width: 0;
	}

	.picture-button {
		flex-shrink: 0;
		display: inline-flex;
		padding: 0;
		border: none;
		border-radius: 50%;
		background: none;
		cursor: pointer;
	}

	.picture-button:hover {
		opacity: 0.85;
	}

	.picture-button:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.picture-button.busy {
		opacity: 0.5;
		cursor: progress;
	}

	.picture-error {
		margin: -0.5rem 0 1rem;
		font-size: 0.8125rem;
		color: var(--color-error);
	}

	.person-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.75rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0;
		line-height: 1.3;
	}

	.person-subtitle {
		font-size: 1rem;
		color: var(--color-foreground-muted);
		margin: 0.25rem 0 0;
	}

	.person-meta {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-top: 0.5rem;
		font-size: 0.875rem;
	}

	.meta-lifespan {
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	/* The bond: one authored serif line, above everything observed. */
	.bond {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		margin: 0.875rem 0 0;
	}

	.bond-line,
	.bond-invite {
		background: none;
		border: none;
		padding: 0;
		text-align: left;
		cursor: pointer;
		font-family: var(--font-serif, Georgia, serif);
		line-height: 1.5;
	}

	.bond-line {
		font-size: 1.0625rem;
		color: var(--color-foreground);
	}

	.bond-invite {
		font-size: 0.9375rem;
		color: var(--color-foreground-subtle);
		max-width: 34rem;
	}

	.bond-invite:hover,
	.bond-line:hover {
		color: var(--color-foreground-muted);
	}

	.bond-input {
		flex: 1;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1rem;
		padding: 0.375rem 0.625rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	/* Sits between the header and the article's rule, indented to the same
	   measure as the prose so it reads as part of the record rather than a
	   form bolted onto it. */
	.person-aliases {
		margin-top: 0.625rem;
	}

	.meta-badge {
		color: var(--color-primary);
		font-weight: 500;
	}

	.meta-sep {
		color: var(--color-border-strong);
	}

	.meta-tier {
		color: var(--color-foreground-muted);
	}

	.divider {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: 1rem 0 1.5rem;
	}

	/* Sections */
	.section {
		margin-bottom: 2rem;
	}

	.section-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.375rem;
		font-weight: 400;
		line-height: 1.35;
		color: var(--color-foreground);
		margin: 0 0 0.75rem;
	}

	/* Info list (dt/dd) */
	.info-list {
		margin: 0;
	}

	.info-row {
		display: flex;
		gap: 1rem;
		padding: 0.375rem 0;
		border-bottom: 1px solid var(--color-border);
	}

	.info-row:last-child {
		border-bottom: none;
	}

	.info-row dt {
		width: 80px;
		flex-shrink: 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.info-row dd {
		flex: 1;
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground);
	}

	.info-link {
		color: var(--color-primary);
		text-decoration: none;
	}

	.info-link:hover {
		text-decoration: underline;
	}

	/* Footer sections */
	.empty-placeholder {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	/* Responsive */
	@media (max-width: 900px) {
		.person-page-layout {
			flex-direction: column;
		}

		.person-article {
			padding: 1rem;
		}

		.person-title {
			font-size: 1.5rem;
		}
	}
</style>
