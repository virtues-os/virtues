#!/usr/bin/env node
// gen-day-illustration.mjs
//
// Called by the Rust nightly illustration action as a subprocess.
// Generates one day's pen-and-ink illustration via Vercel AI Gateway,
// alpha-keys the white background, auto-crops to content bbox, and
// writes the PNG to the target path.
//
// Usage:
//   node gen-day-illustration.mjs --prompt "..." --output /abs/path/to/2026-02-13.png
// Or via env:
//   ILLUSTRATION_PROMPT="..." ILLUSTRATION_OUTPUT=/abs/path node gen-day-illustration.mjs
//
// Reads AI_GATEWAY_API_KEY from the env (inherits from parent process).
// Requires ffmpeg in PATH.
//
// Exit codes: 0 success, 1 invalid args, 2 api error, 3 postprocess error.

import { generateText } from "ai";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

// ── Arg parsing ─────────────────────────────────────────────────────────────
const args = process.argv.slice(2);
function arg(name) {
	const i = args.indexOf(`--${name}`);
	return i >= 0 ? args[i + 1] : undefined;
}

const prompt = arg("prompt") ?? process.env.ILLUSTRATION_PROMPT;
const outputPath = arg("output") ?? process.env.ILLUSTRATION_OUTPUT;

if (!prompt || !outputPath) {
	console.error("Usage: gen-day-illustration.mjs --prompt TEXT --output PATH");
	process.exit(1);
}
if (!process.env.AI_GATEWAY_API_KEY) {
	console.error("AI_GATEWAY_API_KEY not set in environment");
	process.exit(1);
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true });

// ── Generate image ───────────────────────────────────────────────────────────
console.log(`Generating: ${outputPath}`);
console.log(`Prompt: ${prompt.slice(0, 140)}${prompt.length > 140 ? "…" : ""}`);

try {
	const result = await generateText({
		model: "google/gemini-2.5-flash-image",
		prompt,
	});
	const imageFiles = (result.files ?? []).filter((f) =>
		f.mediaType?.startsWith("image/"),
	);
	if (imageFiles.length === 0) {
		console.error(`No image returned. Text: ${result.text}`);
		process.exit(2);
	}
	fs.writeFileSync(outputPath, imageFiles[0].uint8Array);
	console.log(`  ✓ raw PNG ${(imageFiles[0].uint8Array.byteLength / 1024).toFixed(1)} KB`);
} catch (e) {
	console.error(`Image gen failed: ${e?.message ?? e}`);
	process.exit(2);
}

// ── Post-process: alpha-key white → transparent ──────────────────────────────
const tmp = `${outputPath}.tmp.png`;
try {
	execFileSync("ffmpeg", [
		"-y", "-loglevel", "error", "-i", outputPath,
		"-vf",
		"format=rgba,geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='255-(r(X,Y)*0.299+g(X,Y)*0.587+b(X,Y)*0.114)'",
		tmp,
	]);
	fs.renameSync(tmp, outputPath);
	console.log("  ✓ alpha-keyed");
} catch (e) {
	console.error(`Alpha-key failed: ${e?.message ?? e}`);
	process.exit(3);
}

// ── Post-process: auto-crop to alpha bbox + padding ──────────────────────────
try {
	let bboxStderr = "";
	try {
		execFileSync("ffmpeg", [
			"-hide_banner", "-loglevel", "debug", "-i", outputPath,
			"-vf", "alphaextract,bbox=min_val=1", "-f", "null", "-",
		], { stdio: ["ignore", "ignore", "pipe"] });
	} catch (e) {
		bboxStderr = e.stderr?.toString() ?? "";
	}
	const bboxLine = bboxStderr.split("\n").find((l) => /^\[Parsed_bbox/.test(l));
	if (bboxLine) {
		const m = bboxLine.match(/x1:(\d+)\s+x2:(\d+)\s+y1:(\d+)\s+y2:(\d+)/);
		if (m) {
			const [x1, x2, y1, y2] = [+m[1], +m[2], +m[3], +m[4]];
			const pad = 40;
			const W = 1024, H = 1024;
			const nx = Math.max(0, x1 - pad);
			const ny = Math.max(0, y1 - pad);
			const nx2 = Math.min(W, x2 + pad);
			const ny2 = Math.min(H, y2 + pad);
			const cw = nx2 - nx, ch = ny2 - ny;
			execFileSync("ffmpeg", [
				"-y", "-hide_banner", "-loglevel", "error", "-i", outputPath,
				"-vf", `crop=${cw}:${ch}:${nx}:${ny}`, tmp,
			]);
			fs.renameSync(tmp, outputPath);
			console.log(`  ✓ cropped ${cw}×${ch}`);
		}
	}
} catch (e) {
	console.error(`Crop failed (non-fatal): ${e?.message ?? e}`);
	// Don't exit — uncropped image is still usable
}

const finalSize = (fs.statSync(outputPath).size / 1024).toFixed(1);
console.log(`Done: ${outputPath} (${finalSize} KB)`);
