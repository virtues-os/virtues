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
};

export function formatFileSize(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * Text/code/doc extensions — MIME is unreliable for these, so check the name too.
 *
 * ONE list, because there were two and they drifted: this was a regex here and a
 * hand-written `accept` attribute on the file input, and eight extensions
 * (.mjs .cjs .cc .bash .zsh .env .text .txt) had ended up droppable but greyed
 * out in the picker. Both are now built from this array, so adding a language
 * means adding it once.
 */
const TEXT_EXTENSIONS = [
	"md", "markdown", "txt", "text", "csv", "tsv", "json", "html", "htm", "xml",
	"yaml", "yml", "toml", "ini", "env", "log", "ts", "tsx", "js", "jsx", "mjs",
	"cjs", "py", "rb", "rs", "go", "java", "c", "h", "cpp", "cc", "cs", "php",
	"swift", "kt", "sh", "bash", "zsh", "sql", "css", "scss",
] as const;

const TEXT_EXT = new RegExp(`\\.(${TEXT_EXTENSIONS.join("|")})$`, "i");

/**
 * What the `+` picker offers. The MIME wildcards carry the binary kinds; the
 * extensions carry the text ones, which the OS cannot be trusted to type.
 */
export const ATTACH_ACCEPT = [
	"image/*",
	"application/pdf",
	"audio/*",
	"text/*",
	...TEXT_EXTENSIONS.map((e) => `.${e}`),
].join(",");

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
 * How big a file may be to ride inside a message, by kind.
 *
 * NOT the media backend's cap, which is what the old 100 MB here was copied
 * from. A chat attachment never reaches that backend: it travels as base64
 * inside the message, is persisted that way, and `build_context_for_llm`
 * re-emits it on every later turn (virtues-core/src/api/compaction.rs). So the
 * limit that matters is what a provider takes in one request, and base64 adds a
 * third on top of every number below.
 *
 * These four are conservative estimates, not quotes from a provider's docs —
 * pinned low enough to fail here, with a reason, rather than at the gateway
 * mid-turn where the error explains nothing. Raise one when something real
 * argues for it.
 *
 * The image number is generous because `normalizeImage` downscales to ~1568px
 * anyway — a 12 MP photo lands near 400 KB whatever it started at. It matters
 * only for the formats that pass through untouched, GIF being the deliberate
 * one.
 */
const MAX_BYTES: Record<Attachment["kind"], number> = {
	image: 10 * 1024 * 1024,
	pdf: 20 * 1024 * 1024,
	audio: 15 * 1024 * 1024,
	text: 10 * 1024 * 1024, // read whole, then truncated to MAX_TEXT
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
			if (file.size > MAX_BYTES[kind]) {
				turnedAway.push({
					name: file.name,
					why: `${formatFileSize(file.size)}, over the ${formatFileSize(MAX_BYTES[kind])} limit.`,
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

				if (kind === "image") {
					const norm = await normalizeImage(file);
					url = norm.dataUrl;
					mediaType = norm.mediaType;
					// norm.width/height are deliberately not kept: they were stored
					// on every image and read by nothing — not the tray, not the
					// transcript, not the file parts.
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

	/**
	 * Put back what a send took but never delivered.
	 *
	 * `takeAsFileParts` clears the tray before the request goes out, so a throw
	 * on the way to the provider used to leave the person with no files and no
	 * message — nothing sent, everything consumed. Anything staged in the
	 * meantime wins, hence the signature check.
	 */
	restore(previous: Attachment[]) {
		const already = new Set(this.items.map((a) => a.sig));
		this.items = [...previous.filter((a) => !already.has(a.sig)), ...this.items];
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
