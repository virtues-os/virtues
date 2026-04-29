// One-off generator for day-illustration test images.
// Usage: cd apps/web && node scripts/gen-polaroid.mjs
//
// Reads AI_GATEWAY_API_KEY from ../../.env
// Writes two PNGs into static/images/day-illustrations/ for 2026-02-13.

import { generateText } from "ai";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Load project-root .env (no dotenv dep)
const envPath = path.resolve(__dirname, "../../../.env");
if (fs.existsSync(envPath)) {
	const envText = fs.readFileSync(envPath, "utf8");
	for (const line of envText.split("\n")) {
		const m = line.match(/^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*?)\s*$/i);
		if (!m) continue;
		let v = m[2];
		if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) {
			v = v.slice(1, -1);
		}
		if (!process.env[m[1]]) process.env[m[1]] = v;
	}
}

if (!process.env.AI_GATEWAY_API_KEY) {
	console.error("AI_GATEWAY_API_KEY not found in .env");
	process.exit(1);
}

const OUT_DIR = path.resolve(__dirname, "../static/images/day-illustrations");
fs.mkdirSync(OUT_DIR, { recursive: true });

const STYLE_PREAMBLE =
	"Pen and ink line drawing, loose gestural journal-sketchbook style, " +
	"black ink only on plain white background, quick hand-drawn strokes, " +
	"minimal detail, no color, no shading fills, no frame, no border, " +
	"no text. Subject fills most of the frame.";

const VARIANTS = [
	{
		name: "2026-02-13-1x1",
		aspectHint: "Square 1:1 composition, subject centered.",
		subject:
			"a small wooden rowboat pulled up onto a pebbled shore at dusk, " +
			"a single oar resting across it, a few water-line strokes in the foreground",
	},
	{
		name: "2026-02-13-16x9",
		aspectHint:
			"Wide horizontal 16:9 panoramic composition, landscape orientation, " +
			"subject spans across the width with open sky space.",
		subject:
			"a quiet street of low bungalows at sunset, tree silhouettes, telephone poles, " +
			"a single small figure walking on the sidewalk, soft horizon line",
	},
];

async function generateOne({ name, aspectHint, subject }) {
	const prompt = `${STYLE_PREAMBLE} ${aspectHint} Subject: ${subject}.`;
	console.log(`\n→ Generating ${name}`);
	console.log(`  prompt: ${prompt}`);

	const result = await generateText({
		model: "google/gemini-2.5-flash-image",
		prompt,
	});

	const imageFiles = (result.files ?? []).filter((f) =>
		f.mediaType?.startsWith("image/"),
	);
	if (imageFiles.length === 0) {
		console.error(`  ✗ no image returned. response text: ${result.text}`);
		return;
	}
	const outPath = path.join(OUT_DIR, `${name}.png`);
	fs.writeFileSync(outPath, imageFiles[0].uint8Array);
	const kb = (imageFiles[0].uint8Array.byteLength / 1024).toFixed(1);
	console.log(`  ✓ wrote ${outPath} (${kb} KB)`);

	// Step 1: alpha-key white → transparent via luminance
	const tmpPath = `${outPath}.tmp.png`;
	execFileSync("ffmpeg", [
		"-y", "-loglevel", "error", "-i", outPath,
		"-vf",
		"format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='255-(r(X,Y)*0.299+g(X,Y)*0.587+b(X,Y)*0.114)'",
		tmpPath,
	]);
	fs.renameSync(tmpPath, outPath);

	// Step 2: auto-crop to alpha bbox + padding
	const bboxStderr = execFileSync("ffmpeg", [
		"-hide_banner", "-loglevel", "debug", "-i", outPath,
		"-vf", "alphaextract,bbox=min_val=1", "-f", "null", "-",
	], { stdio: ["ignore", "ignore", "pipe"] }).toString();
	// bbox filter doesn't write to stdout — re-run capturing stderr
	const bboxOutput = (() => {
		try {
			execFileSync("ffmpeg", [
				"-hide_banner", "-loglevel", "debug", "-i", outPath,
				"-vf", "alphaextract,bbox=min_val=1", "-f", "null", "-",
			]);
			return "";
		} catch (e) {
			return e.stderr?.toString() ?? "";
		}
	})();
	const bboxLine = (bboxStderr + bboxOutput).split("\n").find((l) =>
		/^\[Parsed_bbox/.test(l),
	);
	if (bboxLine) {
		const m = bboxLine.match(/x1:(\d+)\s+x2:(\d+)\s+y1:(\d+)\s+y2:(\d+)/);
		if (m) {
			const [x1, x2, y1, y2] = [+m[1], +m[2], +m[3], +m[4]];
			const pad = 40;
			const canvasW = 1024; const canvasH = 1024;
			const nx = Math.max(0, x1 - pad);
			const ny = Math.max(0, y1 - pad);
			const nx2 = Math.min(canvasW, x2 + pad);
			const ny2 = Math.min(canvasH, y2 + pad);
			const cw = nx2 - nx, ch = ny2 - ny;
			execFileSync("ffmpeg", [
				"-y", "-hide_banner", "-loglevel", "error", "-i", outPath,
				"-vf", `crop=${cw}:${ch}:${nx}:${ny}`, tmpPath,
			]);
			fs.renameSync(tmpPath, outPath);
			console.log(`  ✓ cropped to ${cw}×${ch} (from 1024×1024)`);
		}
	}
	const kbAfter = (fs.statSync(outPath).size / 1024).toFixed(1);
	console.log(`  ✓ final: ${kbAfter} KB`);
}

for (const v of VARIANTS) {
	try {
		await generateOne(v);
	} catch (e) {
		console.error(`  ✗ error generating ${v.name}:`, e?.message ?? e);
	}
}

console.log("\nDone.");
