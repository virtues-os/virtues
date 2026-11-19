// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/band.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* band.js: a diurnal profile. How often something is true at each hour of the day, drawn as the day's light, for
   weekdays and weekends: being at a place, being active, being online. The words come from how often, never from
   what the thing is; the title names it.

   A new chart mark, `mark: "diurnal"`, from the kept tile "A place card colored by the hours you spend there" (lab 10,
   t-06-g-place.js), whose WebGL band shader this ports to a canvas drawn once per size and theme. Lab 12's map tile
   (t-16-g-map.js) ported the same band to a per-pixel canvas; this follows it.
   - Three columns, found by name: `hour` (0 to 23, a whole number), `days` (weekdays or weekends) and `share`
     (0 to 1: of the days of that kind in the window, the part on which it was true at some point in that hour). Up to 48
     rows; an hour the query leaves out is 0.
   - Two rows of light, weekdays above and weekends below, across the 24 hours of the day. The color stands only
     for the hour, from the painting: night lake, dawn blue, morning sky, midday cream, afternoon sand, evening
     ochre, dusk blue. How often it is true sets how strongly it shows. Where it holds on most days the light
     is solid; its edges, and hours it holds only now and then, break into a 45 degree dot screen whose dots grow
     with the share. Each row is also a quiet track of the same screen, so the light grows out of dots that are
     always there.
   - The paint: the painting's hues are fixed values, written here in OKLab. Only their lightness comes from the
     theme. On a light page they are laid on a step darker than the page, as lab 10 laid them on white. On a dark
     page they are laid a step lighter than the page, as dim light, so the band never glares and every hour keeps
     its hue. The dot track is --color-border-strong and the hour you read is marked in --color-primary.
   - Rest on an hour, or tab to the band and use the arrow keys (left and right move the hour, up and down change
     the row), to read it in words: "Weekdays, 7 PM, Most days, 6 PM to 9 PM". The words are "Most days" from
     a share of 0.6, "Often" from 0.35, "Sometimes" from 0.1 and "Rarely" below that. The shares themselves are in
     See the rows; the band shows no counts and no percentages.
   Block: readout 26 + plot 140 + foot 26 = 192px. Two rows of light need less than a chart's 200px plot: each
   row's light is 44px tall with its soft edge, with a 10px gap between them and the hour axis below.
   Draw cost: the light is computed per device pixel once per width, theme and density, about 330,000 pixels on a
   phone, and kept as a bitmap. Resting on an hour redraws only the bitmap and a hairline. Nothing runs while
   nothing moves.

   Contract: see the top of kit.js. The frame runs its dot along box.bottom, the hour axis, and the light arrives
   behind it through setReveal. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'band', kinds: ['chart'], marks: ['diurnal'] });

const HEIGHT = 140;
const PAD = Object.freeze({ top: 8, right: 8, bottom: 24, left: 64 });
const ROW_Y = Object.freeze([30, 84]);   /* the two rows' centers */
const CH = 8, FY = 14;                   /* a row's solid core half-height, and its soft edge */
const LIGHT_H = 110;                     /* the light is drawn over y 0 to 110; the hour axis sits below */
const PITCH = 4.2;                       /* the dot screen's pitch, at 45 degrees */
const WORDS = Object.freeze(['Weekdays', 'Weekends']);
const TAU = Math.PI * 2;
const RT = Math.SQRT1_2;

/* The painting's light at each hour (lab 10's KEYS), sampled from the home painting. */
const LIGHT = Object.freeze([[0, '#8FA6AE'], [4.5, '#8FA6AE'], [6, '#A9C1CC'], [8, '#C3D5DC'], [10.5, '#D2DDD8'],
  [12.5, '#E9D5A8'], [15, '#E8CC9A'], [17.5, '#E3B877'], [20.5, '#DDAD78'], [22, '#A9BAC5'], [23.5, '#8FA6AE'], [24, '#8FA6AE']]);

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

