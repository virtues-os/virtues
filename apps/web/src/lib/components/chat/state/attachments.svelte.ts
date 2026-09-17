/**
 * attachments — what is staged above the composer, and how a file becomes it.
 *
 * Track E1: multimodal attachments. Files are read to base64 data URLs (so they
 * round-trip to the provider and render on reload) and sent as AI SDK file parts.
 *
 * Owns: the staged list, the drag-hover flag, the kind sniffing, the reads, the
 * caps, and SAYING SO when a file is turned away. It does NOT decide whether the
 * active model can read them — that is the capability gate, which belongs with
 * the model choice.
 *
 * The reporting is not decoration. Every rejection here used to be a bare
 * `continue`, so an unsupported file, an oversized one and an undecodable one
 * all looked identical to a drop target that does not work — which is exactly
 * what a real drop bug looks like too (2026-09-17). The toast lives in the
 * controller rather than the caller so a future call site cannot forget it.
 */

import { toast } from "svelte-sonner";
import { normalizeImage } from "$lib/multimodal/normalizeImage";

export type Attachment = {
	id: string;
	mediaType: string;
	url: string; // data URL
	filename: string;
	size: number;
	kind: "image" | "pdf" | "audio" | "text";
	/** name|size|lastModified — what makes "the same file again" answerable. */
	sig: string;
	width?: number;
	height?: number;
};

export function formatFileSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// Text/code/doc extensions — MIME is unreliable for these, so check the name too.
const TEXT_EXT =
	/\.(md|markdown|txt|text|csv|tsv|json|html?|xml|ya?ml|toml|ini|env|log|ts|tsx|js|jsx|mjs|cjs|py|rb|rs|go|java|c|h|cpp|cc|cs|php|swift|kt|sh|bash|zsh|sql|css|scss)$/i;

function attachmentKind(file: File): Attachment["kind"] | null {
	const mt = (file.type || "").toLowerCase();
	if (mt.startsWith("image/")) return "image";
	if (mt === "application/pdf") return "pdf";
	if (mt.startsWith("audio/")) return "audio";
	if (mt.startsWith("text/") || mt === "application/json" || TEXT_EXT.test(file.name))
		return "text";
	return null;
}

function readAsDataURL(file: File): Promise<string> {
	return new Promise((resolve, reject) => {
		const r = new FileReader();
		r.onload = () => resolve(r.result as string);
		r.onerror = () => reject(r.error);
		r.readAsDataURL(file);
	});
}

function readAsText(file: File): Promise<string> {
	return new Promise((resolve, reject) => {
		const r = new FileReader();
		r.onload = () => resolve(r.result as string);
		r.onerror = () => reject(r.error);
		r.readAsText(file);
	});
}

/**
 * What the PROVIDER will take in one request — NOT what the media backend will
 * store. A chat attachment never reaches the media backend: it travels as
 * base64 inside the message, is persisted that way, and `build_context_for_llm`
 * re-emits it on every later turn (virtues-core/src/api/compaction.rs). The old
 * cap was 100 MB "to match the media backend", which let a file stage that the
 * gateway would then refuse mid-turn — an error the capability gate has no way
 * to predict, because it judges modality and never size.
 *
 * These are the tightest documented per-request ceilings across the models we
 * route to, measured on the ENCODED payload, since that is what ships.
 */
const ENCODED_MAX: Record<Attachment["kind"], number> = {
	image: 5 * 1024 * 1024,
	pdf: 30 * 1024 * 1024,
	audio: 20 * 1024 * 1024,
	text: 1 * 1024 * 1024, // never reached: the content is truncated to MAX_TEXT
};

/**
 * The cap we can apply BEFORE reading the file, from `File.size` alone.
 *
 * base64 inflates by 4/3, so a raw file three-quarters of the ceiling encodes to
 * about the ceiling. An image is the exception: `normalizeImage` downscales it
 * to ~1568px first, so a 12 MP photo lands near 400 KB however big it started —
 * judging it on its raw size would turn away files that are perfectly fine. Its
 * number here only bounds the decode, and the real check happens after.
 */
