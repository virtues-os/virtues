<!--
	Models — the full gateway catalog as a browsable table.

	The Assistant page's four slot pickers are for *pinning*; at ~240 language
	models a dropdown is no place to compare anything. This room is for
	*finding*: every model virtues-api currently mirrors from the gateway, with
	prices, context, capabilities, and the two retention facts (ZDR, training)
	as columns you can sort and filter on. Click a row for details and the pin
	controls.

	Every fact here is the gateway's, fetched live — nothing on this page is
	hand-maintained (see api/model_catalog.rs for what happened when it was).
	Retention is a tri-state and only "all" is a promise: "some" means it
	depends on which endpoint serves a given request. Unknown renders as a
	dash, never as a claim in either direction.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page, Badge, Button } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import UniversalDataGrid, {
		type Column,
	} from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import type { FilterDef } from "$lib/components/datagrid/types";
	import {
		getRecommendedModels,
		getAssistantProfile,
		updateAssistantProfile,
	} from "$lib/api/client";
	import { createResource } from "$lib/utils/resource.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	interface ApiModel {
		model_id: string;
		display_name: string;
		provider: string;
		context_window?: number | null;
		max_output_tokens?: number | null;
		supports_tools?: boolean | null;
		supports_vision?: boolean | null;
		supports_pdf?: boolean | null;
		supports_audio?: boolean | null;
		input_cost_per_1k?: number | null;
		output_cost_per_1k?: number | null;
		recommended?: boolean;
		/** "all" | "some" | "none" | null — null is unknown, not "none". */
		zdr?: string | null;
		no_training?: string | null;
	}

	/** Grid rows need an `id`; the model id is one. */
	type ModelRow = ApiModel & { id: string };

	const res = createResource(() => getRecommendedModels<any>());
	const profileRes = createResource(() =>
		getAssistantProfile<any>().catch(() => null),
	);

	const rows = $derived.by<ModelRow[]>(() => {
		const data: ApiModel[] = Array.isArray(res.data)
			? res.data
			: (res.data?.data ?? []);
		return data.map((m) => ({ ...m, id: m.model_id }));
	});

	/** What each slot resolves to when unpinned — the cloud's choice. */
	const slotDefaults = $derived<Record<string, string>>(res.data?.slots ?? {});

	// The user's pins, kept locally so a pin from this page reflects without a
	// refetch. Same field/legacy-fallback reading as ModelSettings.
	let pins = $state<Record<SlotKey, string | null>>({
		chat: null,
		lite: null,
		coding: null,
	});
	$effect(() => {
		const p = profileRes.data;
		if (!p) return;
		pins = {
			chat: p.chat_model_id || p.default_model_id || null,
			lite: p.lite_model_id || p.background_model_id || null,
			coding: p.coding_model_id || null,
		};
	});

	// The chat-facing slots only. Image never appears here (this list is the
	// gateway's language models) and Omni is a fixed system slot.
	type SlotKey = "chat" | "lite" | "coding";
	const SLOTS: { key: SlotKey; label: string; dbField: string; legacyField?: string }[] = [
		{ key: "chat", label: "Chat", dbField: "chat_model_id", legacyField: "default_model_id" },
		{ key: "lite", label: "Lite", dbField: "lite_model_id", legacyField: "background_model_id" },
		{ key: "coding", label: "Coding", dbField: "coding_model_id" },
	];

	let saveError = $state<string | null>(null);

	async function setPin(slot: (typeof SLOTS)[number], modelId: string | null) {
		const previous = pins[slot.key];
		pins[slot.key] = modelId;
		saveError = null;
		// `null` clears the pin — the backend reads that as "follow the
		// default"; the legacy column is cleared with it so an old value can't
		// resurrect through the `.or()` fallback (see ModelSettings).
		const body: Record<string, string | null> = { [slot.dbField]: modelId };
		if (slot.legacyField && modelId === null) body[slot.legacyField] = null;
		try {
			await updateAssistantProfile(body);
		} catch (e) {
			pins[slot.key] = previous;
			saveError =
				`Couldn't save ${slot.label}. ${e instanceof Error ? e.message : ""}`.trim();
		}
	}

	/** Slot chips for a row: a pin beats the default it displaces. */
	function slotChips(m: ModelRow): { label: string; pinned: boolean }[] {
		const out: { label: string; pinned: boolean }[] = [];
		for (const s of SLOTS) {
			if (pins[s.key] === m.model_id) out.push({ label: s.label, pinned: true });
			else if (!pins[s.key] && slotDefaults[s.key] === m.model_id)
				out.push({ label: s.label, pinned: false });
		}
		return out;
	}

	// ── Retention rendering ──────────────────────────────────────────────────
	// Positives get a badge; negatives are plain words; unknown is a dash.
	// "Some" is deliberately not a badge — it isn't a promise, it's a maybe.
	function zdrText(v: string | null | undefined): string {
		if (v === "all") return "Zero retention";
		if (v === "some") return "Varies by route";
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
			return "Every endpoint the gateway can route this model to is zero-data-retention.";
		if (v === "some")
			return "Some endpoints serving this model are zero-data-retention — it depends on which one the gateway picks for a given request.";
		if (v === "none")
			return "The endpoints serving this model retain request data.";
		return "The gateway hasn't reported a retention posture for this model.";
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

	const CAPS: { key: keyof ApiModel; label: string; icon: string }[] = [
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
			groupOrder: ["Zero retention", "Varies by route", "Retained", "—"],
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
		{ value: "some", label: "Varies by route" },
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

<Page
	title="Models"
	description="Every model the gateway carries — prices, capabilities, and retention, reported live."
	maxWidth="wide"
>
	{#if saveError}
		<div class="mb-3 text-sm text-error">{saveError}</div>
	{/if}

	{#if res.data?.catalog_cold}
		<!-- Two rows with no explanation reads as "the catalog is two models".
		     Say what this list actually is and that it heals itself. -->
		<div
			class="mb-3 flex items-start gap-2 rounded-md border border-border bg-surface-alt px-3 py-2 text-xs text-foreground-muted"
		>
			<Icon icon="ri:cloud-off-line" class="mt-0.5 shrink-0" width="14" />
			<span>
				Showing the built-in defaults — this server hasn't loaded the live
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
		loading={res.loading}
		error={res.error}
		onRetry={res.reload}
		onRefresh={res.reload}
		emptyIcon="ri:cpu-line"
		emptyMessage="The catalog is empty — this box hasn't reached the cloud yet. It fills in on the next refresh."
		loadingMessage="Loading the catalog..."
		searchPlaceholder="Search models..."
		defaultViewMode="table"
	>
		{#snippet tableRow(m: ModelRow)}
			<td class="px-3 py-2.5">
				<div class="flex items-center gap-2 flex-wrap">
					<span class="text-sm font-medium text-foreground truncate">{m.display_name}</span>
					{#each slotChips(m) as chip}
						<!-- "Chat" = this model currently fills that slot. Outline =
						     our default doing so; filled = the user pinned it. -->
						{#if chip.pinned}
							<Badge variant="primary">{chip.label}</Badge>
						{:else}
							<Badge outline>{chip.label} default</Badge>
						{/if}
					{/each}
				</div>
			</td>
			<td class="px-3 py-2.5 text-sm text-foreground-muted">{m.provider}</td>
			<td class="px-3 py-2.5">
				{#if m.zdr === "all"}
					<Badge variant="success">Zero retention</Badge>
				{:else if m.zdr === "some"}
					<span class="text-sm text-foreground-muted">Varies by route</span>
				{:else if m.zdr === "none"}
					<span class="text-sm text-foreground-muted">Retained</span>
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
				{#if m.zdr === "all"}
					<Badge variant="success">Zero retention</Badge>
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
					{#each SLOTS as slot}
						{@const isPinned = pins[slot.key] === m.model_id}
						<Button
							variant={isPinned ? "primary" : "ghost"}
							onclick={(e: MouseEvent) => {
								// The row click owns expand/collapse; this must not also toggle it.
								e.stopPropagation();
								setPin(slot, isPinned ? null : m.model_id);
							}}
						>
							{#if isPinned}
								<Icon icon="ri:pushpin-fill" width="13" />
								{slot.label} — unpin
							{:else}
								<Icon icon="ri:pushpin-line" width="13" />
								Use for {slot.label}
							{/if}
						</Button>
					{/each}
					<span class="text-xs text-foreground-subtle">
						Unpinned slots follow the Virtues default.
					</span>
				</div>
			</div>
		{/snippet}
	</UniversalDataGrid>
</Page>

<style>
	/* Matches the grid's own hideOnMobile header behavior, which a custom
	   tableRow has to mirror cell-by-cell. */
	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}
</style>