/* ---------- color: sRGB and OKLab ---------- */

const lin = (c) => (c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4));
const enc = (c) => { c = c < 0 ? 0 : c > 1 ? 1 : c; return 255 * (c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055); };
function toLab(rgb) {
  const r = lin(rgb[0] / 255), g = lin(rgb[1] / 255), b = lin(rgb[2] / 255);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s, 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s];
}
function toRgb(lab) {
  let l = lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2];
  let m = lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2];
  let s = lab[0] - 0.0894841775 * lab[1] - 1.2914855480 * lab[2];
  l = l * l * l; m = m * m * m; s = s * s * s;
  return [enc(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s), enc(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s), enc(-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s)];
}
const hexRgb = (h) => [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
/* Lab 10's lift for the palest hours: a little more chroma and a little less lightness, so morning and midday
   light still show on paper. Colors that already carry, like the ochre, are left alone. */
function deepen([l, a, b]) {
  const C = Math.hypot(a, b), k = kit.clamp(1 - C / 0.08, 0, 1), f = C > 1e-5 ? (C + 0.025 * k) / C : 1;
  const t = kit.clamp((l - 0.8) / 0.15, 0, 1);
  return [l - 0.05 * t * t * (3 - 2 * t), a * f, b * f];
}
const KEYS = LIGHT.map(([h, c]) => [h, deepen(toLab(hexRgb(c)))]);
function hourLab(h) {
  h = kit.clamp(h, 0, 24);
  let j = 0;
  while (j < KEYS.length - 2 && KEYS[j + 1][0] < h) j++;
  const [h0, c0] = KEYS[j], [h1, c1] = KEYS[j + 1], t = kit.clamp((h - h0) / (h1 - h0 || 1), 0, 1);
  return [kit.lerp(c0[0], c1[0], t), kit.lerp(c0[1], c1[1], t), kit.lerp(c0[2], c1[2], t)];
}

/* The fill and the dot color of an hour for a page. A light page takes lab 10's paint as it is, held a step under
   the page's own lightness; a dark page takes the same hue as dim light a step above the page. */
function paint(h, pageL, dark) {
  const lab = hourLab(h);
  if (!dark) {
    const fl = Math.min(kit.lerp(lab[0], 1, 0.08), pageL - 0.04);
    return { fill: toRgb([fl, lab[1] * 0.92, lab[2] * 0.92]), dot: toRgb([Math.min(lab[0] - 0.05, pageL - 0.1), lab[1] * 1.18, lab[2] * 1.18]) };
  }
  const t = kit.clamp((lab[0] - 0.68) / 0.2, 0, 1);
  const fl = pageL + 0.2 + 0.08 * t;
  return { fill: toRgb([fl, lab[1] * 0.8, lab[2] * 0.8]), dot: toRgb([fl + 0.08, lab[1] * 1.1, lab[2] * 1.1]) };
}

/* ---------- words ---------- */

function formatter(opts) {
  try { return new Intl.DateTimeFormat(undefined, opts); } catch (err) { return null; }
}
const HOUR_FMT = formatter({ hour: 'numeric' });
const hourText = (h) => {
  const t = new Date(2000, 0, 1, ((h % 24) + 24) % 24);
  return HOUR_FMT ? HOUR_FMT.format(t) : kit.formatTime(t);
};
const tierWord = (s) => (s >= 0.6 ? 'Most days' : s >= 0.35 ? 'Often' : s >= 0.1 ? 'Sometimes' : 'Rarely');

/* The run of hours around h whose share is at least min (by default, any hour it holds at all), as
   [first, last + 1]. */
function runAround(row, h, min = 0.1) {
  if (row[h] < min) return null;
  let a = h, b = h;
  while (a > 0 && row[a - 1] >= min) a--;
  while (b < 23 && row[b + 1] >= min) b++;
  return [a, b + 1];
}
const spanText = ([a, b]) => `${hourText(a)} to ${hourText(b)}`;

/* The windows of a row for the figure's label: runs of "often" or more, else runs of "sometimes". */
function windows(row) {
  const out = [];
  for (const min of [0.35, 0.1]) {
    for (let h = 0; h < 24; h++) {
      if (row[h] < min || (h > 0 && row[h - 1] >= min)) continue;
      const [a, b] = runAround(row, h, min);
      let top = 0;
      for (let k = a; k < b; k++) top = Math.max(top, row[k]);
      out.push(`${tierWord(top).toLowerCase()} ${spanText([a, b])}`);
    }
    if (out.length) break;
  }
  return out;
}

/* ---------- the model ---------- */

function groupOf(v) {
  if (typeof v !== 'string') return -1;
  const s = v.trim().toLowerCase();
  if (s === 'weekdays' || s === 'weekday') return 0;
  if (s === 'weekends' || s === 'weekend') return 1;
  return -1;
}

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const named = (want, at) => columns.find((c) => typeof c === 'string' && c.trim().toLowerCase() === want) || columns[at];
  const hourCol = named('hour', 0), daysCol = named('days', 1), shareCol = named('share', 2);
  const grid = [new Float32Array(24), new Float32Array(24)];
  let count = 0;
  for (const r of rows) {
    if (!r || typeof r !== 'object') continue;
    const h = Number(r[hourCol]), g = groupOf(r[daysCol]), s = Number(r[shareCol]);
    if (!Number.isInteger(h) || h < 0 || h > 23 || g < 0 || r[shareCol] === null || !Number.isFinite(s)) continue;
    grid[g][h] = kit.clamp(s, 0, 1);
    count += 1;
  }
  return { grid, count, columns, title: out && typeof out.title === 'string' ? out.title.trim() : '' };
}

