<!--
	HomeView.svelte — the home page.

	Every other room here is one-directional: the wiki is you reading the
	record, chat is you interrogating it, the day page is its account of you
	written overnight. Home is the only place the record and the person are in
	the room at the same time, with the day still open — so the page is built
	as turns of that meeting rather than as a dashboard.

	  speaks   one counted observation, no model involved
	  shows    today, from the raw streams, scrubbable down to the rows
	  opens    the work you had in your hands last
	  asks     the one thing only the owner can answer
	  answers  the one line you choose to keep

	A rule is the box talking; a card is you answering. That is the only
	decoration the page has, and it is load-bearing — so a new block on this
	page has to pick a side before it picks a style.

	Today is drawn from raw streams because the interpreted dayline is composed
	overnight — rather than wait for prose, this draws the record itself while
	it is still arriving. Nothing is written to fill a slot: a section with
	nothing to say does not render.

	Live means polling: there is no ingest broadcast in the box yet, so the deck
	refetches on a 30s beat and stops entirely while the tab is hidden.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import {
		getDayByDate,
		listNotes,
		createNote,
		getDayTimeline,
		getTodayStreams,
		getDayHeartRate,
		getLifeline,
		getWeatherNow,
		getFeed,
		type LifelineRecord,
		type WikiDayApi,
		type WikiNote,
		type TimelineDayPoint,
		type TodayStreamsView,
		type DayHeartRateSample,
		type LifelineData,
		type WeatherNow,
	} from "$lib/wiki/api";
	import { getStreamHealth, type StreamHealth } from "$lib/api/client";
	import { Page } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { notebookStore } from "$lib/stores/notebook.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import DayDeck from "$lib/components/home/DayDeck.svelte";
	import DayGround from "$lib/components/home/DayGround.svelte";
	import DayNovelty from "$lib/components/home/DayNovelty.svelte";
	import PlaceAsk from "$lib/components/home/PlaceAsk.svelte";

	// A `TEMP-VERIFY` global fetch patch lived here until 2026-08-05: it
	// rewrote every `/api/` call to `http://127.0.0.1:7117` from this module's
	// body. Removed because it overrode all three real paths at once — the
	// vite dev proxy (`/api` → :8000, vite.config.ts), same-origin on the
	// box-served desktop app, and the mobile shell's injected origin
	// (`initBackendFromShell`, lib/config/backend.ts). It patched
	// `window.fetch` globally, so the blast radius was the whole app, not this
	// view. It also reached TestFlight in 1.2.5.
	//
	// If you need this view pointed at a real box while developing, set
	// `BACKEND_URL` for the vite proxy rather than patching fetch.

	// ---- the day's window ----
	// The browser's zone decides which day this is and where its midnights
	// fall; the same zone is sent to the server so both ends agree. Day length
	// is measured rather than assumed — a DST day is 23 or 25 hours long.
	function ymd(d: Date): string {
		return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
	}
	const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
	const _now = new Date();
	const todayDate = ymd(_now);
	const yesterdayDate = ymd(new Date(_now.getTime() - 86400000));
	const dayStartMs = new Date(`${todayDate}T00:00:00`).getTime();
	const dayEndMs = new Date(`${ymd(new Date(_now.getTime() + 86400000))}T00:00:00`).getTime();

	// ---- clock ----
	let nowMs = $state(Date.now());
	let clock = $state("");
	let dateline = $state("");
	function tick() {
		const d = new Date();
		nowMs = d.getTime();
		const ap = d.getHours() >= 12 ? "pm" : "am";
		clock = `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, "0")} ${ap}`;
		dateline = d.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });
	}

	// ---- state ----
	let yDay = $state<WikiDayApi | null>(null);
	let today = $state<WikiDayApi | null>(null);
	let streams = $state<TodayStreamsView | null>(null);
	let heart = $state<DayHeartRateSample[]>([]);
	let life = $state<LifelineData | null>(null);
	let points = $state<TimelineDayPoint[]>([]);
	let health = $state<Record<string, StreamHealth>>({});
	let weather = $state<WeatherNow | null>(null);
	let notes = $state<WikiNote[]>([]);
	let scrubMs = $state<number | null>(null);
	let pinnedMs = $state<number | null>(null);

	// ---- the moment: the rows that actually fall where you clicked ----
	// A timeline that cannot hand back its rows is a picture of a life rather
	// than a way into one. The window is ±15 minutes, matching the deck's own
	// bucket, so what you point at is what you get.
	const MOMENT_MS = 15 * 60_000;
	let moment = $state<{ at: number; records: LifelineRecord[]; more: boolean } | null>(null);
	let momentLoading = $state(false);

	$effect(() => {
		const at = pinnedMs;
		if (at == null) {
			moment = null;
			return;
		}
		let dropped = false;
		momentLoading = true;
		getFeed(new Date(at - MOMENT_MS).toISOString(), new Date(at + MOMENT_MS).toISOString(), { limit: 24 })
			.then((f) => {
				if (dropped) return;
				moment = { at, records: f?.records ?? [], more: f?.has_more ?? false };
			})
			.catch(() => {
				if (!dropped) moment = { at, records: [], more: false };
			})
			.finally(() => {
				if (!dropped) momentLoading = false;
			});
		return () => {
			dropped = true;
		};
	});

	// ---- the live set: everything the deck redraws from ----
	async function refresh() {
		const from = new Date(dayStartMs).toISOString();
		const to = new Date(dayEndMs).toISOString();
		await Promise.allSettled([
			// getTodayStreams sends the browser zone itself.
			getTodayStreams(todayDate).then((s) => (streams = s)),
			getDayHeartRate(todayDate, tz).then((h) => (heart = h)),
			getDayTimeline(todayDate).then((t) => {
				if (t?.points) points = t.points;
			}),
			// One call carries three tracks: steps, screen time, messages sent.
			getLifeline(96, from, to, undefined, {
				health: "steps",
				activity: "screen",
				communication: "sent",
			}).then((l) => (life = l)),
			getDayByDate(todayDate).then((d) => {
				today = d;
				if (d?.id) return listNotes("day", d.id).then((n) => (notes = n));
			}),
		]);
	}

	onMount(() => {
		tick();
		refresh();
		getDayByDate(yesterdayDate).then((d) => (yDay = d)).catch(() => {});
		getWeatherNow().then((w) => (weather = w)).catch(() => {});
		// Arm state changes on the scale of connecting a source, not of a day.
		getStreamHealth()
			.then((rows) => (health = Object.fromEntries(rows.map((r) => [r.name, r]))))
			.catch(() => {});
		notebookStore.load?.();
		if (!pagesStore.pages.length) pagesStore.loadPages();
		if (!chatSessions.sessions.length && !chatSessions.isLoading) chatSessions.load();

		const clockId = setInterval(tick, 10_000);
		let dataId = setInterval(refresh, 30_000);
		// A background tab has nothing to show; it should not keep the box busy.
		function onVisible() {
			clearInterval(dataId);
			if (!document.hidden) {
				tick();
				refresh();
				dataId = setInterval(refresh, 30_000);
			}
		}
		document.addEventListener("visibilitychange", onVisible);
		return () => {
			clearInterval(clockId);
			clearInterval(dataId);
			document.removeEventListener("visibilitychange", onVisible);
		};
	});

	function open(route: string, label?: string) {
		windowShellStore.openTabFromRoute(route, label ? { label } : undefined);
	}

	// ---- lead: yesterday in one line (epigraph, else its first sentence) ----
	const leadLine = $derived.by(() => {
		const e = yDay?.epigraph?.trim();
		if (e) return e.replace(/^["“]|["”]$/g, "");
		const auto = yDay?.autobiography?.trim();
		const first = auto?.match(/^.*?[.!?](?:\s|$)/)?.[0]?.trim();
		return first || null;
	});

	// ---- the page's subtitle: the day, the weather, the time ----
	const subtitle = $derived.by(() => {
		const wx =
			weather && weather.temperature_c != null
				? `${Math.round((weather.temperature_c * 9) / 5 + 32)}° ${weather.condition}`.trim()
				: null;
		return [dateline, wx, clock].filter(Boolean).join(" · ");
	});

	// ---- recents: notebooks, pages and chats blended by recency ----
	// Not "desk" — the sidebar's Desk is the pinned shelf, and two different
	// meanings for one word is one too many.
	type RecentItem = { route: string; title: string; kind: string; ts: number; note?: string };
	const recentItems = $derived.by<RecentItem[]>(() => {
		const nb: RecentItem[] = notebookStore.notebooks.map((n: any) => ({
			route: `/notebook/${n.id}`,
			title: n.name || "Untitled",
			kind: "notebook",
			ts: n.updated_at ? Date.parse(n.updated_at) : 0,
			note: n.current_status ? "live" : undefined,
		}));
		const pg: RecentItem[] = pagesStore.pages.map((p) => ({
			route: `/page/${p.id}`,
			title: p.title || "Untitled",
			kind: "page",
			ts: p.updated_at ? Date.parse(p.updated_at) : 0,
		}));
		const ch: RecentItem[] = chatSessions.sessions.map((c) => ({
			route: `/chat/${c.conversation_id}`,
			title: c.title || "Untitled",
			kind: "chat",
			ts: c.last_updated ? Date.parse(c.last_updated) : 0,
		}));
		return [...nb, ...pg, ...ch].sort((a, b) => b.ts - a.ts).slice(0, 5);
	});

	function ago(iso: string | null): string {
		if (!iso) return "";
		const m = Math.round((Date.now() - Date.parse(iso)) / 60000);
		if (isNaN(m)) return "";
		if (m < 60) return `${m}m`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h`;
		const d = Math.floor(h / 24);
		return d < 7 ? `${d}d` : `${Math.floor(d / 7)}w`;
	}

	// ---- the keep: one line, stored as a note on today ----
	// A line you choose is note-shaped: it is about the day, not the day's own
	// account of itself. Longform belongs in the day's article, which you claim
	// by editing it. Saving stays here — you can keep a second line, and see
	// what you already kept, without losing the page.
	let keepText = $state("");
	let keeping = $state(false);
	let keepError = $state<string | null>(null);
	let keepEl = $state<HTMLTextAreaElement | undefined>(undefined);
	const keptToday = $derived(notes.filter((n) => n.author === "human"));

	/**
	 * Grow to the text, up to a point — past that it wants the day's article.
	 *
	 * `auto` before measuring, so an already-tall box reports its content rather
	 * than latching at whatever height it reached and never shrinking back.
	 */
	function grow() {
		const el = keepEl;
		if (!el) return;
		el.style.height = "auto";
		el.style.height = `${Math.min(el.scrollHeight, 160)}px`;
	}
	// Driven by the value rather than by the input event, so it is correct after
	// a programmatic clear (saving) as well as after typing.
	$effect(() => {
		keepText;
		grow();
	});

	async function keep() {
		const body = keepText.trim();
		if (!body || keeping) return;
		keeping = true;
		keepError = null;
		try {
			const day = today ?? (await getDayByDate(todayDate));
			if (!day?.id) throw new Error("no day");
			const saved = await createNote("day", day.id, body, "memo");
			notes = [...notes, saved];
			keepText = "";
		} catch {
			keepError = "That didn't save. The box may be offline — try again.";
		} finally {
			keeping = false;
		}
	}
	/** Most rows carry no `kind` of their own; the ontology names them well
	 *  enough once its domain prefix is dropped. */
	function recKind(r: LifelineRecord): string {
		if (r.kind) return r.kind;
		return r.ontology.replace(/^[a-z]+_/, "").replace(/_/g, " ");
	}
	function recTime(iso: string): string {
		const d = new Date(iso);
		return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
	}
	function keptTime(iso: string): string {
		const d = new Date(iso);
		const ap = d.getHours() >= 12 ? "pm" : "am";
		return `${d.getHours() % 12 || 12}:${String(d.getMinutes()).padStart(2, "0")} ${ap}`;
	}
</script>

<Page title="Home" description={subtitle} maxWidth="wide">
	{#snippet actions()}
		<!-- The two adjacent days, as a stepper: they are neighbours on one axis,
		     which a pair of loose links did not say. -->
		<div class="days" role="group" aria-label="Go to a day">
			<button type="button" onclick={() => open(`/day/day_${yesterdayDate}`, "Yesterday")}>
				<Icon icon="ri:arrow-left-s-line" width="15" />
				Yesterday
			</button>
			<button type="button" class="now" onclick={() => open(`/day/day_${todayDate}`, "Today")}>
				Today
				<Icon icon="ri:arrow-right-s-line" width="15" />
			</button>
		</div>
	{/snippet}

	<div class="body">
		<!-- The box speaks — today's rhythm against the trailing twelve weeks —
		     then, if it wrote one, quotes itself. -->
		<div class="rv">
			<DayNovelty {dayStartMs} {nowMs} {tz} />
			{#if leadLine}
				<p class="lead">{leadLine}</p>
			{/if}
		</div>

		<section class="today rv" style="animation-delay:.06s">
			<div class="tbody" class:solo={points.length <= 1}>
				<DayDeck
					{dayStartMs}
					{dayEndMs}
					{nowMs}
					{streams}
					{heart}
					{life}
					{points}
					sleepCycles={today?.sleep_cycles ?? []}
					{health}
					bind:scrubMs
					bind:pinnedMs
				/>
				{#if points.length > 1}
					<aside class="ground">
						<span class="glabel mono">map</span>
						<div class="gcanvas"><DayGround {points} scrubMs={scrubMs ?? pinnedMs} {nowMs} /></div>
					</aside>
				{/if}
			</div>

			{#if pinnedMs != null}
				<div class="moment">
					<div class="mhead">
						<span class="mono mt">{keptTime(new Date(pinnedMs).toISOString())}</span>
						<span class="mlabel">± 15 minutes</span>
						<button class="mclose" type="button" onclick={() => (pinnedMs = null)} aria-label="Close this moment">
							<Icon icon="ri:close-line" width="15" />
						</button>
					</div>
					{#if momentLoading && !moment}
						<p class="mnone">Looking…</p>
					{:else if moment && moment.records.length}
						<ul class="mlist">
							{#each moment.records as r (r.id + r.at)}
								<li>
									<span class="mono rt">{recTime(r.at)}</span>
									<span class="rk mono">{recKind(r)}</span>
									<span class="rb">
										{r.label ?? "—"}{#if r.preview}<span class="rp"> {r.preview}</span>{/if}
									</span>
								</li>
							{/each}
						</ul>
						{#if moment.more}<p class="mnone">More rows fall in this window than are shown.</p>{/if}
					{:else}
						<p class="mnone">The record holds nothing here.</p>
					{/if}
				</div>
			{/if}
		</section>

		{#if recentItems.length}
			<section class="recents rv" style="animation-delay:.12s">
				<h2 class="kicker">recents</h2>
				{#each recentItems as it (it.route)}
					<div class="line">
						<!-- The kicker's space has to come from CSS: leading whitespace
						     inside the span is trimmed, which rendered "Untitled— page". -->
						<button class="t" type="button" onclick={() => open(it.route, it.title)}>
							{it.title}{#if it.kind !== "notebook"}<span class="s">— {it.kind}</span>{/if}
						</button>
						<span class="d mono">{it.note ?? ago(new Date(it.ts).toISOString())}</span>
					</div>
				{/each}
			</section>
		{/if}

		<!-- The box asks. -->
		<div class="rv" style="animation-delay:.16s"><PlaceAsk /></div>

		<section class="keep rv" style="animation-delay:.2s">
			<!-- The card is the writing surface: the question is the only prompt,
			     so clicking anywhere that isn't already a control puts the cursor
			     where you'd expect it. -->
			<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
			<div
				class="card"
				onclick={(e) => {
					if (!(e.target as HTMLElement).closest("button")) keepEl?.focus();
				}}
			>
				<h2 class="kq">What do you want to remember from today?</h2>
				<textarea
					bind:this={keepEl}
					bind:value={keepText}
					disabled={keeping}
					rows="1"
					aria-label="What do you want to remember from today?"
					onkeydown={(e) => {
						// Enter saves. A longer thought still wants a second line,
						// so Shift holds it.
						if (e.key === "Enter" && !e.shiftKey) {
							e.preventDefault();
							keep();
						}
					}}
				></textarea>
				{#if keepText.trim() || keeping}
					<div class="krow">
						<span class="khint">Shift + Enter for a new line</span>
						<button class="ksave" type="button" onclick={keep} disabled={keeping}>
							{keeping ? "Saving…" : "Save"}
						</button>
					</div>
				{/if}

				{#if keepError}<p class="kerr">{keepError}</p>{/if}

				{#if keptToday.length}
					<ul class="kept">
						{#each keptToday as n (n.id)}
							<li><span class="kt mono">{keptTime(n.created_at)}</span><span class="kb">{n.body}</span></li>
						{/each}
					</ul>
					<button class="link sm kfoot" type="button" onclick={() => open(`/day/day_${todayDate}`, "Today")}>
						In the margin of today's page <span class="arw">→</span>
					</button>
				{/if}
			</div>
		</section>
	</div>
</Page>

<style>
	.mono { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }

	.rv { opacity: 0; transform: translateY(7px); animation: rv 0.7s cubic-bezier(0.2, 0.7, 0.2, 1) forwards; }
	@keyframes rv { to { opacity: 1; transform: none; } }
	@media (prefers-reduced-motion: reduce) { .rv { animation: none; opacity: 1; transform: none; } }

	.kicker { font-family: var(--font-mono); font-size: 10.5px; letter-spacing: 0.04em; color: var(--color-foreground-subtle); margin: 0 0 18px; font-weight: 400; }
	.link { font-family: var(--font-sans); font-size: 13.5px; font-weight: 500; color: var(--color-primary); background: none; border: 0; padding: 0; cursor: pointer; }
	.link:hover { text-decoration: underline; text-underline-offset: 3px; }
	.link .arw { opacity: 0.7; }
	.link.sm { font-size: 12.5px; font-weight: 400; }

	/* Day stepper — sits in the page heading's action slot, so it lines up with
	   the "New X" button every other room puts there. */
	.days { display: inline-flex; border: 1px solid var(--color-border); border-radius: 8px; overflow: hidden; background: var(--color-surface-elevated); }
	.days button {
		display: inline-flex; align-items: center; gap: 3px;
		padding: 7px 12px; background: none; border: 0;
		font-family: var(--font-sans); font-size: 13px; font-weight: 500;
		color: var(--color-foreground-muted); cursor: pointer; white-space: nowrap;
	}
	.days button + button { border-left: 1px solid var(--color-border); }
	.days button:hover { background: var(--hover-bg); color: var(--color-foreground); }
	.days button.now { color: var(--color-foreground); }

	/* Yesterday's own sentence. Quieter than the page's h1 on purpose — it is a
	   line the box wrote, not the name of the room. */
	.lead {
		font-family: var(--font-serif); font-size: 19px; line-height: 1.45;
		color: var(--color-foreground-muted); margin: 0 0 26px; max-width: 58ch;
	}

	/* the deck */
	.today { padding-bottom: clamp(44px, 7vh, 84px); }
	/* No fixes today means no map — the deck takes the whole width
	   rather than leaving a column of air where it would have been. */
	.tbody { display: grid; grid-template-columns: minmax(0, 1fr) 170px; gap: clamp(20px, 3vw, 34px); align-items: center; }
	.tbody.solo { grid-template-columns: minmax(0, 1fr); }
	.ground { display: flex; flex-direction: column; gap: 7px; }
	.glabel { font-size: 9.5px; letter-spacing: 0.04em; color: var(--color-foreground-subtle); }
	.gcanvas { height: 170px; }
	@media (max-width: 780px) {
		.tbody { grid-template-columns: 1fr; }
		.gcanvas { height: 140px; }
	}

	/* the rows behind a point — aligned to the plot, not the lane-name gutter. */
	.moment { margin-top: 22px; margin-left: 62px; max-width: 720px; }
	.mhead { display: flex; align-items: baseline; gap: 10px; margin-bottom: 10px; }
	.mhead .mt { font-size: 12px; color: var(--color-foreground); }
	.mlabel { font-family: var(--font-sans); font-size: 11.5px; color: var(--color-foreground-subtle); }
	.mclose { margin-left: auto; display: flex; align-items: center; background: none; border: 0; padding: 3px; border-radius: 5px; color: var(--color-foreground-subtle); cursor: pointer; }
	.mclose:hover { background: var(--hover-bg); color: var(--color-foreground); }
	.mlist { list-style: none; margin: 0; padding: 0; }
	.mlist li { display: flex; gap: 12px; align-items: baseline; padding: 4px 0; font-size: 13px; line-height: 1.45; }
	.mlist .rt { font-size: 10.5px; color: var(--color-foreground-subtle); flex: none; width: 38px; }
	.mlist .rk { font-size: 10px; color: var(--color-foreground-subtle); flex: none; width: 96px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.mlist .rb { font-family: var(--font-sans); color: var(--color-foreground); min-width: 0; }
	.mlist .rp { color: var(--color-foreground-muted); }
	.mnone { font-family: var(--font-sans); font-size: 12.5px; color: var(--color-foreground-subtle); margin: 0; }
	@media (max-width: 640px) { .moment { margin-left: 0; } }

	/* recents — the work you had in your hands last, newest first */
	.recents { max-width: 640px; }
	.line { display: flex; align-items: baseline; gap: 14px; padding: 8px 0; }
	.line .t { font-family: var(--font-serif); font-size: 16.5px; color: var(--color-foreground); min-width: 0; text-align: left; background: none; border: 0; padding: 0; cursor: pointer; }
	.line .t .s { margin-left: 0.34em; color: var(--color-foreground-subtle); }
	.line .t:hover { color: var(--color-primary); }
	.line .d { margin-left: auto; font-size: 11px; color: var(--color-foreground-subtle); white-space: nowrap; flex: none; }

	/* Home is what the bottom bar points at first, and every control on it sat
	   under Apple's 44pt floor — the day stepper 34px tall, these recents 25px.
	   Both grow by padding, so the type and the rhythm are unchanged and only
	   the hit area moves; the row's own padding moves into the button so the
	   spacing doesn't double. Keyed to touch as well as to width, because an
	   iPad in a wide layout has the same fingers.

	   Placed here, below the rules it overrides: these selectors match the base
	   ones exactly, so at equal specificity it is source order that decides,
	   and a media query does not change that. */
	@media (max-width: 768px), (pointer: coarse) {
		/* 13, not 12: the label measures 19.6px, so 12 lands at 43.6 — under the
		   floor by the kind of fraction that only a measurement catches. */
		.days button { padding: 13px 14px; }
		.line { padding: 0; }
		.line .t { padding: 10px 0; }
	}

	/* the keep */
	.keep { margin-top: clamp(48px, 8vh, 92px); max-width: 640px; }
	.card {
		background: var(--color-surface-elevated); border: 1px solid var(--color-border);
		border-radius: 14px; padding: clamp(18px, 3vw, 24px);
		transition: border-color 0.2s;
		/* The card is the field. Its border is the only affordance the question
		   needs, so no placeholder has to explain itself. */
		cursor: text;
	}
	.card:hover { border-color: var(--color-border-strong, var(--color-border)); }
	.card:focus-within { border-color: color-mix(in srgb, var(--color-primary) 45%, var(--color-border)); }
	.kq { font-family: var(--font-serif); font-size: 18px; font-weight: 400; line-height: 1.4; color: var(--color-foreground); margin: 0 0 12px; }
	.card textarea {
		display: block; width: 100%; resize: none; overflow-y: auto; min-height: 1.6em;
		font-family: var(--font-serif); font-size: 17px; line-height: 1.5;
		color: var(--color-foreground); background: none; border: 0; padding: 0;
	}
	.card textarea:focus { outline: none; }
	.krow { display: flex; align-items: center; gap: 12px; margin-top: 12px; }
	.khint { font-family: var(--font-mono); font-size: 10px; color: var(--color-foreground-subtle); }
	.ksave {
		margin-left: auto; flex: none; cursor: pointer;
		font-family: var(--font-sans); font-size: 12.5px; font-weight: 500;
		background: none; border: 0; padding: 0;
		color: var(--color-primary);
	}
	.ksave:hover:not(:disabled) { text-decoration: underline; text-underline-offset: 3px; }
	.ksave:disabled { color: var(--color-foreground-disabled); cursor: default; }
	.kerr { font-family: var(--font-sans); font-size: 12.5px; color: var(--color-error); margin: 12px 0 0; }
	.kept { list-style: none; margin: 20px 0 0; padding: 0; }
	.kept li { display: flex; gap: 14px; align-items: baseline; padding: 6px 0; }
	.kept .kt { font-size: 10.5px; color: var(--color-foreground-subtle); flex: none; width: 62px; }
	.kept .kb { font-family: var(--font-serif); font-size: 15.5px; line-height: 1.45; color: var(--color-foreground); }
	.kfoot { margin-top: 10px; }
</style>
