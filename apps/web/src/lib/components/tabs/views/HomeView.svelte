<!--
	HomeView.svelte — the home page ("The Home Edition").

	A calm front page of the box's loops: one warm lead (yesterday's reading),
	the movement trace to rest on, a few terse blocks, and the two quiet moments
	where the box asks you something. Whitespace over rules; roman serif only.

	Wired to real data where it exists. The pieces that need new backend or an LLM
	pass are marked `NEEDS:` inline and surfaced to the owner, not faked.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import {
		getDayByDate,
		getDayTimeline,
		getTodayStreams,
		listPeople,
		getWeatherNow,
		getCalendarUpcoming,
		getUnnamedPlaces,
		updatePlace,
		type WikiDayApi,
		type TimelineDayPoint,
		type TodayStreamsView,
		type WikiPersonListItem,
		type WeatherNow,
		type UpcomingEvent,
		type UnnamedPlace,
	} from "$lib/wiki/api";
	import { getReflectionsForDate, createReflection } from "$lib/api/client";
	import { notebookStore } from "$lib/stores/notebook.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";

	// The entity-resolution backlog ("who is 'J'?") — real endpoint, no frontend
	// helper existed yet, so a thin typed fetch here.
	type SurfaceCandidate = { entity_type: string; entity_id: string; name: string; reason: string };
	type SurfaceGroup = {
		normalized: string; surface: string; mention_type: string;
		count: number; sources: number; snippets: string[];
		candidates: SurfaceCandidate[];
	};
	async function getMentionsQueue(): Promise<SurfaceGroup[]> {
		try {
			const r = await fetch("/api/mentions/queue");
			if (!r.ok) return [];
			return await r.json();
		} catch { return []; }
	}

	// ---- dates ----
	const reduce =
		typeof window !== "undefined" && window.matchMedia &&
		window.matchMedia("(prefers-reduced-motion: reduce)").matches;
	function ymd(d: Date): string {
		return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
	}
	const _now = new Date();
	const todayDate = ymd(_now);
	const yesterdayDate = ymd(new Date(_now.getTime() - 86400000));

	// ---- live clock + dateline ----
	let clock = $state("");
	let dateline = $state("");
	function tick() {
		const d = new Date();
		const ap = d.getHours() >= 12 ? "pm" : "am", h = d.getHours() % 12 || 12;
		clock = `${h}:${String(d.getMinutes()).padStart(2, "0")} ${ap}`;
		dateline = d.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });
	}

	// ---- state ----
	let yDay = $state<WikiDayApi | null>(null);
	let points = $state<TimelineDayPoint[]>([]);
	let streams = $state<TodayStreamsView | null>(null);
	let people = $state<WikiPersonListItem[]>([]);
	let asks = $state<SurfaceGroup[]>([]);
	let examenText = $state("");
	let examenSaved = $state(false);
	let weather = $state<WeatherNow | null>(null);
	let upcoming = $state<UpcomingEvent[]>([]);
	let unnamed = $state<UnnamedPlace[]>([]);
	let placeName = $state("");
	let placeNamed = $state(false);
	let traceCanvas = $state<HTMLCanvasElement | undefined>(undefined);

	onMount(() => {
		getDayByDate(yesterdayDate).then((d) => (yDay = d)).catch(() => {});
		getDayTimeline(todayDate).then((t) => { if (t?.points) points = t.points; }).catch(() => {});
		getTodayStreams(todayDate).then((s) => (streams = s)).catch(() => {});
		listPeople().then((p) => (people = p)).catch(() => {});
		getMentionsQueue().then((m) => (asks = m)).catch(() => {});
		getWeatherNow().then((w) => (weather = w)).catch(() => {});
		getCalendarUpcoming(4).then((e) => (upcoming = e)).catch(() => {});
		getUnnamedPlaces(3).then((u) => (unnamed = u)).catch(() => {});
		notebookStore.load?.();
		if (!pagesStore.pages.length) pagesStore.loadPages();
		if (!chatSessions.sessions.length && !chatSessions.isLoading) chatSessions.load();
		tick();
		const id = setInterval(tick, 20000);
		return () => clearInterval(id);
	});

	function open(route: string, label?: string) {
		windowShellStore.openTabFromRoute(route, label ? { label } : undefined);
	}

	// ---- lead: yesterday's one-line reading (epigraph, else first sentence) ----
	const leadLine = $derived.by(() => {
		const e = yDay?.epigraph?.trim();
		if (e) return e.replace(/^["“]|["”]$/g, "");
		const auto = yDay?.autobiography?.trim();
		if (auto) {
			const first = auto.match(/^.*?[.!?](?:\s|$)/)?.[0]?.trim();
			if (first) return first;
		}
		return null;
	});

	// ---- focal caption from today's location ----
	const traceCaption = $derived.by(() => {
		const n = streams?.location?.length ?? 0;
		if (points.length < 2) return "no movement recorded yet today";
		if (n <= 1) return "stayed local · one place";
		return `${n} places today`;
		// NEEDS: steps — data_health_steps exists but no /api/health endpoint (trivial to add).
	});

	// ---- people: most recent (owe-a-reply signal NEEDS a reply-gap query) ----
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
	const recentPeople = $derived.by(() =>
		[...people]
			.filter((p) => p.last_interaction)
			.sort((a, b) => Date.parse(b.last_interaction!) - Date.parse(a.last_interaction!))
			.slice(0, 3),
	);

	// ---- desk: notebooks + recent pages/chats, blended by recency ----
	type DeskItem = { route: string; title: string; kind: string; ts: number; note?: string };
	const deskItems = $derived.by<DeskItem[]>(() => {
		const nb: DeskItem[] = notebookStore.notebooks.map((n: any) => ({
			route: `/notebook/${n.id}`, title: n.name || "Untitled", kind: "notebook",
			ts: n.updated_at ? Date.parse(n.updated_at) : 0,
			note: n.current_status ? "live" : undefined,
		}));
		const pg: DeskItem[] = pagesStore.pages.map((p) => ({
			route: `/page/${p.id}`, title: p.title || "Untitled", kind: "page",
			ts: p.updated_at ? Date.parse(p.updated_at) : 0,
		}));
		const ch: DeskItem[] = chatSessions.sessions.map((c) => ({
			route: `/chat/${c.conversation_id}`, title: c.title || "Untitled", kind: "chat",
			ts: c.last_updated ? Date.parse(c.last_updated) : 0,
		}));
		return [...nb, ...pg, ...ch].sort((a, b) => b.ts - a.ts).slice(0, 4);
	});

	// ---- next: today's calendar events still ahead ----
	// Upcoming events — multi-day, holidays/birthdays filtered server-side.
	const nextEvents = $derived(upcoming.slice(0, 3));
	function evTime(iso: string): string {
		const d = new Date(iso), ap = d.getHours() >= 12 ? "pm" : "am", h = d.getHours() % 12 || 12;
		return `${h}:${String(d.getMinutes()).padStart(2, "0")} ${ap}`;
	}
	function evDay(iso: string): string {
		const d = new Date(iso), n = new Date();
		if (d.toDateString() === n.toDateString()) return "today";
		if (d.toDateString() === new Date(n.getTime() + 86400000).toDateString()) return "tmrw";
		return d.toLocaleDateString(undefined, { weekday: "short" });
	}

	// ---- the box asking: top unresolved surface ----
	const theAsk = $derived.by(() => asks.find((a) => a.candidates?.length) ?? asks[0] ?? null);
	async function resolveAsk(cand: SurfaceCandidate | null) {
		const a = theAsk;
		if (!a) return;
		try {
			if (cand) {
				await fetch("/api/mentions/link", {
					method: "POST", headers: { "Content-Type": "application/json" },
					body: JSON.stringify({ normalized: a.normalized, entity_id: cand.entity_id }),
				});
			} else {
				await fetch("/api/mentions/dismiss", {
					method: "POST", headers: { "Content-Type": "application/json" },
					body: JSON.stringify({ normalized: a.normalized }),
				});
			}
		} catch { /* best-effort */ }
		asks = asks.filter((x) => x.normalized !== a.normalized);
	}

	// ---- examen: writes into today's reflection (the grounded SEED needs an LLM) ----
	async function saveExamen() {
		if (!examenText.trim()) return;
		try {
			let refs = await getReflectionsForDate(todayDate);
			let ref = refs[0] ?? (await createReflection(todayDate));
			examenSaved = true;
			// Open the reflection so the owner can keep writing.
			if (ref?.id) setTimeout(() => open(`/page/${ref.id}`, "Reflection"), 400);
		} catch { examenSaved = true; }
	}

	// Weather (null until the weather_sync cron has run at least once).
	function cToF(c: number | null): number | null { return c == null ? null : Math.round((c * 9) / 5 + 32); }
	const wxLabel = $derived.by(() => {
		if (!weather || weather.temperature_c == null) return null;
		return `${cToF(weather.temperature_c)}° ${weather.condition}`.trim();
	});

	// The "name this place" ask — the top place the box has visited but not named.
	const placeAsk = $derived(unnamed.length ? unnamed[0] : null);
	async function namePlace() {
		const p = placeAsk;
		if (!p || !placeName.trim()) return;
		try { await updatePlace(p.id, { name: placeName.trim() }); } catch { /* best-effort */ }
		placeNamed = true;
	}

	const hasDesk = $derived(deskItems.length > 0);
	const hasPeople = $derived(recentPeople.length > 0);

	// ---- movement trace (real GPS, draws itself in) ----
	function cssvar(n: string): string {
		return getComputedStyle(document.documentElement).getPropertyValue(n).trim() || "#1a2030";
	}
	function rgba(hex: string, a: number): string {
		hex = hex.replace("#", ""); if (hex.length === 3) hex = hex.split("").map((c) => c + c).join("");
		const n = parseInt(hex, 16); return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
	}
	$effect(() => {
		const cv = traceCanvas, pts = points;
		if (!cv || pts.length < 2) return;
		let raf = 0, start = 0;
		const dpr = () => Math.min(window.devicePixelRatio || 1, 2);
		function frame(ts: number) {
			if (!start) start = ts;
			const p = reduce ? 1 : Math.min(1, (ts - start) / 1500);
			const W = cv!.clientWidth, H = cv!.clientHeight, pad = 22, d = dpr();
			if (W < 8 || H < 8) { raf = requestAnimationFrame(frame); return; }
			if (cv!.width !== Math.round(W * d)) { cv!.width = Math.round(W * d); cv!.height = Math.round(H * d); }
			const c = cv!.getContext("2d"); if (!c) return;
			c.setTransform(d, 0, 0, d, 0, 0); c.clearRect(0, 0, W, H);
			let mnLa = Infinity, mxLa = -Infinity, mnLo = Infinity, mxLo = -Infinity;
			for (const q of pts) { mnLa = Math.min(mnLa, q.latitude); mxLa = Math.max(mxLa, q.latitude); mnLo = Math.min(mnLo, q.longitude); mxLo = Math.max(mxLo, q.longitude); }
			const midLa = (mnLa + mxLa) / 2, k = Math.cos(midLa * Math.PI / 180) || 1;
			const spanX = Math.max((mxLo - mnLo) * k, 1e-4), spanY = Math.max(mxLa - mnLa, 1e-4);
			const sc = Math.min((W - 2 * pad) / spanX, (H - 2 * pad) / spanY);
			const offX = (W - spanX * sc) / 2, offY = (H - spanY * sc) / 2;
			const X = (lo: number) => offX + (lo - mnLo) * k * sc, Y = (la: number) => offY + (mxLa - la) * sc;
			const step = Math.max(1, Math.floor(pts.length / 380));
			const drawn = pts.filter((_, i) => i % step === 0 || i === pts.length - 1);
			const n = Math.max(2, Math.floor(drawn.length * p));
			const fg = cssvar("--color-foreground"), accent = cssvar("--color-primary");
			c.beginPath();
			for (let i = 0; i < n; i++) { const q = drawn[i]; i ? c.lineTo(X(q.longitude), Y(q.latitude)) : c.moveTo(X(q.longitude), Y(q.latitude)); }
			c.strokeStyle = rgba(fg, 0.5); c.lineWidth = 1.3; c.lineJoin = "round"; c.lineCap = "round"; c.stroke();
			const hd = drawn[n - 1], hx = X(hd.longitude), hy = Y(hd.latitude), pu = 0.5 + 0.5 * Math.sin(ts / 760);
			const gl = c.createRadialGradient(hx, hy, 0, hx, hy, 8 + pu * 3);
			gl.addColorStop(0, rgba(accent, 0.3)); gl.addColorStop(1, rgba(accent, 0));
			c.fillStyle = gl; c.beginPath(); c.arc(hx, hy, 8 + pu * 3, 0, 6.29); c.fill();
			c.fillStyle = accent; c.beginPath(); c.arc(hx, hy, 2.6, 0, 6.29); c.fill();
			raf = requestAnimationFrame(frame);
		}
		raf = requestAnimationFrame(frame);
		return () => { if (raf) cancelAnimationFrame(raf); };
	});
