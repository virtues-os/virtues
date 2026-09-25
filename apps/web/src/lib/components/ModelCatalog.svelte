<!--
	Every model the gateway carries, as a table, at the bottom of Settings →
	Assistant.

	The slot pickers above it are for choosing. At ~240 language models a
	dropdown is no place to compare anything, so this table is for finding:
	prices, context, capabilities, and the two retention facts (ZDR, training)
	as columns to sort and filter on. A row expands to the same choose/Use
	recommended controls the pickers have. Both write through `modelSlots`.

	This was a separate Settings page ("Models") until 2026-09-25. Two places
	holding one setting, each with its own words for it, is what made going
	back to the recommendation hard to find.

	Every fact here is the gateway's, fetched live. Nothing on this page is
	hand-maintained (see api/model_catalog.rs for what happened when it was).

	The Retention column reports the posture AFTER enforcement, not the
	gateway's raw tri-state: virtues-api sets `zeroDataRetention` on every call
	whose model has any zero-retention endpoint, so a `some` model is held to
	those endpoints and is zero-retention here. Only `none` (no such endpoint
	exists) is retained, and choosing one is a per-slot decision that leaves
	every other slot untouched.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { Badge, Button } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import UniversalDataGrid, {
		type Column,
	} from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import type { FilterDef } from "$lib/components/datagrid/types";
	import {
		modelSlots,
		SLOTS,
		type CatalogModel,
	} from "$lib/stores/modelSlots.svelte";

	onMount(() => modelSlots.load());

	/** Grid rows need an `id`; the model id is one. */
	type ModelRow = CatalogModel & { id: string };

	const rows = $derived<ModelRow[]>(
		modelSlots.models.map((m) => ({ ...m, id: m.model_id })),
	);

	// The table lists language models, so the Image slot has no row to choose
	// from here. Its picker above is the only control for it.
	const TABLE_SLOTS = SLOTS.filter((s) => s.key !== "image");

	/** Slot chips for a row. The recommended model keeps its chip while a
	 *  choice overrides it, so the table always shows where "Use recommended"
	 *  goes. */
	function slotChips(m: ModelRow): { label: string; chosen: boolean }[] {
		const out: { label: string; chosen: boolean }[] = [];
		for (const s of TABLE_SLOTS) {
			if (modelSlots.chosen[s.key] === m.model_id)
				out.push({ label: `${s.label} · your choice`, chosen: true });
			if (modelSlots.recommended[s.key] === m.model_id)
				out.push({ label: `${s.label} · recommended`, chosen: false });
		}
		return out;
	}

	// ── Retention rendering ──────────────────────────────────────────────────
	// What this column reports is the posture AFTER our enforcement, not the
	// gateway's raw tri-state. The box asks the gateway for zero-retention
	// routing on every call whose model has any, so `some` — retention depends
	// on which endpoint gets picked — resolves to zero retention in practice,
	// and showing "varies" would understate what we actually do. Only `none`,
	// where no zero-retention endpoint exists at all, is genuinely retained.
	function zdrText(v: string | null | undefined): string {
		if (v === "all" || v === "some") return "Zero retention";
		if (v === "none") return "Retained";
		return "—";
	}
	function trainingText(v: string | null | undefined): string {
		if (v === "all") return "No training";
		if (v === "some") return "Varies by route";
		if (v === "none") return "May train";
		return "—";
	}
	function zdrDetail(v: string | null | undefined): string {
		if (v === "all")
			return "Every provider that serves this model is zero-data-retention.";
		if (v === "some")
			return "Only some endpoints serving this model are zero-data-retention, so your server pins every request to those and never uses the others.";
		if (v === "none")
			return "No zero-data-retention endpoint exists for this model. Choosing it means the provider keeps this slot's requests; every other slot stays zero-retention.";
		return "Nobody has reported how this model handles retention. Your server requires zero retention on every call, so a provider that won't agree never gets the request.";
	}

	function perM(per1k: number | null | undefined): string {
		if (per1k == null) return "—";
		const v = per1k * 1000;
		return `$${v >= 100 ? v.toFixed(0) : v.toFixed(2)}`;
	}
	function ctxText(ctx: number | null | undefined): string {
		if (!ctx) return "—";
		if (ctx >= 1_000_000) return `${(ctx / 1_000_000).toFixed(ctx % 1_000_000 ? 1 : 0)}M`;
		return `${Math.round(ctx / 1000)}K`;
	}

	const CAPS: { key: keyof CatalogModel; label: string; icon: string }[] = [
		{ key: "supports_tools", label: "tools", icon: "ri:tools-line" },
		{ key: "supports_vision", label: "vision", icon: "ri:eye-line" },
		{ key: "supports_pdf", label: "pdf", icon: "ri:file-text-line" },
		{ key: "supports_audio", label: "audio", icon: "ri:mic-line" },
	];
	function capList(m: ModelRow): typeof CAPS {
		return CAPS.filter((c) => m[c.key]);
	}

	// Columns feed search/sort/group; cells come from the tableRow snippet.
	const columns: Column<ModelRow>[] = [
		{ key: "display_name", label: "Model", icon: "ri:cpu-line", width: "26%", minWidth: "200px" },
		// Searchable/groupable but not a column — the id is in the detail row.
		{ key: "model_id", label: "Id", hidden: true },
		{ key: "provider", label: "Provider", width: "11%", minWidth: "90px", groupable: true },
		{
			key: "zdr",
			label: "Retention",
			icon: "ri:shield-check-line",
			width: "13%",
			minWidth: "110px",
			groupable: true,
			groupOrder: ["Zero retention", "Retained", "—"],
			getValue: (m) => zdrText(m.zdr),
		},
		{
			key: "no_training",
			label: "Training",
			width: "11%",
			minWidth: "95px",
			hideOnMobile: true,
			getValue: (m) => trainingText(m.no_training),
		},
		{
			key: "input_cost_per_1k",
			label: "In $/M",
			width: "8%",
			minWidth: "70px",
			format: "number",
			getValue: (m) => (m.input_cost_per_1k == null ? null : m.input_cost_per_1k * 1000),
		},
		{
			key: "output_cost_per_1k",
			label: "Out $/M",
			width: "8%",
			minWidth: "75px",
			format: "number",
			hideOnMobile: true,
			getValue: (m) => (m.output_cost_per_1k == null ? null : m.output_cost_per_1k * 1000),
		},
		{
			key: "context_window",
			label: "Context",
			width: "8%",
			minWidth: "70px",
			format: "number",
			getValue: (m) => m.context_window ?? null,
		},
		{
			key: "supports_tools",
			label: "Capabilities",
			width: "10%",
			minWidth: "90px",
			hideOnMobile: true,
			sortable: false,
			getValue: (m) => capList(m).map((c) => c.label).join(" "),
		},
	];

	const RETENTION_OPTIONS = [
		{ value: "all", label: "Zero retention" },
		{ value: "some", label: "Zero retention (pinned)" },
		{ value: "none", label: "Retained" },
	];

	const filters = $derived.by<FilterDef<ModelRow>[]>(() => [
		{
			id: "zdr",
			label: "Retention",
			kind: "enum",
			field: "zdr",
			options: RETENTION_OPTIONS,
		},
		{
			id: "no_training",
			label: "Training",
			kind: "enum",
			field: "no_training",
			options: [
				{ value: "all", label: "No training" },
				{ value: "some", label: "Varies by route" },
				{ value: "none", label: "May train" },
			],
		},
		{
			id: "provider",
			label: "Provider",
			kind: "multi",
			field: "provider",
			options: [...new Set(rows.map((m) => m.provider))]
				.sort()
				.map((p) => ({ value: p, label: p })),
		},
	]);
