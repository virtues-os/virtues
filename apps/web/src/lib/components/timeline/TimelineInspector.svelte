<!--
	TimelineInspector.svelte - the day as one column, in the prototype's two tiers
	(dayback/src/main.js:1600-1637, dayback/index.html:912-953): a section for
	where you were (a stay, a drive, a signal gap, In Bed), titled with a
	duration only, and under it the moments that happened there, each with its
	time in the gutter. A section header pins while its rows scroll past.

	A click parks the playhead and takes the map there (`onpick`). The inspector
	follows the playhead (main.js:1607-1615): the latest-begun section holding
	it is where you are, the latest-begun row holding it is what's happening.

	"Help Virtues out" lives here too, where the day is read: a stay whose
	place Virtues can't name is "Unknown · <city>", its question one line
	under it. A yes-or-no question answers there, "Were you at Sandy's?
	Yes  No": "No" opens the prototype's card (ReviewCard) in this column in
	place of the list, for the other answers (on Home or Work it goes
	straight to tapping the map); any other question has one way in,
	"Which of 7 places nearby?  Choose"; the tile over the map steps through the other
	days' questions and opens the same card on each.
-->
<script lang="ts">
	import { untrack } from 'svelte';
	import type { InspectorPick as Pick, InspectorRow, InspectorSection } from '$lib/timeline/inspector';
	import { lineAt, lineTime, rowLines } from '$lib/timeline/transcript';
	import Pill from '$lib/components/timeline/Pill.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import ReviewCard from '$lib/components/timeline/ReviewCard.svelte';
	import { getNearbyPlaces, type NearbyPlace } from '$lib/wiki/api';
	import { askFor, type Answer, type PlaceCandidate } from '$lib/timeline/review';
	import type { ReviewState } from '$lib/timeline/reviewState.svelte';

	let {
		sections,
		zone,
		day,
		playT,
		focus = 'row',
		armed = false,
		onpick,
		onseek,
		onname,
		onrole,
		homeSet = false,
		onmerge,
		review,
		selected = null,
		hovered = null,
		onfocus,
		onopenplace,
		ondetour,
		onlist,
	}: {
		sections: InspectorSection[];
		zone: string;
		/** The day on show, YYYY-MM-DD: the list's header. */
		day: string;
		playT: number;
		/** What the user last pointed at: 'sec' after a section click keeps the
		 *  inspector on that section; anything else hands focus back to the moment. */
		focus?: 'row' | 'sec';
		/** The user has moved the playhead in this day: until then nothing opens itself. */
		armed?: boolean;
		onpick: (p: Pick) => void;
		/** A transcript line was picked: park the playhead at its moment. */
		onseek: (t: number) => void;
		/** Name an unnamed place; resolves false when the server refused. */
		onname: (placeId: string, name: string) => Promise<boolean>;
		/** Make a place Home, or where the person works; resolves false when
		 *  the server refused. */
		onrole?: (placeId: string, role: 'home' | 'work') => Promise<boolean>;
		/** Home is already set somewhere: choosing it here replaces it. */
		homeSet?: boolean;
		/** "Help Virtues out": the questions, the answers and Undo. */
		review?: ReviewState;
		/** The stay or drive picked, from the list or the map: the list shows
		 *  it selected until it's let go. */
		selected?: { kind: 'place' | 'transit'; s: number } | null;
		/** The stay or drive under the pointer on the map: its row lights as
		 *  on hover. */
		hovered?: { kind: 'place' | 'transit'; s: number } | null;
		/** The card's place and the named places it offers, for the map. */
		onfocus?: (at: { lat: number; lon: number; reachM: number; shown: PlaceCandidate[] } | null) => void;
		/** Fold an unnamed place into a named one near it. */
		onmerge: (placeId: string, into: NearbyPlace) => Promise<boolean>;
		/** A name picked in "Named Places": open the day `day` with the place's
		 *  stay picked. */
		onopenplace?: (id: string, day: string) => void;
		/** "Named Places" opened (`true`) or left (`false`); `back` when left
		 *  by "‹ Back to the Day", which returns to the day it was opened on. */
		ondetour?: (open: boolean, back?: boolean) => void;
		/** The "Named Places" list is on show, opened or come back to from a
		 *  name's day: the day behind it goes back to where it was opened. */
		onlist?: () => void;
	} = $props();

	// Naming a place, one at a time. The named places within 150 m come first,
	// as "Same as…": a stay's centre drifts, and the place it drifted to is
	// usually the one already named next door.
	let naming = $state<string | null>(null);
	let draft = $state('');
	let nearby = $state<NearbyPlace[]>([]);
	let saving = $state(false);
	let nameError = $state<string | null>(null);

	function startNaming(placeId: string) {
		naming = placeId;
		replacing = false;
		draft = '';
		nearby = [];
		nameError = null;
		getNearbyPlaces(placeId)
			.then((rows) => {
				if (naming === placeId) nearby = rows;
			})
			.catch(() => {});
	}
	function stopNaming() {
		naming = null;
		nameError = null;
	}
	async function finish(done: Promise<boolean>) {
		saving = true;
		nameError = null;
		const ok = await done.catch(() => false);
		saving = false;
		if (ok) stopNaming();
		else nameError = "Your server couldn't save that. Try again.";
	}
	function saveName(placeId: string) {
		const name = draft.trim();
		if (name && !saving) void finish(onname(placeId, name));
	}
	function sameAs(placeId: string, into: NearbyPlace) {
		if (!saving) void finish(onmerge(placeId, into));
	}
	/** Home asked to replace a Home set elsewhere: one more tap confirms. */
	let replacing = $state(false);
	function setRole(placeId: string, role: 'home' | 'work') {
		if (!onrole || saving) return;
		if (role === 'home' && homeSet && !replacing) {
			replacing = true;
			return;
		}
		replacing = false;
		void finish(onrole(placeId, role));
	}

	let inspector = $state<HTMLElement | null>(null);
	let scroller = $state<HTMLElement | null>(null);
	export function width(): number {
		return inspector?.offsetWidth ?? 0;
	}
	/** Open a conversation row's transcript, as a click on the row does: a
	 *  moment picked on the map opens its row here too. */
	export function expand(s: number) {
		const row = sections.flatMap((x) => x.rows).find((r) => r.s === s);
		if (row?.convs.length) open = s;
	}

	/** "Sunday, September 13": the calendar date itself, not an instant, so
	 *  read at noon UTC and never shifted by a zone. */
	const dayTitle = $derived(
		day ? new Date(`${day}T12:00:00Z`).toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric', timeZone: 'UTC' }) : '',
	);
	const time = (ms: number) =>
		new Date(ms).toLocaleTimeString('en-US', { timeZone: zone, hour: 'numeric', minute: '2-digit' });
	const holds = (x: { s: number; e: number }) => x.s <= playT && playT < x.e;
	// Sections and rows are in start order, so the last one holding the
	// playhead is the latest begun; the highlight never climbs back up. A
	// section the person clicked stays the live one while the playhead is
	// inside it: In Bed and the stay that starts the same minute both hold
	// it, and the click says which one they meant.
	let chosen = $state<number | null>(null);
	const curSection = $derived.by(() => {
		const c = chosen;
		return c !== null && sections[c] && holds(sections[c]) ? c : sections.findLastIndex(holds);
	});

	// "Help Virtues out": each place's question under every stay there that
	// day (one answer names them all). A question's card opens in the list,
	// under the stay it was opened from, with the rest of the day dimmed
	// around it (the owner's call, 2026-10-10: the day is the context); the
	// names Virtues chose open in place of the list.
	// A named place's question can be asked again from "Named Places"
	// (`change`), to change its name with the first time's answers.
	let card = $state<{ id: string; at: number; change?: boolean } | 'names' | null>(null);
	const cardQ = $derived(
		card && card !== 'names' ? ((card.change ? review?.named.get(card.id) : review?.byPlace.get(card.id)) ?? null) : null,
	);
	/** The named place being changed, for what it is now. */
	const changing = $derived.by(() => {
		const c = card;
		return c && c !== 'names' && c.change ? review?.confirmed.find((x) => x.id === c.id) : undefined;
	});
	/** The section the open question sits under. */
	const openAt = $derived(cardQ && card && card !== 'names' ? card.at : null);
	// Answered (or another day on show without it): back to the list, where
	// the stay has its new name and Undo.
	$effect(() => {
		void sections;
		if (card && card !== 'names' && !card.change && (!cardQ || !sections.some((x) => x.placeId === (card as { id: string }).id))) untrack(() => (card = null));
	});
	// "Named Places" is a detour from the day being read: the list, and any
	// name's card opened from it. "‹ Back to the Day" ends it on the day it
	// began from, not the last name's (the owner's call, 2026-10-13).
	const detour = $derived(card === 'names' || !!(card && card.change));
	let inDetour = false;
	let leavingBack = false;
	$effect(() => {
		const d = detour;
		untrack(() => {
			if (d !== inDetour) ondetour?.(d, leavingBack);
			inDetour = d;
			leavingBack = false;
		});
	});
	$effect(() => {
		if (card === 'names') untrack(() => onlist?.());
	});
	// A name changed from "Named Places": back to the list it was picked in,
	// with the new name on it.
	let changesSeen = 0;
	$effect(() => {
		const n = review?.changes ?? 0;
		untrack(() => {
			if (n === changesSeen) return;
			changesSeen = n;
			if (!card || card === 'names' || !card.change) return;
			// "No" to the map's match leaves the place unnamed: its question
			// goes on in the same card, as the first time. Any other change
			// is done.
			card = review?.byPlace.has(card.id) ? { id: card.id, at: card.at } : 'names';
		});
	});
	/** Open the card on a place's question, with stay `at` picked, else the
	 *  place's first stay that day. */
	export function ask(id: string, at?: number) {
		const i = at ?? sections.findIndex((x) => x.placeId === id);
		if (i < 0 || !review?.byPlace.has(id)) return;
		if (i !== curSection) pickSection(i, sections[i]);
		card = { id, at: i };
	}
	/** "No" to a guess on a stay's line: the guess is set aside, and the
	 *  card opens on what the place was, rather than a second "Choose". */
	async function saidNo(id: string, no: Answer, at: number) {
		if (await review?.answer(id, no)) ask(id, at);
	}
	/** Open the card on the names Virtues chose without asking. */
	export function names() {
		card = 'names';
	}
	/** Ask a named place's question again, under its last stay on the day
	 *  on show: "Named Places" opens a name there, to change it. */
	export function revisit(id: string) {
		const i = sections.findLastIndex((x) => x.placeId === id);
		if (i < 0 || !review?.named.has(id)) return void (card = null);
		card = null;
		if (i !== curSection) pickSection(i, sections[i]);
		card = { id, at: i, change: true };
	}
	const curRow = $derived(
		sections
			.flatMap((s) => s.rows)
			.sort((a, b) => a.s - b.s)
			.findLast(holds) ?? null,
	);

	/** The one conversation row whose transcript is open, by its start
	 *  (main.js:1548, 1634-1636); a fresh day starts with none open. */
	let open = $state<number | null>(null);
	/** The live section's detail (In Bed's source). A new live section opens
	 *  itself once the day has been touched; a second click folds it
	 *  (main.js:1610, 1637). */
	let secOpen = $state(false);
	let lastSec = -2;
	$effect(() => {
		void sections;
		untrack(() => {
			open = null;
			secOpen = false;
			lastSec = -2;
			chosen = null;
		});
	});
	$effect(() => {
		const c = curSection;
		untrack(() => {
			if (c !== lastSec) {
				lastSec = c;
				secOpen = armed;
			}
		});
	});

	function pickSection(i: number, sec: InspectorSection) {
		// Another stay picked while a question is open, or its own stay picked
		// again, which lets go of it: the question closes.
		const again = selected?.s === sec.s && selected.kind === sec.kind;
		if (openAt !== null && (i !== openAt || again)) card = null;
		if (i === curSection) secOpen = !secOpen;
		chosen = i;
		open = null;
		onpick({ kind: sec.kind, s: sec.s, e: sec.e });
	}
	function pickRow(row: InspectorRow) {
		if (row.convs.length) open = open === row.s ? null : row.s;
		onpick({ kind: row.kind, s: row.s, e: row.e });
	}

	// The open conversation (main.js:1616-1622): how many voices, the names
	// mentioned (never "present"), and its lines as turns, the one spoken at
	// the playhead lit - an estimate, as the recorder keeps no per-line times.
	const openRow = $derived(sections.flatMap((s) => s.rows).find((r) => r.s === open) ?? null);
	const openLines = $derived(openRow ? rowLines(openRow.convs) : []);
	const openSpeakers = $derived(openRow ? Math.max(0, ...openRow.convs.map((c) => c.speaker_count)) : 0);
	const openPeople = $derived(openRow ? [...new Set(openRow.convs.flatMap((c) => c.people))] : []);
	const liveLine = $derived(openRow && openLines.length ? lineAt(openRow.s, openRow.e, openLines.length, playT) : -1);

	// One scroll surface, kept on the finest live thing (main.js:1627-1633):
	// an open question, else the pointed-at section, else the spoken line,
	// else the live row, else the live section. It glides only when that
	// thing is out of view, and once per target.
	/** The date header's height: the stays' headers pin just under it. */
	let dayHd = $state(0);
	let glide = -1;
	$effect(() => {
		const box = scroller;
		const sec = curSection;
		const row = curRow;
		const asking = openAt;
		const under = dayHd;
		void liveLine;
		if (!box) return;
		const top = box.getBoundingClientRect().top;
		const at = (n: HTMLElement) => n.getBoundingClientRect().top - top + box.scrollTop;
		const sT = box.scrollTop;
		const h = box.clientHeight;
		const group = sec >= 0 ? box.querySelectorAll<HTMLElement>('.group')[sec] : undefined;
		const lineEl = box.querySelector<HTMLElement>('.line.cur');
		const rowEl = row ? box.querySelector<HTMLElement>('.row.cur') : null;
		const sectionView = (g: HTMLElement) => {
			const t = at(g);
			// A section counts as seen while any of it is in the box: its pinned header shows.
			return { seen: t <= sT + h - 40 && t + g.offsetHeight >= sT + 40, want: t - h * 0.28 };
		};
		let view: { seen: boolean; want: number } | null = null;
		const openGroup = asking !== null ? box.querySelector<HTMLElement>('.group.open') : null;
		if (openGroup) {
			// The open question shows whole, its stay and every answer (the
			// stay's conversations may run on below), under the date header:
			// the least scroll that does it, the top kept when the two don't
			// both fit.
			const t = at(openGroup);
			const cardEl = openGroup.querySelector<HTMLElement>('.inline-card');
			const b = cardEl ? at(cardEl) + cardEl.offsetHeight : t + openGroup.offsetHeight;
			const above = t - under - 8;
			view = { seen: t >= sT + under && b <= sT + h, want: b > sT + h ? Math.min(b - h + 16, above) : above };
		} else if (focus === 'sec' && group) view = sectionView(group);
		else if (lineEl) {
			const t = at(lineEl);
			view = { seen: t >= sT + 24 && t <= sT + h - 48, want: t - h * 0.45 };
		} else if (rowEl) {
			const t = at(rowEl);
			view = { seen: t >= sT && t <= sT + h - 40, want: t - h * 0.28 };
		} else if (group) view = sectionView(group);
		if (!view || view.seen) {
			glide = -1;
			return;
		}
		const want = Math.max(0, Math.round(view.want));
		if (want !== glide) {
			glide = want;
			box.scrollTo({ top: want, behavior: 'smooth' });
		}
	});

	// A pinned header is one pressed against the top of the box, poking 1 px
	// out of it: only then does it take a hairline (main.js:1605).
	$effect(() => {
		const box = scroller;
		const under = dayHd;
		void sections;
		if (!box || typeof IntersectionObserver === 'undefined') return;
		const io = new IntersectionObserver(
			(es) => {
				for (const e of es) {
					const pinned = e.intersectionRatio < 1 && e.boundingClientRect.top <= (e.rootBounds?.top ?? 0) + 1;
					(e.target as HTMLElement).classList.toggle('stuck', pinned);
				}
			},
			{ root: box, threshold: [1], rootMargin: `-${under}px 0px 0px 0px` },
		);
		box.querySelectorAll('.sec').forEach((n) => io.observe(n));
		return () => io.disconnect();
	});