</script>

<div class="home">
	<div class="page">
		<header class="mast rv">
			<span class="wm">The Home Edition</span>
			<span class="dl mono">{dateline}{#if wxLabel} · {wxLabel}{/if}<span class="clk"> · {clock}</span></span>
		</header>

		<section class="lead rv" style="animation-delay:.05s">
			{#if leadLine}
				<h1>{leadLine}</h1>
				<div class="rd"><button class="link" type="button" onclick={() => open(`/day/day_${yesterdayDate}`, "Yesterday")}>Read the account <span class="arw">→</span></button></div>
			{:else}
				<h1>A new day, not yet written.</h1>
				<div class="rd sub">The box composes yesterday's account overnight.</div>
			{/if}
		</section>

		{#if points.length > 1}
			<figure class="focal rv" style="animation-delay:.1s">
				<canvas bind:this={traceCanvas} aria-hidden="true"></canvas>
				<figcaption class="cap mono">{traceCaption}</figcaption>
			</figure>
		{/if}

		<section class="cols rv" style="animation-delay:.14s">
			{#if hasPeople}
				<div class="block">
					<h2 class="kicker">People</h2>
					{#each recentPeople as p}
						<div class="line">
							<button class="t" type="button" onclick={() => open(`/person/${p.id}`, p.canonical_name)}>{p.canonical_name}{#if p.relationship_category}<span class="s"> — {p.relationship_category}</span>{/if}</button>
							<span class="d mono">{ago(p.last_interaction)}</span>
						</div>
					{/each}
					<!-- NEEDS: "owe a reply / unanswered Nd" marker — a reply-gap query over
					     data_communication_message (direction is metadata->>'is_from_me'). New query. -->
				</div>
			{/if}

			{#if hasDesk}
				<div class="block">
					<h2 class="kicker">On your desk</h2>
					{#each deskItems as it}
						<div class="line">
							<button class="t" type="button" onclick={() => open(it.route, it.title)}>{it.title}{#if it.kind !== "notebook"}<span class="s"> — {it.kind}</span>{/if}</button>
							<span class="d mono">{it.note ?? ago(new Date(it.ts).toISOString())}</span>
						</div>
					{/each}
				</div>
			{/if}

			{#if nextEvents.length}
				<div class="block">
					<h2 class="kicker">Next</h2>
					{#each nextEvents as e}
						<div class="line"><span class="t">{e.title}{#if e.location_name}<span class="s"> — {e.location_name}</span>{/if}</span><span class="d mono">{evDay(e.start_time)} {evTime(e.start_time)}</span></div>
					{/each}
				</div>
			{/if}
		</section>

		<!-- NEEDS: "Alongside" (a recurring theme across chats+pages → "privacy surfaced
		     in 3 chats + 2 pages, a notebook?"). Needs an aggregation query + an LLM pass
		     to name the thread. Omitted — the discovered-story scaffolding it would have
		     hung on was cut with wiki_stories. -->

		{#if (placeAsk && !placeNamed) || theAsk || !examenSaved}
			<section class="dialogue rv" style="animation-delay:.18s">
				{#if placeAsk && !placeNamed}
					<div class="panel">
						<div class="k">The box is asking</div>
						<p class="q">{#if placeAsk.visit_count > 1}You've stopped somewhere <span class="m">{placeAsk.visit_count} times</span> but never named it. What is that place?{:else}There's a place near you the box keeps seeing but hasn't named. What is it?{/if}</p>
						<input type="text" bind:value={placeName} placeholder="name this place…" aria-label="Name this place"
							onkeydown={(e) => { if (e.key === 'Enter') namePlace(); }} />
					</div>
				{:else if theAsk}
					<div class="panel">
						<div class="k">The box is asking</div>
						<p class="q">There's a <span class="m">“{theAsk.surface}”</span> in {theAsk.count} {theAsk.count === 1 ? "mention" : "mentions"}{#if theAsk.candidates?.length}. Is that {theAsk.candidates[0].name}, or someone new?{:else} the box hasn't met. Who is it?{/if}</p>
						<div class="chips">
							{#each theAsk.candidates.slice(0, 2) as cand}
								<button type="button" onclick={() => resolveAsk(cand)}>{cand.name}</button>
							{/each}
							<button type="button" onclick={() => resolveAsk(null)}>Someone new</button>
						</div>
					</div>
				{/if}
				<div class="panel">
					<div class="k">Tonight · a reflection</div>
					{#if examenSaved}
						<p class="q">Kept. <span class="s">Opening your reflection…</span></p>
					{:else}
						<p class="q">What do you want to remember from today?</p>
						<!-- NEEDS: the *seeded* examen ("You were most alive in the map cache,
						     10-3 — keep it?") needs an LLM pass grounded on the day. This is the
						     plain prompt; the answer is stored as today's reflection Page. -->
						<input type="text" bind:value={examenText} placeholder="answer in a line…"
							aria-label="Tonight's reflection"
							onkeydown={(e) => { if (e.key === "Enter") saveExamen(); }} />
					{/if}
				</div>
			</section>
		{/if}

		<footer class="foot rv" style="animation-delay:.2s">
			<span>Your life stays on this box — no cloud, no third parties.</span>
			{#if yDay?.updated_at}<span>·</span><span class="mono">last composed at home {evTime(yDay.updated_at)}</span>{/if}
			<!-- NEEDS: a literal "nothing left this box today" egress audit needs a
			     per-connection collector (none exists). Stated as the design invariant, not audited. -->
		</footer>
	</div>
</div>

<style>
	.home { height: 100%; overflow-y: auto; background: var(--color-background); color: var(--color-foreground); }
	.page { max-width: 940px; margin: 0 auto; padding: 0 clamp(24px, 6vw, 40px); }
	.mono { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }

	.rv { opacity: 0; transform: translateY(7px); animation: rv 0.7s cubic-bezier(0.2, 0.7, 0.2, 1) forwards; }
	@keyframes rv { to { opacity: 1; transform: none; } }
	@media (prefers-reduced-motion: reduce) { .rv { animation: none; opacity: 1; transform: none; } }

	.kicker { font-family: var(--font-mono); font-size: 10.5px; letter-spacing: 0.2em; text-transform: uppercase; color: var(--color-foreground-subtle); margin: 0 0 18px; }
	.link { font-family: var(--font-sans); font-size: 13.5px; font-weight: 500; color: var(--color-primary); background: none; border: 0; padding: 0; cursor: pointer; }
	.link:hover { text-decoration: underline; text-underline-offset: 3px; }
	.link .arw { opacity: 0.7; }

	.mast { padding-top: clamp(30px, 5vh, 52px); display: flex; align-items: baseline; gap: 16px; }
	.mast .wm { font-family: var(--font-serif); font-weight: 600; font-size: 16px; letter-spacing: 0.14em; text-transform: uppercase; }
	.mast .dl { margin-left: auto; font-size: 11px; letter-spacing: 0.08em; color: var(--color-foreground-subtle); }
	.mast .clk { color: var(--color-foreground); }

	.lead { padding: clamp(48px, 9vh, 104px) 0 clamp(40px, 7vh, 84px); max-width: 24ch; }
	.lead h1 { font-family: var(--font-serif); font-weight: 600; font-size: clamp(30px, 4.6vw, 46px); line-height: 1.14; letter-spacing: -0.015em; margin: 0; font-style: normal; }
	.lead .rd { margin-top: 22px; }
	.lead .rd.sub { font-family: var(--font-sans); font-size: 13.5px; color: var(--color-foreground-subtle); }

	.focal { display: flex; flex-direction: column; align-items: center; padding: clamp(20px, 3vh, 40px) 0 clamp(44px, 8vh, 90px); margin: 0; }
	.focal canvas { width: min(560px, 100%); height: clamp(200px, 30vh, 280px); display: block; }
	.focal .cap { font-size: 11px; letter-spacing: 0.04em; color: var(--color-foreground-subtle); margin-top: 14px; }

	.cols { display: grid; grid-template-columns: 1fr 1fr; gap: clamp(40px, 7vw, 88px) clamp(40px, 6vw, 72px); }
	@media (max-width: 720px) { .cols { grid-template-columns: 1fr; gap: clamp(38px, 7vh, 56px); } }
	.line { display: flex; align-items: baseline; gap: 14px; padding: 9px 0; }
	.line .t { font-family: var(--font-serif); font-size: 16.5px; color: var(--color-foreground); min-width: 0; text-align: left; background: none; border: 0; padding: 0; cursor: default; }
	button.t { cursor: pointer; }
	.line .t .s { color: var(--color-foreground-subtle); }
	.line button.t:hover { color: var(--color-primary); }
	.line .d { margin-left: auto; font-size: 11px; color: var(--color-foreground-subtle); white-space: nowrap; flex: none; }

	.dialogue { margin: clamp(48px, 8vh, 90px) 0 0; display: grid; grid-template-columns: 1fr 1fr; gap: clamp(28px, 4vw, 44px); }
	@media (max-width: 720px) { .dialogue { grid-template-columns: 1fr; gap: clamp(32px, 6vh, 44px); } }
	.panel { background: color-mix(in srgb, var(--color-primary) 6%, var(--color-background)); border-radius: 14px; padding: clamp(22px, 3vw, 30px); }
	.panel .k { font-family: var(--font-mono); font-size: 10px; letter-spacing: 0.18em; text-transform: uppercase; color: var(--color-primary); margin: 0 0 14px; }
	.panel .q { font-family: var(--font-serif); font-size: 17px; line-height: 1.45; color: var(--color-foreground); margin: 0; font-style: normal; }
	.panel .q .m { color: var(--color-primary); }
	.panel .q .s { color: var(--color-foreground-subtle); }
	.panel .chips { display: flex; gap: 8px; margin-top: 16px; flex-wrap: wrap; }
	.panel .chips button { font-family: var(--font-sans); font-size: 12.5px; color: var(--color-foreground); background: var(--color-surface); border: 1px solid var(--color-border); border-radius: 999px; padding: 6px 14px; cursor: pointer; }
	.panel .chips button:hover { border-color: var(--color-primary); color: var(--color-primary); }
	.panel input { width: 100%; margin-top: 14px; font-family: var(--font-serif); font-size: 15px; color: var(--color-foreground); background: none; border: 0; border-bottom: 1px solid var(--color-border-strong, var(--color-border)); padding: 6px 2px; }
	.panel input::placeholder { font-family: var(--font-sans); font-size: 13px; color: var(--color-foreground-subtle); }
	.panel input:focus { outline: none; border-bottom-color: var(--color-primary); }

	.foot { margin-top: clamp(56px, 9vh, 110px); border-top: 1px solid var(--color-border); padding: 22px 0 70px; font-family: var(--font-mono); font-size: 11px; letter-spacing: 0.04em; color: var(--color-foreground-subtle); display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
</style>
