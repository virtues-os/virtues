<!--
	Chats. Bring what the person has already talked through with other AI
	assistants, so theirs starts out knowing them.

	One row per site, each in its own state:

	  - ChatGPT and Claude send an export by email, sometimes days later.
	    "Request export" opens the site's own export setting; the row then
	    says it was asked for, on this device. When the zip arrives it is
	    dropped anywhere on the page or chosen from the row.
	  - A site with chats on the server says the span of months they cover,
	    with a dot for each month that has any, darker where there were more.

	Gemini is left out until the importer has been checked against a real
	Google Takeout export: today a Takeout file would fail to import.

	Every import, from a row or a drop, shows its progress and result in one
	line under the rows. Several files go in one after another; the server
	reads one export at a time. The step is optional: Continue once anything is in
	or asked for, Skip for now otherwise. Both record that this device moved
	past it (`setup.passChats`).
-->
<script lang="ts">
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { type ChatHistory } from "$lib/api/client";
	import { importChatExport, isAbort, type ImportPhase } from "$lib/chat-import";
	import { openExternal } from "$lib/tauri/bridge";
	import { isIOS, isPhoneBrowser } from "$lib/utils/platform";
	import { setup, intoLabel } from "../setup.svelte";
	import StepFrame from "../StepFrame.svelte";

	let { onnext }: { onnext: () => void } = $props();

	/** A phone has nothing to drag a file from, so it is only offered the picker. */
	const canDrop = !isIOS && !isPhoneBrowser;

	type ExportSite = "chatgpt" | "claude";
	const SITES: { id: ExportSite; name: string; url: string; wait: string }[] = [
		{
			id: "chatgpt",
			name: "ChatGPT",
			url: "https://chatgpt.com/#settings/DataControls",
			wait: "Your export arrives by email, sometimes days later.",
		},
		{
			id: "claude",
			name: "Claude",
			url: "https://claude.ai/settings/data-privacy-controls",
			wait: "Your export arrives by email.",
		},
	];

	// ── asked for, on this device ─────────────────────────────────────────
	/** When an export was asked for, per site. A per-device convenience:
	 *  losing it only shows "Request export" again. */
	const ASKED_KEY = "virtues-setup-export-asked";
	function readAsked(): Partial<Record<ExportSite, string>> {
		try {
			return JSON.parse(localStorage.getItem(ASKED_KEY) ?? "{}") ?? {};
		} catch {
			return {};
		}
	}
	let asked = $state<Partial<Record<ExportSite, string>>>(readAsked());

	async function requestExport(site: (typeof SITES)[number]) {
		asked = { ...asked, [site.id]: new Date().toISOString() };
		try {
			localStorage.setItem(ASKED_KEY, JSON.stringify(asked));
		} catch {
			/* the row forgets on the next visit; the export is still coming */
		}
		await openExternal(site.url);
	}

	function whenAsked(iso: string): string {
		const then = new Date(iso);
		const days = Math.round((startOfDay(new Date()) - startOfDay(then)) / 86_400_000);
		if (days <= 0) return "today";
		if (days === 1) return "yesterday";
		return `on ${then.toLocaleDateString(undefined, { month: "long", day: "numeric" })}`;
	}
	function startOfDay(d: Date): number {
		return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
	}

	// ── what has arrived ──────────────────────────────────────────────────
	function historyOf(provider: string): ChatHistory | null {
		return setup.chats.find((h) => h.provider === provider) ?? null;
	}

	function monthName(ym: string): string {
		const [y, m] = ym.split("-").map(Number);
		return new Date(Date.UTC(y, m - 1, 1)).toLocaleDateString(undefined, {
			month: "long",
			year: "numeric",
			timeZone: "UTC",
		});
	}

	/** "Your chats from March 2023 to November 2026 are in." */
	function spanLine(h: ChatHistory): string {
		const first = h.months[0].month;
		const last = h.months[h.months.length - 1].month;
		return first === last
			? `Your chats from ${monthName(first)} are in.`
			: `Your chats from ${monthName(first)} to ${monthName(last)} are in.`;
	}

	function monthIndex(ym: string): number {
		const [y, m] = ym.split("-").map(Number);
		return y * 12 + (m - 1);
	}

	/** A dot per month with chats, placed along the span, darker where there
	 *  were more. */
	function dotsOf(h: ChatHistory): { left: number; strength: number }[] {
		const start = monthIndex(h.months[0].month);
		const span = Math.max(1, monthIndex(h.months[h.months.length - 1].month) - start);
		const most = Math.max(...h.months.map((m) => m.messages));
		return h.months.map((m) => ({
			left: h.months.length === 1 ? 50 : ((monthIndex(m.month) - start) / span) * 100,
			strength: 0.3 + 0.7 * Math.sqrt(m.messages / most),
		}));
	}

	// ── importing ─────────────────────────────────────────────────────────
	let phase = $state<ImportPhase | null>(null);
	let result = $state<string | null>(null);
	let error = $state<string | null>(null);
	let dragging = $state(false);
	let picker: HTMLInputElement | undefined = $state();
	const stop = new AbortController();

	/** Bring in each file in turn: the server reads one export at a time. */
	async function bringIn(files: File[]) {
		if (phase) {
			error = "One export at a time. Add this one when the current one finishes.";
			return;
		}
		result = null;
		error = null;
		const said: string[] = [];
		try {
			for (const file of files) {
				said.push(await importChatExport(file, { onPhase: (p) => (phase = p), signal: stop.signal }));
				await setup.refresh();
			}
			result = said.join(" ");
		} catch (e) {
			if (isAbort(e)) return;
			result = said.length ? said.join(" ") : null;
			error = e instanceof Error ? e.message : "The upload failed. Try again.";
		} finally {
			phase = null;
		}
	}

	function onPick(e: Event) {
		const input = e.target as HTMLInputElement;
		const files = [...(input.files ?? [])];
		input.value = "";
		if (files.length) void bringIn(files);
	}

	/** The files in a drop, or a sentence saying why there are none to use.
	 *  Safari unzips downloads by default, so a dropped folder is likely. */
	function droppedFiles(data: DataTransfer): File[] | string {
		const items = [...data.items].filter((i) => i.kind === "file");
		if (items.some((i) => i.webkitGetAsEntry?.()?.isDirectory)) {
			return "That is a folder. Drop the conversations.json inside it, or the zip itself.";
		}
		return [...data.files];
	}

	onMount(() => {
		// The whole page takes a dropped export. `dragging` counts on the
		// window so moving between children doesn't flicker the line.
		let depth = 0;
		const hasFiles = (e: DragEvent) => !!e.dataTransfer?.types.includes("Files");
		const enter = (e: DragEvent) => {
			if (!hasFiles(e)) return;
			depth++;
			dragging = true;
		};
		const leave = (e: DragEvent) => {
			if (!hasFiles(e)) return;
			depth = Math.max(0, depth - 1);
			if (!depth) dragging = false;
		};
		const over = (e: DragEvent) => {
			if (hasFiles(e)) e.preventDefault();
		};
		const drop = (e: DragEvent) => {
			if (!hasFiles(e)) return;
			e.preventDefault();
			depth = 0;
			dragging = false;
			if (!e.dataTransfer) return;
			const files = droppedFiles(e.dataTransfer);
			if (typeof files === "string") error = files;
			else if (files.length) void bringIn(files);
		};
		window.addEventListener("dragenter", enter);
		window.addEventListener("dragleave", leave);
		window.addEventListener("dragover", over);
		window.addEventListener("drop", drop);
		return () => {
			stop.abort();
			window.removeEventListener("dragenter", enter);
			window.removeEventListener("dragleave", leave);
			window.removeEventListener("dragover", over);
			window.removeEventListener("drop", drop);
		};
	});

	// ── moving on ─────────────────────────────────────────────────────────
	const anything = $derived(setup.chats.length > 0 || Object.keys(asked).length > 0);

	function moveOn() {
		setup.passChats();
		onnext();
	}