function ariaLabel(M) {
  if (!M.count) return 'Diurnal profile by hour of the day, with no rows';
  const parts = M.grid.map((row, g) => {
    const w = windows(row);
    return `${WORDS[g]}: ${w.length ? w.join(', ') : 'rarely'}`;
  });
  return `${M.title ? `${M.title.replace(/[.\s]+$/, '')}. ` : ''}A diurnal profile by hour of the day. ${parts.join('. ')}.`;
}

/* ---------- small noise for the light's edge, and grain ---------- */

function hash1(i, seed) {
  let h = Math.imul(i ^ seed, 0x27d4eb2d);
  h ^= h >>> 15; h = Math.imul(h, 0x85ebca6b); h ^= h >>> 13;
  return (h >>> 0) / 4294967296;
}
function noise1(x, seed) {
  const i = Math.floor(x), f = x - i, t = f * f * (3 - 2 * f);
  return kit.lerp(hash1(i, seed), hash1(i + 1, seed), t);
}

/* A Gaussian blur of a 1D array, sigma in samples, edges held. */
function blur(src, sigma) {
  if (sigma < 0.5) return src.slice();
  const R = Math.ceil(sigma * 3), ker = new Float32Array(R * 2 + 1);
  let sum = 0;
  for (let j = -R; j <= R; j++) { const w = Math.exp(-(j * j) / (2 * sigma * sigma)); ker[j + R] = w; sum += w; }
  const n = src.length, out = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    let a = 0;
    for (let j = -R; j <= R; j++) a += src[Math.min(n - 1, Math.max(0, i + j))] * ker[j + R];
    out[i] = a / sum;
  }
  return out;
}
const smooth = (a, b, x) => { const t = kit.clamp((x - a) / (b - a), 0, 1); return t * t * (3 - 2 * t); };

/* ---------- the light, per device pixel ---------- */

/* Renders the band's light into a canvas the size of the plot's light area in device pixels. With bare true only the
   dot tracks are drawn, for the reveal. The canvas is transparent where there is no light, so the band sits on any
   page. */