</script>

{#if sections.length || card}
	<section class="inspector" bind:this={inspector} aria-label="The day, stay by stay">
		{#if review && card === 'names'}
			<div class="scroll">
				<ReviewCard {review} onback={() => ((leavingBack = true), (card = null))} onfocus={(at) => onfocus?.(at)} onopen={onopenplace ? (c) => c.last_day && onopenplace(c.id, c.last_day) : undefined} />
			</div>
		{:else}
		<div class="scroll" class:asking-open={openAt !== null} style:--day-hd="{dayHd}px" bind:this={scroller}>
			<!-- What the list describes, pinned while it scrolls. The day's
			     standout goes under it when significance lands (the owner's
			     call, 2026-10-10). -->
			<header class="day-hd" bind:clientHeight={dayHd}>{dayTitle}</header>
			{#each sections as sec, i (sec.kind + sec.s)}
				{@const asked = sec.kind === 'place' && sec.placeId ? review?.byPlace.get(sec.placeId) : undefined}
				<div class="group" class:bare={!sec.rows.length} class:cur={i === curSection} class:sel={selected?.s === sec.s && selected.kind === sec.kind} class:hov={hovered?.s === sec.s && hovered.kind === sec.kind} class:asking={!!asked} class:open={openAt === i}>
					<button class="sec" onclick={() => pickSection(i, sec)}>
						<span class="hd">
							<i class="dot {sec.kind}"></i>
							<b>{sec.title}</b>
							<span class="dur">{sec.dur}</span>
							{#if sec.atag}<em class="tag">· {sec.atag}</em>{/if}
						</span>
						{#each sec.notes as line (line)}
							<span class="note">{line}</span>
						{/each}
						{#if secOpen && i === curSection && curRow === null && sec.kind === 'sleep'}
							<span class="note">{sec.src === 'estimated' ? 'Estimated from your usual time in bed' : 'From Health on your iPhone'}</span>
						{/if}
					</button>
					{#if openAt === i && cardQ && review}
						<div class="inline-card">
							<ReviewCard {review} q={cardQ} inline change={changing} onback={() => (card = changing ? 'names' : null)} onfocus={(at) => onfocus?.(at)} />
						</div>
					{:else if asked && review && !review.pointing}
						{@const id = asked.id}
						{@const a = askFor(asked)}
						{@const yes = a.yes}
						<!-- The stay and its question are one thing to point at: the line
						     selects the stay as its header does (the header button is
						     the keyboard's way in), and the hover takes both. -->
						<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
						<div class="ask-line" onclick={(e) => !(e.target as HTMLElement).closest('button') && pickSection(i, sec)}>
							<span>{a.line}</span>
							{#if yes}
								<span class="ask-actions">
									<Pill filled disabled={review.saving} onclick={() => void review.answer(id, yes)}>Yes</Pill>
									<Pill disabled={review.saving} onclick={() => (a.role ? review.startPointing(id, a.role) : a.no ? void saidNo(id, a.no, i) : ask(id, i))}>No</Pill>
								</span>
							{:else}
								<Pill onclick={() => ask(id, i)}>Choose</Pill>
							{/if}
						</div>
					{:else if review?.undo && review.undo.at === sec.placeId}
						<div class="ask-line">
							<span>{review.undo.said}</span>
							<Pill onclick={() => void review.undoLast()}>Undo</Pill>
						</div>
					{:else if sec.kind === 'place' && !sec.named && sec.source !== 'home' && sec.source !== 'work' && sec.placeId && !sec.placeId.startsWith('visit:') && !(review && review.byPlace.has(sec.placeId))}
						{@const placeId = sec.placeId}
						{#if naming === placeId}
							<form class="name-form" onsubmit={(e) => (e.preventDefault(), saveName(placeId))}>
								{#if sec.guessed}<p class="same">Matched by Virtues</p>{/if}
								{#if nearby.length}
									<p class="same">Same as</p>
									<span class="pills">
										{#each nearby as p (p.id)}<Pill disabled={saving} onclick={() => sameAs(placeId, p)}>{p.name}</Pill>{/each}
									</span>
								{/if}
								{#if onrole}
									<span class="pills">
										<Pill disabled={saving} onclick={() => setRole(placeId, 'home')}>{replacing ? 'Replace your Home' : 'Home'}</Pill>
										<Pill disabled={saving} onclick={() => setRole(placeId, 'work')}>Work</Pill>
									</span>
									{#if replacing}<p class="same">This replaces the Home you set.</p>{/if}
								{/if}
								<!-- svelte-ignore a11y_autofocus -->
								<input
									class="name-input"
									bind:value={draft}
									placeholder="Name this place"
									aria-label="Name this place"
									autofocus
									onkeydown={(e) => e.key === 'Escape' && stopNaming()}
								/>
								<span class="pills">
									<Pill filled type="submit" disabled={!draft.trim() || saving}>Save the Name</Pill>
									<Pill quiet onclick={stopNaming}>Cancel</Pill>
								</span>
								{#if nameError}<p class="name-error">{nameError}</p>{/if}
							</form>
						{:else}
							{#if sec.guessed}
								<!-- A name the map chose: no line under it, only Reminders'
								     ⓘ on the row pointed at or selected, which opens the
								     name field to correct it. -->
								<button class="info" class:shown={i === curSection} type="button" aria-label="About this name" title="Matched by Virtues" onclick={() => startNaming(placeId)}>
									<Icon icon="ri:information-line" width="18" />
								</button>
							{:else}
								<span class="name-door" class:shown={i === curSection}>
									<Pill onclick={() => startNaming(placeId)}>Name This Place</Pill>
								</span>
							{/if}
						{/if}
					{/if}
					{#each sec.rows as row (row.kind + row.s)}
						<div class="row" class:cur={row === curRow}>
							<button class="row-hd" onclick={() => pickRow(row)}>
								<span class="t">{time(row.s)}</span>
								<span class="hd">
									<i class="dot {row.kind}"></i>
									<b>{row.title}</b>
									<span class="dur">{row.dur}</span>
									{#if row.convs.length}<span class="chev" class:open={row.s === open} aria-hidden="true">›</span>{/if}
								</span>
							</button>
							{#if row.s === open && openRow}
								<div class="open">
									<span class="sub">{openSpeakers} speaker{openSpeakers !== 1 ? 's' : ''}</span>
									{#if openPeople.length}<span class="people">Mentioned {openPeople.join(', ')}</span>{/if}
									{#if openLines.length}
										<div class="lines">
											{#each openLines as q, li (li)}
												<button class="line" class:cur={li === liveLine} onclick={() => onseek(lineTime(row.s, row.e, li, openLines.length))}>
													{#if q.spk}<span class="spk">{q.spk}</span>{' '}{/if}{q.txt}
												</button>
											{/each}
										</div>
									{/if}
								</div>
							{/if}
						</div>
					{/each}
				</div>
			{/each}
			<!-- Every name Virtues chose or the person set, at the end of every
			     day's list: always there, never on the map (the owner's call,
			     2026-10-13, over a pill that stayed after the last answer). -->
			{#if review?.review && review.confirmed.length}
				<div class="all-names"><Pill onclick={names}>Named Places</Pill></div>
			{/if}
		</div>
		{/if}
	</section>
{/if}

<style>
	/* The day as one column on the right, the page's surface in a hairline
	   like every card over the map. A pane too narrow for the date card beside
	   it starts the inspector under the date card (TimelineView sets
	   --inspector-top). */
	.inspector {
		position: absolute;
		top: var(--inspector-top, 16px);
		bottom: 16px;
		z-index: 10;
		right: 16px;
		width: min(384px, 42%);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		border-radius: var(--tile-radius);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
	}
	/* Narrow pane: a sheet across the bottom third rather than a column. */
	:global(.timeline.narrow) .inspector {
		top: auto;
		left: 16px;
		width: auto;
		height: 34%;
	}
	/* No space above the pinned date header: rows scrolled up would show in
	   it, above the header. */
	.scroll {
		overflow-y: auto;
		padding: 0 0 8px;
		background: inherit;
	}
	.group {
		position: relative;
		background: inherit;
	}
	button {
		display: block;
		width: 100%;
		border: 0;
		background: transparent;
		font: inherit;
		text-align: left;
		color: inherit;
		cursor: pointer;
	}
	/* The section header pins while its rows scroll past, like Contacts. */
	.sec {
		position: sticky;
		/* Pinned under the date header, it pokes 1 px past it: that is how
		   the observer tells pinned from at rest. */
		top: calc(var(--day-hd, 0px) - 1px);
		z-index: 2;
		background: var(--color-surface);
		/* The right edge keeps room for a map-named stay's ⓘ, and so lines
		   its duration up with the rows', which end in a chevron. */
		padding: 16px 40px 8px 16px;
	}
	/* A hairline only while rows are passing beneath it. */
	.sec:global(.stuck) {
		border-bottom: 1px solid var(--color-border);
	}
	.bare .sec {
		padding-bottom: 12px;
	}
	.row {
		position: relative;
		padding: 8px 16px 8px 32px;
	}
	.row-hd {
		display: grid;
		grid-template-columns: 56px 1fr;
		gap: 8px;
	}
	.row:hover {
		background: var(--hover-bg);
	}
	/* A stay's header pins over the rows scrolling beneath it, so its hover
	   is laid over the surface, never see-through. */
	.sec:hover {
		background: linear-gradient(var(--hover-bg), var(--hover-bg)) var(--color-surface);
	}
	/* Selected, as a Mac list selects: a rounded fill in the accent, stronger
	   than the hover's grey, over the stay and its question line; the
	   playhead's edge steps aside for it. */
	/* A stay with its question open stays selected, whatever else lets go
	   of the pick (the map, "Show the whole day"): the card hangs from it. */
	.group:is(.sel, .open) > .sec,
	.group.sel > .ask-line,
	.group:is(.sel, .open):has(> .sec:hover, > .ask-line:hover) > :is(.sec, .ask-line) {
		margin: 0 8px;
		background: color-mix(in srgb, var(--color-primary) 12%, var(--color-surface));
	}
	.group:is(.sel, .open) > .sec {
		width: calc(100% - 16px);
		padding-left: 8px;
		padding-right: 32px;
		border-radius: var(--radius-xl);
	}
	/* A line under the stay (its question, or "Saved as… Undo") joins the
	   selection: the row squares off where the line meets it. */
	.group:is(.sel, .open) > .sec:not(:last-child) {
		border-radius: var(--radius-xl) var(--radius-xl) 0 0;
	}
	.group.sel > .ask-line {
		padding-left: 24px;
		padding-right: 32px;
		border-radius: 0 0 var(--radius-xl) var(--radius-xl);
	}
	.group:is(.sel, .open)::before {
		display: none;
	}
	/* A stay and its question line light together, wherever the pointer is:
	   on the row, or on its stretch of path on the map. */
	.group.hov > :is(.sec, .ask-line),
	.group:has(> .sec:hover, > .ask-line:hover) > :is(.sec, .ask-line) {
		background: linear-gradient(var(--hover-bg), var(--hover-bg)) var(--color-surface);
	}
	/* Where the playhead is: an edge in the accent, which means now. */
	.group.cur::before,
	.row.cur::before {
		content: '';
		position: absolute;
		left: 0;
		top: 8px;
		bottom: 8px;
		width: 3px;
		border-radius: var(--radius-full);
		background: var(--c-sel);
		z-index: 3;
	}
	.group.cur::before {
		opacity: 0.35;
	}
	.hd {
		display: flex;
		align-items: baseline;
		gap: 8px;
		min-width: 0;
	}
	/* What a stretch was, by form: a stay is filled ink, a drive a hollow
	   ring, a gap a dashed one, a night half-filled, a conversation a small
	   dot. No hue (design-grammar §5). */
	.dot {
		flex: 0 0 8px;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		position: relative;
		top: -1px;
		box-sizing: border-box;
	}
	.dot.place {
		background: var(--c-place);
	}
	.dot.transit,
	.dot.walk {
		border: 1.5px solid var(--c-move);
	}
	/* A ring broken into four even dashes, as the map's dashed bridge across
	   a gap: a dashed border at 8 px draws two or three uneven ones. */
	.dot.gap {
		background: repeating-conic-gradient(var(--c-gap) 0 54deg, transparent 54deg 90deg);
		-webkit-mask: radial-gradient(circle, transparent 2.5px, #000 2.6px);
		mask: radial-gradient(circle, transparent 2.5px, #000 2.6px);
	}
	.dot.sleep {
		border: 1.5px solid var(--c-rest-ink);
		background: linear-gradient(90deg, var(--c-rest-ink) 50%, transparent 50%);
	}
	.dot.conversation {
		flex-basis: 6px;
		width: 6px;
		height: 6px;
		background: var(--c-voice);
	}
	/* Two voices: a section names where you were in the serif, a row says
	   what happened in the sans; durations and times are the caption. */
	.hd b {
		flex: 1 1 auto;
		min-width: 0;
		font-family: var(--font-sans);
		font-size: 14px;
		font-weight: 400;
		line-height: 1.3;
		color: var(--color-foreground);
	}
	/* A conversation's title is the start of its summary: two lines at most,
	   and the open transcript holds the rest. */
	.row .hd b {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.sec .hd b {
		font-family: var(--font-serif-ui);
		font-size: 18px;
		line-height: 1.2;
	}
	/* Nothing filed here (a drive, a gap): a quieter line. */
	.bare .sec .hd b {
		color: var(--color-foreground-muted);
	}
	.row.cur .hd b {
		color: var(--color-foreground);
	}
	.dur,
	.t {
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.t {
		text-align: right;
	}
	.note {
		display: block;
		margin: 4px 0 0 16px;
		font-family: var(--font-sans);
		font-size: 13px;
		line-height: 1.3;
		color: var(--color-foreground-muted);
	}
	/* A conversation row opens its transcript: the chevron says so, and
	   turns down while it is open. */
	.chev {
		flex: 0 0 auto;
		font-size: 14px;
		line-height: 1;
		color: var(--color-foreground-subtle);
		transition: transform 0.18s ease;
	}
	.chev.open {
		transform: rotate(90deg);
	}
	@media (prefers-reduced-motion: reduce) {
		.chev {
			transition: none;
		}
	}
	/* A quiet stay's audio note, after its duration. */
	.tag {
		flex: 0 0 auto;
		font-style: normal;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
	/* The open conversation, under its row and out of the time gutter: the
	   voices, the names mentioned, then the lines. */
	.open {
		display: block;
		margin-top: 4px;
	}
	.sub,
	.people {
		display: block;
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.people {
		margin-top: 4px;
	}
	.lines {
		margin-top: 8px;
		padding-top: 8px;
		border-top: 1px solid var(--color-border);
	}
	.line {
		padding: 4px 0;
		font-family: var(--font-sans);
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
	.line:hover {
		color: var(--color-foreground);
	}
	/* The line being spoken at the playhead. */
	.line.cur {
		color: var(--color-foreground);
	}
	/* Naming a place: a quiet verb under the stay at the playhead (and under
	   any stay you point at), then a field where the name goes. */
	.name-door {
		display: none;
		padding: 0 16px 8px 32px;
		font-size: 13px;
	}
	.name-door.shown,
	.group:hover .name-door,
	.name-door:focus-within {
		display: block;
	}
	.name-form {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 0 16px 12px 32px;
	}
	/* "Help Virtues out" in the list: a question as its stay's second line,
	   in the type of the stay's notes, with one way in; an
	   answer's Undo takes the same line. */
	.ask-actions,
	.pills {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	/* A long question wraps; its buttons stay side by side. */
	.ask-actions {
		flex: 0 0 auto;
		flex-wrap: nowrap;
	}
	.ask-line {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin: 0;
		/* The buttons end where the durations above them do. */
		padding: 0 40px 8px 32px;
		font-size: 13px;
		line-height: 1.3;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.same {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	/* Reminders' ⓘ: at the end of the stay's header, shown on the row
	   pointed at or selected; the header keeps room for it. */
	.info {
		position: absolute;
		top: 16px;
		right: 12px;
		z-index: 3;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		background: none;
		color: var(--color-primary);
		cursor: pointer;
		opacity: 0;
	}
	.info.shown,
	.group:hover .info,
	.info:focus-visible {
		opacity: 1;
	}
	.name-input {
		width: 100%;
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-background);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 14px;
	}
	.name-input:focus {
		outline: none;
		border-color: var(--color-primary);
	}
	.name-error {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground);
	}
	.spk {
		font-weight: 600;
		color: var(--color-foreground);
	}
	/* A question open in the list: its stay and card at full strength, the
	   rest of the day dimmed around it, still there to read and to pick. The
	   contents dim, never a pinned header's surface, which would let the
	   rows beneath show through it; the ⓘ keeps its own showing and
	   hiding. */
	.scroll.asking-open .group:not(.open) > :not(.sec, .info),
	.scroll.asking-open .group:not(.open) > .sec > :not(.info),
	.scroll.asking-open .group:not(.open)::before {
		opacity: 0.45;
		transition: opacity 160ms ease;
	}
	.scroll.asking-open .group:not(.open):hover > :not(.sec, .info),
	.scroll.asking-open .group:not(.open):hover > .sec > :not(.info) {
		opacity: 0.8;
	}
	/* The open question hangs inside the stay's selection: the tint runs on
	   under the card and rounds off below it, so stay and question read as
	   one thing. */
	.inline-card {
		margin: 0 8px 8px;
		padding: 0 8px 8px;
		border-radius: 0 0 var(--radius-xl) var(--radius-xl);
		background: color-mix(in srgb, var(--color-primary) 12%, var(--color-surface));
	}
	@media (prefers-reduced-motion: reduce) {
		.scroll.asking-open .group:not(.open) > :not(.sec, .info),
		.scroll.asking-open .group:not(.open) > .sec > :not(.info) {
			transition: none;
		}
	}
	/* In line with the date header above the list. */
	.all-names {
		padding: 8px 16px 16px;
	}
	/* The list's subject, quiet in the muted colour, pinned over the rows
	   as they scroll. */
	.day-hd {
		position: sticky;
		top: 0;
		z-index: 3;
		padding: 16px 16px 8px;
		background: var(--color-surface);
		font-family: var(--font-sans);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
</style>