</script>

<section id="model-catalog" class="space-y-3 scroll-mt-6">
	<div>
		<h2 class="text-sm font-medium text-foreground">All models</h2>
		<p class="text-xs text-foreground-subtle mt-0.5">
			Every model the gateway carries. Requests go only to zero-retention
			endpoints wherever the model has any. Open a row to choose it for a slot.
		</p>
	</div>

	{#if modelSlots.catalogCold}
		<!-- Two rows with no explanation reads as "the catalog is two models".
		     Say what this list actually is and that it heals itself. -->
		<div
			class="flex items-start gap-2 rounded-md border border-border bg-surface-alt px-3 py-2 text-xs text-foreground-muted"
		>
			<Icon icon="ri:cloud-off-line" class="mt-0.5 shrink-0" width="14" />
			<span>
				Showing the built-in defaults. This server hasn't loaded the live
				catalog yet. It retries every few minutes; the full list appears as
				soon as the cloud is reachable.
			</span>
		</div>
	{/if}

	<UniversalDataGrid
		items={rows}
		{columns}
		{filters}
		entityType="models"
		loading={modelSlots.loading && !modelSlots.loaded}
		error={modelSlots.error}
		onRetry={() => modelSlots.load(true)}
		onRefresh={() => modelSlots.load(true)}
		emptyIcon="ri:cpu-line"
		emptyMessage="Your server hasn't reached the cloud yet, so there are no models to list. Refresh once it's back online."
		loadingMessage="Loading the catalog..."
		searchPlaceholder="Search models..."
		defaultViewMode="table"
	>
		{#snippet tableRow(m: ModelRow)}
			<td class="px-3 py-2.5">
				<div class="flex items-center gap-2 flex-wrap">
					<span class="text-sm font-medium text-foreground truncate">{m.display_name}</span>
					{#each slotChips(m) as chip}
						<!-- Filled = the person's choice for that slot. Outline = what
						     Virtues recommends for it, shown even while overridden. -->
						{#if chip.chosen}
							<Badge variant="primary">{chip.label}</Badge>
						{:else}
							<Badge outline>{chip.label}</Badge>
						{/if}
					{/each}
				</div>
			</td>
			<td class="px-3 py-2.5 text-sm text-foreground-muted">{m.provider}</td>
			<td class="px-3 py-2.5">
				{#if m.zdr === "all" || m.zdr === "some"}
					<Badge variant="success">Zero retention</Badge>
				{:else if m.zdr === "none"}
					<Badge variant="warning">Retained</Badge>
				{:else}
					<span class="text-sm text-foreground-subtle">—</span>
				{/if}
			</td>
			<td class="px-3 py-2.5 hide-mobile">
				{#if m.no_training === "all"}
					<Badge variant="success">No training</Badge>
				{:else}
					<span
						class="text-sm {m.no_training
							? 'text-foreground-muted'
							: 'text-foreground-subtle'}">{trainingText(m.no_training)}</span
					>
				{/if}
			</td>
			<td class="px-3 py-2.5 text-sm font-mono text-foreground-muted">
				{perM(m.input_cost_per_1k)}
			</td>
			<td class="px-3 py-2.5 text-sm font-mono text-foreground-muted hide-mobile">
				{perM(m.output_cost_per_1k)}
			</td>
			<td class="px-3 py-2.5 text-sm font-mono text-foreground-muted">
				{ctxText(m.context_window)}
			</td>
			<td class="px-3 py-2.5 hide-mobile">
				<div class="flex items-center gap-1.5 text-foreground-muted">
					{#each capList(m) as cap}
						<Icon icon={cap.icon} width="14" aria-label={cap.label} />
					{/each}
				</div>
			</td>
		{/snippet}

		{#snippet card(m: ModelRow)}
			<div class="flex flex-col items-center gap-2 text-center">
				<span class="text-sm font-medium text-foreground break-all">{m.display_name}</span>
				<span class="text-xs text-foreground-muted">{m.provider}</span>
				{#if m.zdr === "all" || m.zdr === "some"}
					<Badge variant="success">Zero retention</Badge>
				{:else if m.zdr === "none"}
					<Badge variant="warning">Retained</Badge>
				{/if}
				<span class="text-xs font-mono text-foreground-muted">
					{perM(m.input_cost_per_1k)} / {perM(m.output_cost_per_1k)} · {ctxText(
						m.context_window,
					)}
				</span>
			</div>
		{/snippet}

		{#snippet expandDetail(m: ModelRow)}
			<div class="px-4 py-3 space-y-3 bg-surface-alt/50">
				<div class="flex items-center gap-3 flex-wrap">
					<span class="text-xs font-mono text-foreground-muted">{m.model_id}</span>
					{#if m.max_output_tokens}
						<span class="text-xs text-foreground-muted">
							max output {ctxText(m.max_output_tokens)}
						</span>
					{/if}
					{#each capList(m) as cap}
						<span class="text-xs text-foreground-muted flex items-center gap-1">
							<Icon icon={cap.icon} width="12" />{cap.label}
						</span>
					{/each}
				</div>
				<p class="text-xs text-foreground-muted max-w-prose">
					{zdrDetail(m.zdr)}
					{#if m.no_training === "all"}
						Providers don't train on request data.
					{:else if m.no_training === "some"}
						Whether providers train on request data varies by route.
					{:else if m.no_training === "none"}
						Providers may train on request data.
					{/if}
				</p>
				<div class="flex items-center gap-2 flex-wrap">
					{#each TABLE_SLOTS as slot}
						{@const isChosen = modelSlots.chosen[slot.key] === m.model_id}
						{@const isRecommended = modelSlots.recommended[slot.key] === m.model_id}
						<!-- Secondary, not primary: going back to Recommended is a real
						     action but not what this page is for, and claret read as a
						     warning here. -->
						<Button
							variant={isChosen ? "secondary" : "ghost"}
							onclick={(e: MouseEvent) => {
								// The row click owns expand/collapse; this must not also toggle it.
								e.stopPropagation();
								modelSlots.choose(slot, isChosen ? null : m.model_id);
							}}
						>
							{#if isChosen}
								<Icon icon="ri:arrow-go-back-line" width="13" />
								Use recommended for {slot.label.toLowerCase()}
							{:else if isRecommended && !modelSlots.chosen[slot.key]}
								<!-- Already this model, by recommendation. Choosing it
								     keeps it when the recommendation moves. -->
								<Icon icon="ri:pushpin-line" width="13" />
								Keep for {slot.label.toLowerCase()}
							{:else}
								<Icon icon="ri:pushpin-line" width="13" />
								Use for {slot.label.toLowerCase()}
							{/if}
						</Button>
					{/each}
					<span class="text-xs text-foreground-subtle">
						A slot on Recommended changes when Virtues recommends a different model. Your choice stays until you change it.
					</span>
				</div>
			</div>
		{/snippet}
	</UniversalDataGrid>
</section>

<style>
	/* Matches the grid's own hideOnMobile header behavior, which a custom
	   tableRow has to mirror cell-by-cell. */
	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}
</style>
