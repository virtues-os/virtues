/**
 * Client-side image normalization for chat attachments (Track E1.5).
 *
 * Goals: make iPhone photos "just work" and stop bloating the DB / token cost.
 *  - HEIC/HEIF → JPEG (browsers like Chrome can't decode HEIC; WKWebView can).
 *  - Downscale to ≤1568px long edge (Claude's ~1.15MP sweet spot) and re-encode
 *    JPEG, so a 12MP photo becomes <~500KB instead of multi-MB base64.
 *  - Skip work entirely when the image is already small + model-friendly.
 *
 * JPEG (not WebP) on purpose: Safari/WKWebView's canvas can't reliably *encode*
 * WebP and silently falls back to PNG. Re-encoding drops EXIF (incl. location) —
 * accepted for this pass.
 */

const MAX_EDGE = 1568;
const SKIP_REENCODE_BYTES = 1_000_000;
const FRIENDLY = new Set(['image/jpeg', 'image/png', 'image/webp']);

export interface NormalizedImage {
	dataUrl: string;
	mediaType: string;
	width: number;
	height: number;
}

function isHeic(file: File): boolean {
	const t = (file.type || '').toLowerCase();
	if (t === 'image/heic' || t === 'image/heif') return true;
	const n = file.name.toLowerCase();
	return n.endsWith('.heic') || n.endsWith('.heif');
}

function blobToDataURL(blob: Blob): Promise<string> {
	return new Promise((resolve, reject) => {
		const r = new FileReader();
		r.onload = () => resolve(r.result as string);
		r.onerror = () => reject(r.error);
		r.readAsDataURL(blob);
	});
}

export async function normalizeImage(file: File): Promise<NormalizedImage> {
	let source: Blob = file;
	let mediaType = file.type || 'image/jpeg';
	const heic = isHeic(file);

	// 1) HEIC/HEIF → JPEG. Lazy-load the wasm decoder only when needed.
	if (heic) {
		try {
			const heic2any = (await import('heic2any')).default as (opts: {
				blob: Blob;
				toType?: string;
				quality?: number;
			}) => Promise<Blob | Blob[]>;
			const converted = await heic2any({ blob: file, toType: 'image/jpeg', quality: 0.85 });
			source = Array.isArray(converted) ? converted[0] : converted;
			mediaType = 'image/jpeg';
		} catch {
			// WKWebView decodes HEIC natively — fall through and let canvas try.
		}
	}

	// GIF: keep as-is (downscaling via canvas would flatten the animation).
	if (mediaType === 'image/gif') {
		return { dataUrl: await blobToDataURL(source), mediaType, width: 0, height: 0 };
	}

	const bitmap = await createImageBitmap(source);
	const w = bitmap.width;
	const h = bitmap.height;
	const longEdge = Math.max(w, h);

	// Already small + a model-friendly format → send the original untouched.
	if (!heic && longEdge <= MAX_EDGE && FRIENDLY.has(mediaType) && file.size < SKIP_REENCODE_BYTES) {
		bitmap.close?.();
		return { dataUrl: await blobToDataURL(source), mediaType, width: w, height: h };
	}

	const scale = longEdge > MAX_EDGE ? MAX_EDGE / longEdge : 1;
	const tw = Math.max(1, Math.round(w * scale));
	const th = Math.max(1, Math.round(h * scale));

	const canvas = document.createElement('canvas');
	canvas.width = tw;
	canvas.height = th;
	const ctx = canvas.getContext('2d');
	if (!ctx) {
		bitmap.close?.();
		return { dataUrl: await blobToDataURL(source), mediaType, width: w, height: h };
	}
	ctx.drawImage(bitmap, 0, 0, tw, th);
	bitmap.close?.();

	return {
		dataUrl: canvas.toDataURL('image/jpeg', 0.82),
		mediaType: 'image/jpeg',
		width: tw,
		height: th,
	};
}
