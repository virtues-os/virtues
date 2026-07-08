<script lang="ts">
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { askVirtues } from "$lib/stores/pendingPrompt.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { chatSessions } from "$lib/stores/chatSessions.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";

	// HomeViewCockpit — "mission control for your life" (stark-terminal motif).
	// A thin command/status line pins the top; an AI BRIEF leads; data panels sit
	// below in a dense, glanceable grid. Doctrine: oriented first (Brief + Needs
	// you), curious second (Noticed). The panels are a config array — the seam
	// where a user's own arrangement would plug in later. Monospace-forward, dot
	// leaders, status dots: the app's CLI vocabulary, rendered.
	//
	// Everything except "Jump back in" (real recents) is HARDCODED until the
	// brief / source / salience endpoints exist.

	let input = $state("");
	const now = new Date();
	const clock = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;

	function open(route: string, title: string) {
		windowShellStore.openTabFromRoute(route, { label: title });
	}
	function submit() {
		const v = input.trim();
		if (!v) return;
		askVirtues(v);
		input = "";
	}

	// --- HARDCODED cockpit data -------------------------------------------
	const brief = [
		{ t: "Busy Monday — " },
		{ t: "3 meetings", link: "/day" },
		{ t: ", dentist at 2. " },
		{ t: "Sarah", link: "/day" },
		{ t: " replied about the trip; " },
		{ t: "2 invoices", link: "/day" },
		{ t: " need you. You've been quiet with " },
		{ t: "Mom", link: "/day" },
		{ t: " this week." },
	];
	const needsYou = [
		{ label: "Reply to Sarah", tag: "mail · 2d" },
		{ label: "Approve invoice #2831", tag: "finance" },
		{ label: "3 promises unkept", tag: "journal" },
		{ label: "Renew domain", tag: "2d left" },
	];
	const nowNext = [
		{ time: "09:00", title: "Standup", note: "in 46m" },
		{ time: "14:00", title: "Dentist", note: "" },
		{ time: "", title: "free after 15:30", note: "", dim: true },
	];
	const streams = [
		{ label: "Mail", value: "12", meta: "3 to you" },
		{ label: "Messages", value: "8", meta: "" },
		{ label: "Health", value: "6h20m", meta: "8.2k steps" },
		{ label: "Places", value: "Home → Office", meta: "" },
	];
	const noticed = [
		"First lunch with Sarah in 3 months.",
		"You've written “tired” 4× this week.",
		"Unusual: 5 calendar changes today.",
	];
	// ----------------------------------------------------------------------

	type Recent = { route: string; title: string; ts: number };
	const recents = $derived.by<Recent[]>(() => {
		const chats: Recent[] = chatSessions.sessions.map((c) => ({
			route: `/chat/${c.conversation_id}`,
			title: c.title || "Untitled",
			ts: c.last_updated ? Date.parse(c.last_updated) : 0,
		}));
		const pages: Recent[] = pagesStore.pages.map((p) => ({
			route: `/page/${p.id}`,
			title: p.title || "Untitled",
			ts: p.updated_at ? Date.parse(p.updated_at) : 0,
		}));
		return [...chats, ...pages].sort((a, b) => b.ts - a.ts).slice(0, 5);
	});

	onMount(() => {
		if (chatSessions.sessions.length === 0 && !chatSessions.isLoading)
			chatSessions.load();
		if (pagesStore.pages.length === 0 && !pagesStore.pagesLoading)
			pagesStore.loadPages();
	});
</script>

