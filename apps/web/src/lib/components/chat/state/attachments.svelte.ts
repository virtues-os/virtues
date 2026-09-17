/**
 * attachments — what is staged above the composer, and how a file becomes it.
 *
 * Track E1: multimodal attachments. Files are read to base64 data URLs (so they
 * round-trip to the provider and render on reload) and sent as AI SDK file parts.
 *
 * Owns: the staged list, the drag-hover flag, the kind sniffing, the reads and
 * the caps. It does NOT decide whether the active model can read them — that is
 * the capability gate, which belongs with the model choice.
 */

import { normalizeImage } from "$lib/multimodal/normalizeImage";

export type Attachment = {
	id: string;
	mediaType: string;
	url: string; // data URL
	filename: string;
	size: number;
	kind: "image" | "pdf" | "audio" | "text";
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
		const MAX = 100 * 1024 * 1024; // 100 MB, matches the media backend cap
		const MAX_TEXT = 100 * 1024; // inline-text cap (~25k tokens) before truncating
		for (const file of files) {
			const kind = attachmentKind(file);
			if (!kind || file.size > MAX) continue;
			try {
				let mediaType = file.type || "application/octet-stream";
				let url: string;
				let width: number | undefined;
				let height: number | undefined;

				if (kind === "image") {
					const norm = await normalizeImage(file);
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
				/* unreadable / undecodable file — skip */
			}
		}
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
