<!--
	Every model the box's catalog lists, as a table to compare and choose from.
	It is the page behind "Compare all" in the Assistant page's Models section,
	and the two write through one store, `modelSlots`.

	Every fact here is the gateway's, fetched live. Nothing on this page is
	hand-maintained (see api/model_catalog.rs for why).

	Retention and training mark only the exception. virtues-api asks for zero
	retention on every call except to a model with no zero-retention provider
	(`enforce_zdr` in services/virtues-api/src/catalog.rs), so `all`, `some`
	and a missing value all show nothing. Only `none` is marked, and it affects
	only the roles that use it. A cold box's built-in rows carry no posture at
	all, and virtues-api may know better, so those read as unknown.

	On a box with its own AI key every AI call skips virtues-api
	(virtues_api/client.rs), so retention and training are hidden there, not
	shown wrong.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { Button, Card } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import UniversalDataGrid, {
		type Column,
	} from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import type { FilterDef, FilterValue } from "$lib/components/datagrid/types";
	import {
		modelSlots,
		type CatalogModel,
		type SaveError,
		type SlotConfig,
	} from "$lib/stores/modelSlots.svelte";

	onMount(() => modelSlots.load());

	/** Grid rows need an `id`; the model id is one. */
	type ModelRow = CatalogModel & { id: string };

	const rows = $derived<ModelRow[]>(
		modelSlots.models.map((m) => ({ ...m, id: m.model_id })),
	);

	/** The roles a catalog row can fill. Pictures is not one of them: the
	 *  catalog lists language models. */
	const ROLE_SLOTS = $derived(modelSlots.slots.filter((s) => s.language));

	/** A role's failed save, on the row it concerns: the model the person
	 *  tried to choose, or the one a failed "go back" left in place. */
	function errorOnRow(slot: SlotConfig, m: ModelRow): SaveError | undefined {
		const e = modelSlots.saveErrors[slot.key];
		if (!e) return undefined;
		const here = e.attempted === m.model_id || (e.attempted === null && e.kept === m.model_id);
		return here ? e : undefined;
	}

	/** Plain words under a model's name: the roles it runs or is
	 *  recommended for. */
	function roleMarks(m: ModelRow): string[] {
		const out: string[] = [];
		for (const s of ROLE_SLOTS) {
			if (modelSlots.chosen[s.key] === m.model_id) out.push(`Your ${s.mark} model`);
			if (modelSlots.recommended[s.key] === m.model_id)
				out.push(`Recommended for ${s.phrase}`);
		}
		return out;
	}

	// ── Retention and training ───────────────────────────────────────────
	/** Hidden on a box with its own AI key: none of it reaches those calls. */
	const showPosture = $derived(!modelSlots.byoKey);

	type Posture = "zero" | "open" | "unknown";
	function retention(v: string | null | undefined): Posture {
		if (v === "none") return "open";
		if (v == null && modelSlots.catalogCold) return "unknown";
		return "zero";
	}
	const RETENTION_LABEL: Record<Posture, string> = {
		zero: "Zero retention",
		open: "Not zero retention",
		unknown: "Unknown",
	};
	/** The cell. Empty for zero retention, which is the norm. */
	function retentionMark(v: string | null | undefined): string {
		const p = retention(v);
		return p === "zero" ? "" : RETENTION_LABEL[p];
	}
	function retentionSentence(v: string | null | undefined): string {
		const p = retention(v);
		if (p === "unknown") return "Your server doesn't have this model's retention details yet.";
		if (p === "open")
			return "None of this model's providers has agreed to keep nothing. Only the roles that use it send requests there, so choose it only if that's acceptable to you.";
		if (v === "all") return "This model's providers keep nothing after they answer.";
		if (v === "some")
			return "Virtues sends your requests only to this model's providers that keep nothing.";
		return "Virtues asks for zero retention on every request to this model.";
	}

	const TRAINING_LABEL: Record<string, string> = {
		all: "No training",
		some: "Varies",
		none: "Might train",
		unknown: "Unknown",
	};
	function trainingKey(v: string | null | undefined): string {
		return v === "all" || v === "some" || v === "none" ? v : "unknown";
	}
	/** The cell. Empty for no training, which is the norm. */
	function trainingMark(v: string | null | undefined): string {
		const k = trainingKey(v);
		return k === "all" ? "" : TRAINING_LABEL[k];
	}
	function trainingSentence(v: string | null | undefined): string {
		if (v === "some") return "Some of this model's providers might train on requests.";
		if (v === "none") return "This model's providers might train on requests.";
		return "";
	}

	// ── Figures ──────────────────────────────────────────────────────────
	function perM(per1k: number | null | undefined): string {
		if (per1k == null) return "";
		const v = per1k * 1000;
		return `$${v >= 100 ? v.toFixed(0) : v.toFixed(2)}`;
	}
	function tokens(n: number | null | undefined): string {
		if (!n) return "";
		if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 ? 1 : 0)}M`;
		return `${Math.round(n / 1000)}K`;
	}
	function cardFigures(m: ModelRow): string {
		const parts: string[] = [];
		if (m.input_cost_per_1k != null) parts.push(`${perM(m.input_cost_per_1k)} in`);
		if (m.output_cost_per_1k != null) parts.push(`${perM(m.output_cost_per_1k)} out`);
		if (m.context_window) parts.push(`${tokens(m.context_window)} context`);
		return parts.join(" · ");
	}

	const CAPS: { key: keyof CatalogModel; label: string; icon: string }[] = [
		{ key: "supports_tools", label: "Tools", icon: "mynaui:tool" },
		{ key: "supports_vision", label: "Images", icon: "mynaui:eye" },
		{ key: "supports_pdf", label: "PDFs", icon: "mynaui:file-text" },
		{ key: "supports_audio", label: "Audio", icon: "mynaui:microphone" },
	];
	function capList(m: ModelRow): typeof CAPS {
		return CAPS.filter((c) => m[c.key]);
	}

	// Columns feed search, sort and group; cells come from the tableRow snippet.
	const ALL_COLUMNS: Column<ModelRow>[] = [
		{ key: "display_name", label: "Model", width: "26%", minWidth: "200px" },
		// Searchable but not a column: the id is in the expanded row.
		{ key: "model_id", label: "Id", hidden: true },
		{ key: "provider", label: "Provider", width: "11%", minWidth: "90px", groupable: true },
		{
			key: "zdr",
			label: "Retention",
			width: "11%",
			minWidth: "90px",
			groupable: true,
			groupOrder: [RETENTION_LABEL.zero, RETENTION_LABEL.open, RETENTION_LABEL.unknown],
			getValue: (m) => RETENTION_LABEL[retention(m.zdr)],
		},
		{
			key: "no_training",
			label: "Training",
			width: "10%",
			minWidth: "90px",
			hideOnMobile: true,
			getValue: (m) => TRAINING_LABEL[trainingKey(m.no_training)],
		},
		{
			key: "input_cost_per_1k",
			label: "Input",
			width: "8%",
			minWidth: "70px",
			format: "number",
			getValue: (m) => (m.input_cost_per_1k == null ? null : m.input_cost_per_1k * 1000),
		},
		{
			key: "output_cost_per_1k",
			label: "Output",
			width: "8%",
			minWidth: "70px",
			format: "number",
			hideOnMobile: true,
			getValue: (m) => (m.output_cost_per_1k == null ? null : m.output_cost_per_1k * 1000),
		},
		{
			key: "context_window",
			label: "Context (tokens)",
			width: "10%",
			minWidth: "100px",
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
	const POSTURE_KEYS = new Set(["zdr", "no_training"]);
	const columns = $derived(
		showPosture ? ALL_COLUMNS : ALL_COLUMNS.filter((c) => !POSTURE_KEYS.has(String(c.key))),
	);

	const filters = $derived.by<FilterDef<ModelRow>[]>(() => [
		...(showPosture ? postureFilters() : []),
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
	function postureFilters(): FilterDef<ModelRow>[] {
		return [
			{
				id: "zdr",
				label: "Retention",
				kind: "enum",
				options: (["zero", "open", "unknown"] as Posture[]).map((p) => ({
					value: p,
					label: RETENTION_LABEL[p],
				})),
				predicate: (m: ModelRow, v: FilterValue) => retention(m.zdr) === v,
			},
			{
				id: "no_training",
				label: "Training",
				kind: "enum",
				options: ["all", "some", "none", "unknown"].map((k) => ({
					value: k,
					label: TRAINING_LABEL[k],
				})),
				predicate: (m: ModelRow, v: FilterValue) => trainingKey(m.no_training) === v,
			},
		];
	}
</script>

<div class="catalog">
	{#if modelSlots.byoKey}
		<p class="note">
			Your own AI key is on, so your provider runs every AI request, at its own
			prices and under its own retention terms.
		</p>
	{/if}

	{#if modelSlots.catalogCold}
		<div class="cold">
			<p class="note">
				Showing the built-in models only. Your server checks for the full list
				every 5 minutes.
			</p>
			<div>
				<Button
					variant="ghost"
					size="sm"
					loading={modelSlots.loading}
					onclick={() => modelSlots.load(true)}>Check again</Button
				>
			</div>
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
		loadingMessage="Loading the models"
		searchPlaceholder="Search models"
		defaultViewMode="table"
	>
		{#snippet tableRow(m: ModelRow)}
			{@const marks = roleMarks(m)}
			<td class="cell">
				<span class="model-name">{m.display_name}</span>
				{#if marks.length}
					<span class="model-marks">{marks.join(" · ")}</span>
				{/if}
			</td>
			<td class="cell quiet">{m.provider}</td>
			{#if showPosture}
				<td class="cell quiet">{retentionMark(m.zdr)}</td>
				<td class="cell quiet hide-mobile">{trainingMark(m.no_training)}</td>
			{/if}
			<td class="cell quiet figure">{perM(m.input_cost_per_1k)}</td>
			<td class="cell quiet figure hide-mobile">{perM(m.output_cost_per_1k)}</td>
			<td class="cell quiet figure">{tokens(m.context_window)}</td>
			<td class="cell quiet hide-mobile">
				<span class="caps">
					{#each capList(m) as cap (cap.key)}
						<span class="cap" role="img" title={cap.label} aria-label={cap.label}>
							<Icon icon={cap.icon} width="14" />
						</span>
					{/each}
				</span>
			</td>
		{/snippet}

		{#snippet card(m: ModelRow)}
			{@const marks = roleMarks(m)}
			{@const figures = cardFigures(m)}
			{@const kept = showPosture ? retentionMark(m.zdr) : ""}
			<Card padding="sm" class="model-card">
				<span class="card-body">
					<span class="model-name">{m.display_name}</span>
					<span class="model-marks">{m.provider}</span>
					{#if marks.length}
						<span class="model-marks">{marks.join(" · ")}</span>
					{/if}
					{#if figures}
						<span class="model-marks figure">{figures}</span>
					{/if}
					{#if kept}
						<span class="model-marks">{kept}</span>
					{/if}
				</span>
			</Card>
		{/snippet}

		{#snippet expandDetail(m: ModelRow)}
			<div class="detail">
				<div class="facts">
					<span class="model-id">{m.model_id}</span>
					{#if m.max_output_tokens}
						<span>Up to {tokens(m.max_output_tokens)} output tokens</span>
					{/if}
					{#if capList(m).length}
						<span>{capList(m).map((c) => c.label).join(", ")}</span>
					{/if}
				</div>
				{#if showPosture}
					<p class="posture">
						{retentionSentence(m.zdr)}
						{trainingSentence(m.no_training)}
					</p>
				{/if}
				<div class="roles">
					{#each ROLE_SLOTS as slot (slot.key)}
						{@const isChosen = modelSlots.chosen[slot.key] === m.model_id}
						{@const saveError = errorOnRow(slot, m)}
						<div class="role">
							<div class="role-control">
								{#if isChosen}
									<span class="chosen">Chosen for {slot.phrase}</span>
									<Button
										variant="ghost"
										size="sm"
										onclick={() => modelSlots.choose(slot, null)}
										>Go back to the recommended model</Button
									>
								{:else}
									<Button
										variant="secondary"
										size="sm"
										onclick={() => modelSlots.choose(slot, m.model_id)}
										>Use for {slot.phrase}</Button
									>
								{/if}
							</div>
							{#if saveError}
								<p class="role-error" role="alert">{saveError.message}</p>
							{/if}
						</div>
					{/each}
				</div>
			</div>
		{/snippet}
	</UniversalDataGrid>
</div>

<style>
	.catalog {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.cold {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.note {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
		max-width: 60ch;
	}

	.cell {
		padding: 12px;
		vertical-align: top;
		font-size: 13px;
	}
	.quiet {
		color: var(--color-foreground-muted);
	}
	.figure {
		font-variant-numeric: tabular-nums;
	}

	.model-name {
		display: block;
		font-size: 14px;
		font-weight: 500;
		color: var(--color-foreground);
	}
	.model-marks {
		display: block;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.caps {
		display: inline-flex;
		align-items: center;
		gap: 8px;
	}
	.cap {
		display: inline-flex;
	}

	/* The card body. The grid's card is an unstyled button; the surface is
	   the Card inside it, stretched so a row of cards shares one height. */
	.card-body {
		display: flex;
		flex-direction: column;
	}
	:global(.v-card.model-card) {
		flex: 1 1 auto;
	}
	:global(.card:hover .v-card.model-card),
	:global(.card[aria-expanded="true"] .v-card.model-card) {
		border-color: var(--color-border-strong);
	}

	/* Capped and pinned left: in a table wider than its pane, the cell spans
	   the whole scroll width, and the controls would wrap off screen. */
	.detail {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px 12px;
		max-width: 40rem;
		position: sticky;
		left: 0;
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		column-gap: 16px;
		row-gap: 8px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.model-id {
		color: var(--color-foreground);
		user-select: text;
	}
	.posture {
		margin: 0;
		font-size: 15px;
		color: var(--color-foreground);
		max-width: 60ch;
	}

	.roles {
		display: flex;
		flex-wrap: wrap;
		column-gap: 16px;
		row-gap: 8px;
	}
	.role {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.role-control {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	.chosen {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.role-error {
		margin: 0;
		font-size: 12px;
		color: var(--color-error);
		max-width: 40ch;
	}

	/* Matches the grid's own hideOnMobile header, which a custom tableRow
	   has to mirror cell by cell. */
	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}
</style>