<div class="cockpit">
	<!-- Command / status line -->
	<div class="cmdbar">
		<span class="prompt">&gt;</span>
		<input
			class="cmd"
			bind:value={input}
			placeholder="run an action, or ask…"
			spellcheck="false"
			autocomplete="off"
			onkeydown={(e) => e.key === "Enter" && submit()}
		/>
		<kbd class="kbd">⌘K</kbd>
		<span class="status">
			<span class="dot"></span>
			box live · synced {clock} · 5 sources
		</span>
	</div>

	<div class="grid">
		<!-- BRIEF — the AI orientation, spans full width -->
		<section class="panel brief">
			<div class="phead">
				<span class="ptitle">Brief</span>
				<span class="pmeta">this morning</span>
			</div>
			<p class="brief-text">
				{#each brief as seg (seg.t)}{#if seg.link}<button
							class="ent"
							onclick={() => open(seg.link, "Today")}>{seg.t}</button
						>{:else}{seg.t}{/if}{/each}
			</p>
		</section>

		<!-- NEEDS YOU -->
		<section class="panel">
			<div class="phead">
				<span class="ptitle">Needs you</span>
				<span class="count">{needsYou.length}</span>
			</div>
			<ul class="rows">
				{#each needsYou as n (n.label)}
					<li>
						<button class="row act">
							<span class="caret">↳</span>
							<span class="rlabel">{n.label}</span>
							<span class="leader"></span>
							<span class="rtag">{n.tag}</span>
						</button>
					</li>
				{/each}
			</ul>
		</section>

		<!-- NOW · NEXT -->
		<section class="panel">
			<div class="phead">
				<span class="ptitle">Now · Next</span>
				<button class="pmeta link" onclick={() => open("/day", "Today")}
					>open day</button
				>
			</div>
			<ul class="rows">
				{#each nowNext as e (e.title)}
					<li>
						<div class="row" class:dim={e.dim}>
							<span class="etime">{e.time}</span>
							<span class="etitle">{e.title}</span>
							<span class="leader"></span>
							<span class="enote">{e.note}</span>
						</div>
					</li>
				{/each}
			</ul>
		</section>

		<!-- STREAMS -->
		<section class="panel">
			<div class="phead">
				<span class="ptitle">Streams</span>
				<span class="pmeta">today</span>
			</div>
			<ul class="rows">
				{#each streams as s (s.label)}
					<li>
						<div class="row">
							<span class="slabel">{s.label}</span>
							<span class="leader"></span>
							<span class="svalue">{s.value}</span>
							{#if s.meta}<span class="smeta">· {s.meta}</span>{/if}
						</div>
					</li>
				{/each}
			</ul>
		</section>

		<!-- NOTICED — the curiosity panel -->
		<section class="panel noticed">
			<div class="phead">
				<span class="ptitle">Noticed</span>
				<Icon icon="ri:sparkling-2-line" width="13" class="spark" />
			</div>
			<ul class="rows">
				{#each noticed as n (n)}
					<li><div class="row notice">{n}</div></li>
				{/each}
			</ul>
		</section>

		<!-- JUMP BACK IN — real recents, spans full width -->
		<section class="panel jump">
			<div class="phead">
				<span class="ptitle">Jump back in</span>
			</div>
			<div class="chips">
				{#if recents.length > 0}
					{#each recents as r (r.route)}
						<button class="chip" onclick={() => open(r.route, r.title)}
							>{r.title}</button
						>
					{/each}
				{:else}
					<span class="pmeta">Nothing yet.</span>
				{/if}
			</div>
		</section>
	</div>
</div>

<style>
	.cockpit {
		height: 100%;
		width: 100%;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		font-family: var(--font-mono);
		background: var(--color-background);
	}

	/* Command / status ------------------------------------------------------ */
	.cmdbar {
		position: sticky;
		top: 0;
		z-index: 10;
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.75rem 1.25rem;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-background);
	}
	.prompt {
		color: var(--color-primary);
		font-weight: 600;
	}
	.cmd {
		flex: 1;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		color: var(--color-foreground);
		font-family: var(--font-mono);
		font-size: 0.8125rem;
	}
	.cmd::placeholder {
		color: var(--color-foreground-subtle);
	}
	.kbd {
		font-family: var(--font-mono);
		font-size: 0.625rem;
		color: var(--color-foreground-subtle);
		border: 1px solid var(--color-border);
		border-radius: 4px;
		padding: 0.0625rem 0.3125rem;
	}
	.status {
		display: flex;
		align-items: center;
		gap: 0.4375rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-success);
		box-shadow: 0 0 0 0 var(--color-success);
		animation: pulse 2.4s ease-out infinite;
	}
	@keyframes pulse {
		0% {
			box-shadow: 0 0 0 0 color-mix(in srgb, var(--color-success) 50%, transparent);
		}
		70%,
		100% {
			box-shadow: 0 0 0 5px transparent;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.dot {
			animation: none;
		}
	}

	/* Grid ------------------------------------------------------------------ */
	.grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1px;
		background: var(--color-border);
		border-bottom: 1px solid var(--color-border);
	}
	.panel {
		background: var(--color-background);
		padding: 1rem 1.25rem 1.125rem;
		min-width: 0;
	}
	.brief,
	.jump {
		grid-column: 1 / -1;
	}
	@media (max-width: 46rem) {
		.grid {
			grid-template-columns: 1fr;
		}
	}

	.phead {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin-bottom: 0.75rem;
	}
	.ptitle {
		font-size: 0.6875rem;
		text-transform: uppercase;
		letter-spacing: 0.16em;
		color: var(--color-foreground);
		font-weight: 500;
	}
	.pmeta {
		font-size: 0.625rem;
		color: var(--color-foreground-subtle);
		margin-left: auto;
	}
	.pmeta.link {
		background: transparent;
		border: none;
		cursor: pointer;
		font-family: var(--font-mono);
	}
	.pmeta.link:hover {
		color: var(--color-foreground);
	}
	.count {
		margin-left: auto;
		font-size: 0.6875rem;
		color: var(--color-background);
		background: var(--color-foreground);
		border-radius: 999px;
		min-width: 1.125rem;
		height: 1.125rem;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0 0.25rem;
	}
	.phead :global(.spark) {
		color: var(--color-primary);
		margin-left: auto;
	}

	/* Rows ------------------------------------------------------------------ */
	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.4375rem;
	}
	.row {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		width: 100%;
		font-size: 0.8125rem;
		color: var(--color-foreground);
		text-align: left;
	}
	.row.act {
		background: transparent;
		border: none;
		padding: 0.125rem 0;
		cursor: pointer;
	}
	.row.act:hover .rlabel {
		color: var(--color-primary);
	}
	.row.dim {
		color: var(--color-foreground-subtle);
	}
	.caret {
		color: var(--color-foreground-subtle);
		flex-shrink: 0;
	}
	.rlabel {
		flex-shrink: 0;
		transition: color var(--duration-fast) ease;
	}
	.leader {
		flex: 1;
		border-bottom: 1px dotted var(--color-border-strong);
		transform: translateY(-0.2em);
		min-width: 1rem;
	}
	.rtag,
	.enote,
	.smeta {
		flex-shrink: 0;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}
	.etime {
		flex-shrink: 0;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.etitle {
		flex-shrink: 0;
	}
	.slabel {
		flex-shrink: 0;
		color: var(--color-foreground-muted);
	}
	.svalue {
		flex-shrink: 0;
		color: var(--color-foreground);
	}

	/* Brief ----------------------------------------------------------------- */
	.brief-text {
		margin: 0;
		font-size: 0.9375rem;
		line-height: 1.6;
		color: var(--color-foreground);
		max-width: 54rem;
	}
	.ent {
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		font: inherit;
		color: var(--color-foreground);
		border-bottom: 1px solid var(--color-border-strong);
		transition:
			color var(--duration-fast) ease,
			border-color var(--duration-fast) ease;
	}
	.ent:hover {
		color: var(--color-primary);
		border-color: var(--color-primary);
	}

	/* Noticed --------------------------------------------------------------- */
	.notice {
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	/* Jump back in ---------------------------------------------------------- */
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4375rem;
	}
	.chip {
		max-width: 16rem;
		padding: 0.3125rem 0.625rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		font-family: var(--font-mono);
		font-size: 0.75rem;
		cursor: pointer;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		transition:
			color var(--duration-fast) ease,
			border-color var(--duration-fast) ease;
	}
	.chip:hover {
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
	}
</style>