function renderLight(M, width, k, T, bare) {
  const Wd = Math.max(1, Math.round(width * k)), Hd = Math.max(1, Math.round(LIGHT_H * k));
  const c = document.createElement('canvas');
  c.width = Wd; c.height = Hd;
  const ctx = c.getContext('2d');
  const img = ctx.createImageData(Wd, Hd);
  const px = img.data;
  const L = PAD.left, R = Math.max(L + 1, width - PAD.right), bandW = R - L;
  const pageRgb = T.rgb.page, pageL = toLab(pageRgb)[0], dark = pageL < 0.6;
  const track = T.rgb.borderStrong;

  /* per column: the hour, its paint, each row's strength, glow and edge warp, and the track's ends */
  const fillR = new Float32Array(Wd), fillG = new Float32Array(Wd), fillB = new Float32Array(Wd);
  const dotR = new Float32Array(Wd), dotG = new Float32Array(Wd), dotB = new Float32Array(Wd);
  const trackX = new Float32Array(Wd);
  const ramp = [];
  for (let i = 0; i <= 96; i++) ramp.push(paint(i / 4, pageL, dark));
  const s0 = [new Float32Array(Wd), new Float32Array(Wd)];
  for (let X = 0; X < Wd; X++) {
    const x = (X + 0.5) / k, h = kit.clamp(((x - L) / bandW) * 24, 0, 24);
    const q = h * 4, i = Math.min(95, Math.floor(q)), f = q - i, a = ramp[i], b = ramp[i + 1];
    fillR[X] = kit.lerp(a.fill[0], b.fill[0], f); fillG[X] = kit.lerp(a.fill[1], b.fill[1], f); fillB[X] = kit.lerp(a.fill[2], b.fill[2], f);
    dotR[X] = kit.lerp(a.dot[0], b.dot[0], f); dotG[X] = kit.lerp(a.dot[1], b.dot[1], f); dotB[X] = kit.lerp(a.dot[2], b.dot[2], f);
    trackX[X] = smooth(L - 2, L + 8, x) * (1 - smooth(R - 8, R + 2, x));
    /* the share between hour centers, held at the ends */
    const u = h - 0.5, j = Math.floor(u), t = u - j;
    for (let g = 0; g < 2; g++) {
      const row = M.grid[g];
      const va = row[kit.clamp(j, 0, 23)], vb = row[kit.clamp(j + 1, 0, 23)];
      s0[g][X] = bare ? 0 : kit.lerp(va, vb, t) * (x >= L - 1 && x <= R + 1 ? 1 : 0);
    }
  }
  const pxPerHour = (bandW / 24) * k;
  const S = s0.map((a) => blur(a, 0.3 * pxPerHour));
  const glow = s0.map((a) => blur(a, 1.1 * pxPerHour));
  const warp = [0, 1].map((g) => {
    const w = new Float32Array(Wd);
    for (let X = 0; X < Wd; X++) w[X] = 2.4 * (noise1(X / k / 46 + g * 7.3, 0x51ed) - 0.5) * 2;
    return w;
  });
  /* exp(-2.4 e^2) for e from 0 to 3, the soft edge's profile */
  const LUT = new Float32Array(257);
  for (let i = 0; i <= 256; i++) { const e = (i / 256) * 3; LUT[i] = Math.exp(-2.4 * e * e); }
  const edge = (e) => (e >= 3 ? 0 : LUT[Math.round((e / 3) * 256)]);
  const midY = (ROW_Y[0] + ROW_Y[1]) / 2;

  for (let Y = 0; Y < Hd; Y++) {
    const y = (Y + 0.5) / k;
    const g = y < midY ? 0 : 1;
    const d = Math.abs(y - ROW_Y[g]);
    const glowV = Math.exp(-((Math.max(0, d - CH) / (FY * 1.6)) ** 2));
    const trackV = 1 - smooth(CH + 2, CH + 10, d);
    const sg = S[g], gg = glow[g], wg = warp[g];
    for (let X = 0; X < Wd; X++) {
      const x = (X + 0.5) / k;
      const e = Math.max(0, d - CH - wg[X]) / FY;
      const s = sg[X] * edge(e);
      const gl = gg[X] * glowV;
      const tr = trackV * trackX[X];
      /* the dot screen at 45 degrees: each dot's area follows the strength, and the track keeps a fine dot everywhere */
      const u = ((x + y) * RT) / PITCH, v = ((y - x) * RT) / PITCH;
      const du = u - Math.round(u), dv = v - Math.round(v);
      const dist = Math.sqrt(du * du + dv * dv) * PITCH;
      /* a dot starts a little larger than the track's, so an hour you keep now and then still shows as its own dots */
      const rad = PITCH * (0.1 + 0.3 * Math.sqrt(s > 1 ? 1 : s));
      const cov = kit.clamp(((Math.max(rad, 0.4 * tr) - dist) * k) / 0.8 + 0.5, 0, 1) * Math.max(smooth(0.015, 0.05, s), tr);
      const solid = smooth(0.3, 0.92, s);
      /* over-composite: the glow, then the dots, then the solid light */
      let A = 0, Cr = 0, Cg = 0, Cb = 0;
      const ga = bare ? 0 : 0.2 * gl;
      if (ga > 0.001) { Cr = fillR[X] * ga; Cg = fillG[X] * ga; Cb = fillB[X] * ga; A = ga; }
      if (cov > 0.001) {
        const m = smooth(0, 0.12, s);
        const r0 = kit.lerp(track[0], dotR[X], m), g0 = kit.lerp(track[1], dotG[X], m), b0 = kit.lerp(track[2], dotB[X], m);
        Cr = r0 * cov + Cr * (1 - cov); Cg = g0 * cov + Cg * (1 - cov); Cb = b0 * cov + Cb * (1 - cov); A = cov + A * (1 - cov);
      }
      if (solid > 0.001) {
        Cr = fillR[X] * solid + Cr * (1 - solid); Cg = fillG[X] * solid + Cg * (1 - solid); Cb = fillB[X] * solid + Cb * (1 - solid); A = solid + A * (1 - solid);
      }
      const o = (Y * Wd + X) * 4;
      if (A <= 0.002) { px[o + 3] = 0; continue; }
      const grain = (hash1(X * 7919 + Y * 104729, 0x2b) - 0.5) * 3;
      px[o] = Cr / A + grain; px[o + 1] = Cg / A + grain; px[o + 2] = Cb / A + grain;
      px[o + 3] = Math.round(A * 255);
    }
  }
  ctx.putImageData(img, 0, 0);
  return c;
}

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-band');
  el.appendChild(root);
  const parts = kit.block(root, {
    plotHeight: HEIGHT, readout: true, foot: opts.foot !== false,
    hint: 'Use the left and right arrow keys to read each hour, and up and down to change between weekdays and weekends.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let full = null, bare = null, cacheKey = '';
  let hover = null;                                  /* { g, h } */
  let hoverFrom = null;
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let clock = 0, revealAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  const geo = () => {
    const b = box(width);
    return { L: b.left, R: b.right, bottom: b.bottom, bandW: Math.max(1, b.right - b.left) };
  };
  const xOfHour = (h) => { const G = geo(); return G.L + (h / 24) * G.bandW; };

  function bitmaps() {
    const k = canvas.width / Math.max(1, width);
    const key = `${width}|${k}|${tokens.signature}`;
    if (key !== cacheKey || !full) { full = renderLight(M, width, k, tokens, false); bare = null; cacheKey = key; }
    if (mode === 'live' && reveal < Infinity && !bare) bare = renderLight(M, width, k, tokens, true);
  }

  /* ---------- drawing ---------- */
  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const T = tokens, G = geo();
    const sx = canvas.width / width;
    const labelA = mode === 'live' && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    bitmaps();

    /* the light: whole, or arriving behind the frame's sweep with a soft edge */
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    if (mode === 'live' && reveal < Infinity) {
      const rx = Math.max(0, Math.round(reveal * sx));
      /* the dot tracks wait right of the dot; the light, which carries its own tracks, fills in left of it */
      if (bare && rx < bare.width) ctx.drawImage(bare, rx, 0, bare.width - rx, bare.height, rx, 0, bare.width - rx, bare.height);
      if (rx > 0) ctx.drawImage(full, 0, 0, Math.min(rx, full.width), full.height, 0, 0, Math.min(rx, full.width), full.height);
      const feather = Math.round(24 * sx), stepW = Math.max(1, Math.round(feather / 6));
      for (let i = 0; i < 6; i++) {
        const x = rx + i * stepW;
        if (x >= full.width) break;
        ctx.globalAlpha = 1 - (i + 0.5) / 6;
        const w = Math.min(stepW, full.width - x);
        ctx.drawImage(full, x, 0, w, full.height, x, 0, w, full.height);
      }
      ctx.globalAlpha = 1;
    } else ctx.drawImage(full, 0, 0);
    ctx.restore();

    /* the hour axis: a fine dot at each hour, and words every few hours */
    ctx.save();
    ctx.globalAlpha = labelA;
    ctx.fillStyle = T.borderStrong;
    ctx.beginPath();
    for (let h = 0; h <= 24; h++) { const x = xOfHour(h); ctx.moveTo(x + 0.9, G.bottom); ctx.arc(x, G.bottom, 0.9, 0, TAU); }
    ctx.fill();
    const every = [3, 6, 12].find((s) => (s * G.bandW) / 24 >= 64) || 12;
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textAlign = 'center'; ctx.textBaseline = 'alphabetic';
    for (let h = every; h < 24; h += every) ctx.fillText(hourText(h), xOfHour(h), HEIGHT - 6);
    /* the row names */
    ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    WORDS.forEach((w, g) => {
      ctx.fillStyle = hover && hover.g === g ? T.ink : T.muted;
      ctx.fillText(w, G.L - 10, ROW_Y[g]);
    });
    ctx.restore();

    /* the hour you read: a hairline through its row's light, and a dot on the axis */
    if (hover) {
      const x = xOfHour(hover.h + 0.5), yc = ROW_Y[hover.g];
      const P = T.rgb.primary;
      ctx.save();
      ctx.strokeStyle = kit.rgba(P, P[3] * 0.55); ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(Math.round(x * sx) / sx + 0.5 / sx, yc - CH - FY); ctx.lineTo(Math.round(x * sx) / sx + 0.5 / sx, yc + CH + FY); ctx.stroke();
      ctx.fillStyle = T.primary;
      ctx.beginPath(); ctx.arc(x, G.bottom, 1.8, 0, TAU); ctx.fill();
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    if (!hover) { kit.setReadout(parts.readout, M.count ? [{ label: 'How often, at each hour of the day' }] : []); return; }
    const row = M.grid[hover.g], s = row[hover.h];
    const at = [{ when: `${WORDS[hover.g]}, ${hourText(hover.h)}` }, { value: tierWord(s) }];
    /* the window around this hour, cut the way the figure's label cuts it: often or more, else at all */
    const run = runAround(row, hover.h, s >= 0.35 ? 0.35 : 0.1);
    if (run) at.push({ label: spanText(run) });
    kit.setReadout(parts.readout, at);
  }
  function setHover(next, from) {
    const same = (a, b) => (a === null && b === null) || (a && b && a.g === b.g && a.h === b.h);
    hoverFrom = from;
    if (same(next, hover)) return;
    hover = next;
    readout();
    draw();
  }
  const peakHour = (g) => { const row = M.grid[g]; let p = 0; for (let h = 1; h < 24; h++) if (row[h] > row[p]) p = h; return p; };

  /* ---------- the reveal the frame drives ---------- */
  function rowsWait(on) {
    for (const node of [parts.readout, parts.foot]) {
      if (!node) continue;
      node.classList.toggle('vk-fade', !on && !reduced());
      node.classList.toggle('vk-wait', on);
    }
  }
  function setReveal(x) {
    if (mode === 'none' && x === Infinity) return;
    if (mode === 'none') { mode = 'live'; revealAt = clock; }
    reveal = reduced() ? Infinity : x;
    rowsWait(reveal < Infinity);
    if (reveal === Infinity) bare = null;
    loop.wake();
  }
  function step(dt) {
    clock += dt;
    draw();
    return mode === 'live' && !reduced() && clock - revealAt < 0.35;
  }

  /* ---------- pointer and keys ---------- */
  function pick(px, py) {
    const G = geo();
    if (py < 0 || py > G.bottom + 10 || px > G.R + 6 || px < 0) return null;
    const g = py < (ROW_Y[0] + ROW_Y[1]) / 2 ? 0 : 1;
    if (px < G.L) return { g, h: peakHour(g) };
    return { g, h: kit.clamp(Math.floor(((px - G.L) / G.bandW) * 24), 0, 23) };
  }
  const onMove = (e) => {
    if (!width || !M.count) return;
    const r = canvas.getBoundingClientRect();
    const hit = pick(e.clientX - r.left, e.clientY - r.top);
    if (!hit) { if (hoverFrom === 'pointer') setHover(null, null); return; }
    setHover(hit, 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onMove);
  parts.plot.addEventListener('pointerleave', onLeave);
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey || !M.count) return;
    let next = hover ? { ...hover } : null;
    const start = () => {
      /* the first key lands on the hour it most often holds */
      const g = M.grid[1][peakHour(1)] > M.grid[0][peakHour(0)] ? 1 : 0;
      return { g, h: peakHour(g) };
    };
    switch (e.key) {
      case 'ArrowLeft': next = next ? { ...next, h: Math.max(0, next.h - 1) } : start(); break;
      case 'ArrowRight': next = next ? { ...next, h: Math.min(23, next.h + 1) } : start(); break;
      case 'ArrowUp': next = next ? { ...next, g: 0 } : start(); break;
      case 'ArrowDown': next = next ? { ...next, g: 1 } : start(); break;
      case 'Home': next = { g: next ? next.g : 0, h: 0 }; break;
      case 'End': next = { g: next ? next.g : 0, h: 23 }; break;
      case 'Escape':
        if (!hover) return;
        e.preventDefault(); setHover(null, 'key'); return;
      default: return;
    }
    e.preventDefault();
    setHover(next, 'key');
  };
  const onBlur = () => { if (hoverFrom === 'key') setHover(null, null); };
  parts.plot.addEventListener('keydown', onKey);
  parts.plot.addEventListener('blur', onBlur);

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    full = null; bare = null; cacheKey = '';
    hover = null; hoverFrom = null;
    kit.setLabel(parts.plot, ariaLabel(M));
    readout();
    kit.foot(parts.foot, out, opts);
    if (mode === 'live') rowsWait(reveal < Infinity);
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => { width = w; draw(); });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; draw(); }
  });
  const stopDpr = kit.watchDpr(() => draw());
  const stopMotion = kit.watchReducedMotion(() => { if (reduced() && reveal < Infinity) setReveal(Infinity); else loop.wake(); });
  apply();
  if (mode === 'live') { revealAt = clock; if (reduced()) setReveal(Infinity); loop.wake(); }

  return {
    plot: parts.plot,
    update(next, patch) {
      if (patch) opts = { ...opts, ...patch };
      out = next || {};
      apply();
    },
    setReveal,
    destroy() {
      loop.stop(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerdown', onMove);
      parts.plot.removeEventListener('pointerleave', onLeave);
      parts.plot.removeEventListener('keydown', onKey);
      parts.plot.removeEventListener('blur', onBlur);
      root.remove();
    },
  };
}