</script>

{#snippet dots(h: ChatHistory)}
	<div class="span" role="img" aria-label={spanLine(h)}>
		<span class="year">{h.months[0].month.slice(0, 4)}</span>
		<span class="line">
			{#each dotsOf(h) as d, i (i)}
				<span class="dot" style:left="{d.left}%" style:opacity={d.strength}></span>
			{/each}
		</span>
		<span class="year">{h.months[h.months.length - 1].month.slice(0, 4)}</span>
	</div>
{/snippet}

<StepFrame
	title="Bring your other AI chats"
	subtitle="What you have already talked through with other assistants, so yours starts out knowing you."
>
	<div class="rows">
		{#each SITES as site (site.id)}
			{@const h = historyOf(site.id)}
			<article class="row">
				<div class="head">
					<span class="glyph" aria-hidden="true">{site.name[0]}</span>
					<div class="text">
						<h2>{site.name}</h2>
						{#if h}
							<p>{spanLine(h)}</p>
							{@render dots(h)}
						{:else if asked[site.id]}
							<p>
								You asked for your export {whenAsked(asked[site.id]!)}. When the email comes, download the zip
								and {canDrop ? "drop it here" : "choose it here"}.
							</p>
						{:else}
							<p>{site.wait}</p>
						{/if}
					</div>
				</div>
				<div class="act">
					{#if h}
						<button type="button" class="setup-past" disabled={!!phase} onclick={() => picker?.click()}>
							Add a newer export
						</button>
					{:else if asked[site.id]}
						<button type="button" class="setup-go quiet" disabled={!!phase} onclick={() => picker?.click()}>
							Choose the file
						</button>
					{:else}
						<button type="button" class="setup-go quiet" onclick={() => requestExport(site)}>
							Request export
						</button>
					{/if}
				</div>
			</article>
		{/each}

	</div>

	<p class="status" class:error={!!error} class:done={!!result && !error} role="status" aria-live="polite">
		{#if phase?.step === "uploading"}
			{phase.percent < 100 ? `Uploading your export, ${phase.percent}%` : "Upload finished. Starting the import…"}
			<span class="bar" aria-hidden="true"><span style:width="{phase.percent}%"></span></span>
		{:else if phase?.step === "reading"}
			Reading your chats…
		{:else if dragging}
			Drop it to bring it in.
		{:else if error}
			{error}
		{:else if result}
			{result}
		{:else if canDrop}
			Already have an export? Drop the zip anywhere on this page, or
			<button type="button" class="inline" onclick={() => picker?.click()}>choose it</button>.
		{:else}
			Already have an export?
			<button type="button" class="inline" onclick={() => picker?.click()}>Choose the zip</button>.
		{/if}
	</p>

	<input
		bind:this={picker}
		type="file"
		accept=".zip,application/zip,.json,application/json"
		multiple
		class="sr-only"
		tabindex="-1"
		aria-hidden="true"
		onchange={onPick}
	/>

	{#snippet actions()}
		{#if anything}
			<button type="button" class="setup-go" onclick={moveOn}>
				{intoLabel(setup.upNext("chats"))}
				<Icon icon="mynaui:arrow-right" width="16" />
			</button>
		{:else}
			<button type="button" class="setup-past" onclick={moveOn}>Skip for now</button>
		{/if}
	{/snippet}
</StepFrame>

<style>
	/* One row per site under a hairline, the way Connections lists the
	   device not in hand: name and line on the left, one action on the
	   right. */
	.rows {
		display: flex;
		flex-direction: column;
		text-align: left;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: 12px 20px;
		padding: 20px 0;
		border-top: 1px solid var(--color-border);
	}
	.row:first-child {
		padding-top: 0;
		border-top: 0;
	}
	.row:last-child {
		padding-bottom: 0;
	}
	.head {
		display: flex;
		gap: 0.85rem;
		align-items: flex-start;
		flex: 1 1 18rem;
		min-width: 0;
	}
	.glyph {
		flex: none;
		display: grid;
		place-items: center;
		width: 36px;
		height: 36px;
		border-radius: var(--radius-xl);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		font-family: var(--font-serif, Georgia, serif);
		font-size: 18px;
		color: var(--color-foreground);
	}
	.text {
		min-width: 0;
		flex: 1;
	}
	.text h2 {
		margin: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: 1.2rem;
		line-height: 1.2;
		color: var(--color-foreground);
	}
	.text p {
		margin: 0.3rem 0 0;
		font-size: 14px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
	.act .setup-go {
		font-size: 14px;
		padding: 0.6rem 1.15rem;
	}
	/* Row buttons are outlined, so the page keeps one filled thing: the way
	   forward. */
	.act :global(.setup-go.quiet) {
		background: transparent;
		color: var(--color-foreground);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
	}

	/* The span of months: a hairline from the first year to the last, a dot
	   for each month with chats. */
	.span {
		display: flex;
		align-items: center;
		gap: 8px;
		max-width: 20rem;
		margin-top: 10px;
	}
	.year {
		font-size: 11px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}
	.line {
		position: relative;
		flex: 1;
		height: 10px;
	}
	.line::before {
		content: "";
		position: absolute;
		left: 0;
		right: 0;
		top: 4.5px;
		height: 1px;
		background: var(--color-border);
	}
	.dot {
		position: absolute;
		top: 2px;
		width: 6px;
		height: 6px;
		margin-left: -3px;
		border-radius: 50%;
		background: var(--color-primary);
	}

	.status {
		margin: 1.75rem 0 0;
		text-align: center;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}
	.status.done {
		color: var(--color-success);
	}
	.status.error {
		color: var(--color-error);
	}
	/* The upload's progress: a hairline under the line, filling in the ink. */
	.bar {
		display: block;
		width: min(100%, 16rem);
		height: 2px;
		margin: 10px auto 0;
		border-radius: var(--radius-full);
		background: var(--color-border);
		overflow: hidden;
	}
	.bar > span {
		display: block;
		height: 100%;
		background: var(--color-primary);
		transition: width var(--m-quick) linear;
	}
	.inline {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}
</style>
