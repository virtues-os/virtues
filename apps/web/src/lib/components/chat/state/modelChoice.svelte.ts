/**
 * modelChoice — what the picker shows, what goes on the wire, and the two
 * ways out when the shown model cannot do the job.
 *
 * The distinction this file exists to keep: what the picker is SHOWING is not
 * a choice. The box resolves every unpinned turn from the slot (see
 * `api/model_choice.rs`), which is what lets a slot swap reach a conversation
 * already in progress, and what keeps a client that failed to load the catalog
 * able to chat at all. So the id only goes on the wire once the person has
 * moved the picker off what we prefilled for them.
 */

import type { ModelOption } from "$lib/config/models";
import {
	getSelectedModel,
	getDefaultModel,
	getModels,
	setSelectedModel,
	initializeSelectedModel,
} from "$lib/stores/models.svelte";
import type { Attachment } from "./attachments.svelte";

export class ModelChoiceController {
	/** Model selection state - bindable for the ChatInput toolbar */
	selected = $state<ModelOption | undefined>(undefined);
	/** The id we put in the picker ourselves, so `idForWire` can tell a
	 *  prefill apart from a choice. Picking this exact model back is the same
	 *  as not choosing: either way the box resolves the slot. */
	prefilledId = $state<string | undefined>(undefined);

	// The catalog, read from the one store that loads it. This used to be a
	// SECOND fetch of /api/models with a `.catch(() => {})`, so the list the
	// attachment gate judged against and the list everything else used were
	// different objects that could disagree about what models exist. There is
	// no picker in the composer, so the only readers left are the capability
	// gate and the two recovery buttons below.
	readonly available = $derived(getModels());

	#attachments: () => Attachment[];

	constructor(attachments: () => Attachment[]) {
		this.#attachments = attachments;
	}

	// The id that goes on the wire — ONLY when the person moved the picker off
	// what we prefilled for them. What the picker is showing is not a choice.
	//
	// This function returned `""` when the catalog had not loaded, and the box
	// rejected it with a 400 listing 244 allowed ids and never the one it refused.
	idForWire(): string | undefined {
		const id = this.selected?.id;
		return id && id !== this.prefilledId ? id : undefined;
	}

	/** Show what the next turn will use: the owner's pin, else the Virtues
	 *  default. Records what it showed; sends nothing. */
	prefillDisplay(profileDefaultModelId?: string) {
		// Re-seed on every mount UNLESS a recovery button picked something this
		// session. The store's seeder only runs once per session, so changing
		// your pin in Settings used to leave the attachment capability gate
		// judging against the model you were pinned to before — warning that
		// you cannot send an image to a model that was no longer answering.
		const picked = this.selected && this.selected.id !== this.prefilledId;
		if (!picked) setSelectedModel(undefined);
		initializeSelectedModel(profileDefaultModelId);
		const shown = getSelectedModel() ?? getDefaultModel();
		if (!shown) return; // catalog not loaded — the box still knows
		this.selected = shown;
		this.prefilledId = shown.id;
	}

	/** Sync from the store on initial load. Still a prefill, not a choice —
	 *  record it as one. */
	adoptStoreSelection() {
		const storeModel = getSelectedModel();
		if (storeModel && !this.selected) {
			this.selected = storeModel;
			this.prefilledId = storeModel.id;
		}
	}

	// Capability gate: does the active model support every attached modality? If not,
	// surface a switch to a model that does (or note none is available).
	readonly capabilityIssue = $derived.by(() => {
		const attachments = this.#attachments();
		if (attachments.length === 0) return null;
		const model =
			this.available.find((m) => m.id === this.selected?.id) ?? this.selected ?? null;
		// No catalog means no capabilities to judge, not a model that lacks
		// them. Reading absent flags as "unsupported" told anyone whose
		// catalog failed to load that they could not attach an image, while
		// the box would have taken it happily.
		if (!model || this.available.length === 0) return null;
		const needs = {
			image: attachments.some((a) => a.kind === "image"),
			pdf: attachments.some((a) => a.kind === "pdf"),
			audio: attachments.some((a) => a.kind === "audio"),
		};
		const lacks: string[] = [];
		if (needs.image && !model?.supportsVision) lacks.push("images");
		if (needs.pdf && !model?.supportsPdf) lacks.push("PDFs");
		if (needs.audio && !model?.supportsAudio) lacks.push("audio");
		if (lacks.length === 0) return null;
		const candidate =
			this.available.find(
				(m) =>
					(!needs.image || m.supportsVision) &&
					(!needs.pdf || m.supportsPdf) &&
					(!needs.audio || m.supportsAudio),
			) ?? null;
		return { lacks, modelName: model?.displayName ?? "This model", candidate };
	});

	switchToCapable() {
		const candidate = this.capabilityIssue?.candidate;
		if (candidate) this.selected = candidate;
	}

	// Runtime recovery: when a picked model errors (unsupported tools, context
	// overflow, a gateway quirk), let the user drop to the Recommended model and
	// re-run in one click — a plain retry would just re-hit the same model.
	readonly recommendedFallback = $derived.by(() => {
		const rec = getDefaultModel();
		const currentId = this.selected?.id ?? getDefaultModel()?.id;
		// Only worth offering when we'd actually change models.
		return rec && rec.id !== currentId ? rec : null;
	});

	/** Drop to Recommended and hand the retry back to the caller. */
	switchToRecommended(): boolean {
		const rec = getDefaultModel();
		if (rec) {
			this.selected = rec;
			setSelectedModel(rec);
			return true;
		}
		return false;
	}
}