const RAW_MAX: Record<Attachment["kind"], number> = {
	image: 50 * 1024 * 1024,
	pdf: Math.floor((ENCODED_MAX.pdf * 3) / 4),
	audio: Math.floor((ENCODED_MAX.audio * 3) / 4),
	text: 10 * 1024 * 1024,
};

const KINDS_SENTENCE = "Images, PDFs, audio, and text files only.";

/** Identity for "you already attached this one". */
function signature(file: File): string {
	return `${file.name}|${file.size}|${file.lastModified}`;
}

function base64Utf8(s: string): string {
	const bytes = new TextEncoder().encode(s);
	let bin = "";
	for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i]);
	return btoa(bin);
}

export class AttachmentsController {
	items = $state<Attachment[]>([]);
	dragActive = $state(false);

	get count(): number {
		return this.items.length;
	}

	async add(files: File[]) {
		const MAX_TEXT = 100 * 1024; // inline-text cap (~25k tokens) before truncating
		const turnedAway: { name: string; why: string }[] = [];
		for (const file of files) {
			const kind = attachmentKind(file);
			if (!kind) {
				turnedAway.push({ name: file.name, why: KINDS_SENTENCE });
				continue;
			}
			if (file.size > RAW_MAX[kind]) {
				turnedAway.push({
					name: file.name,
					why: `${formatFileSize(file.size)}, over the ${formatFileSize(RAW_MAX[kind])} limit.`,
				});
				continue;
			}
			// Against the live list, so the same file dropped twice in one batch
			// is caught as well as one dropped twice a minute apart.
			const sig = signature(file);
			if (this.items.some((a) => a.sig === sig)) {
				turnedAway.push({ name: file.name, why: "Already attached." });
				continue;
			}
			try {
				let mediaType = file.type || "application/octet-stream";
				let url: string;
				let width: number | undefined;
				let height: number | undefined;

				if (kind === "image") {
					const norm = await normalizeImage(file);
					// normalizeImage passes some formats through untouched — GIF
					// deliberately, to keep the animation — so the only honest
					// place to weigh an image is after it, on what will ship.
					if (norm.dataUrl.length > ENCODED_MAX.image) {
						turnedAway.push({
							name: file.name,
							why: `Still ${formatFileSize(norm.dataUrl.length)} after encoding, over the ${formatFileSize(ENCODED_MAX.image)} limit.`,
						});
						continue;
					}
					url = norm.dataUrl;
					mediaType = norm.mediaType;
					width = norm.width || undefined;
					height = norm.height || undefined;
				} else if (kind === "text") {
					let text = await readAsText(file);
					if (text.length > MAX_TEXT) text = text.slice(0, MAX_TEXT) + "\n…[truncated]";
					mediaType = "text/plain";
					url = `data:text/plain;base64,${base64Utf8(text)}`;
				} else {
					url = await readAsDataURL(file);
				}

				this.items = [
					...this.items,
					{
						id: crypto?.randomUUID?.() ?? `att-${this.items.length}-${file.size}`,
						sig,
						mediaType,
						url,
						filename: file.name,
						size: file.size,
						kind,
						width,
						height,
					},
				];
			} catch {
				turnedAway.push({ name: file.name, why: "The file could not be read." });
			}
		}
		if (turnedAway.length > 0) this.#reportTurnedAway(turnedAway);
	}

	/** One toast per batch: dropping a folder of twenty must not fire twenty. */
	#reportTurnedAway(turnedAway: { name: string; why: string }[]) {
		if (turnedAway.length === 1) {
			const [only] = turnedAway;
			toast.error(`Could not attach ${only.name}`, { description: only.why });
			return;
		}
		toast.error(`Could not attach ${turnedAway.length} files`, {
			description: turnedAway.map((t) => `${t.name} — ${t.why}`).join("\n"),
		});
	}

	remove(id: string) {
		this.items = this.items.filter((a) => a.id !== id);
	}

	/** Hand the staged files to the SDK as file parts and clear the tray. */
	takeAsFileParts() {
		const files = this.items.map((a) => ({
			type: "file" as const,
			mediaType: a.mediaType,
			url: a.url,
			filename: a.filename,
		}));
		this.items = [];
		return files;
	}
}
