// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/ridges.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* ridges.js: a ridgeline plot. A value by day and position in the day, each day stacked as a ridge of dots, so the
   shape of many days shows at once and a repeat or a change across them reads down the stack.

   A new chart mark, `mark: "ridgeline"`, from two kept tiles: "Thirty nights of sleep stacked as ridges" (lab 8) and
   "Ten weeks of walking stacked as ridges" (lab 12). Both were WebGL. This is canvas 2D with a capped number of dots.
   - Three columns, in order: the day each row of dots belongs to (a date), the position in that day, and the value.
     The position column is named `hour` (the hour of day, 0 to 24, where 6.5 is 6:30) or `minute` (minutes since
     the row began, such as since you fell asleep). Each query row is one step of a day: where it starts, and its
     value. The steps sit on a regular grid, which the chart finds from the rows. A step the query leaves out, or a
     null, is drawn at zero and reads as "-".
   - The value is a number of zero or more (such as steps in the hour), drawn against the largest value in the result, or a
     sleep stage, drawn at a fixed height: deep 1, core and asleep 0.55, REM 0.35, awake and in bed 0 (STAGES).
   - Each day is one ridge: a dotted line along its top and a stipple under it, laid over a fill in the page color,
     so a nearer ridge hides the ones behind it. The earliest day is at the back and the latest at the front.
   - Sort by date or by weekday. The rows slide to their new places with the front rows first, and the row labels
     change once the rows have landed.
   - Rest on a ridge, or tab to the chart and use the arrow keys, to lift it out and read its date, the step and its
     value. Up and down move between rows and left and right move along one. The rows in front of it fade.
   - One accent: every ridge is the primary, lighter toward the back. Nothing marks a better or a worse day.
   Block: readout 26 + plot 320 + foot 26 = 372px. The plot is taller than a chart's 200px because ten weeks of
   rows need about 4px each to stay separate lines, and the back row needs room above it for its ridge to rise.
   Draw cost: a frame's cost is the round dots it rasterizes. Each ridge's dots are built once into paths and moved,
   never rebuilt, while rows slide or lift. The dot pitch is a whole number of device pixels, so a ridge's flat
   stretches are stamped from one pattern of dots and only its rising parts are drawn dot by dot. The stipple is
   held to one budget for the whole chart, smaller on a phone. Ten weeks of walking draw about 14,600 dots a frame
   at 640px (7,800 of them one by one) and 6,700 at a phone's width (3,500). Nothing runs while nothing moves.

   Contract: see the top of kit.js. The chart draws the ink ends of its dotted left rule at (box.left, box.top) and
   (box.left, box.bottom). */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'ridges', kinds: ['chart'], marks: ['ridgeline'] });

/* The sleep stages HealthKit records, and the height each one is drawn at: deeper sleep stands taller. show.rs
   writes the names short (asleep_deep becomes deep); the chart reads either. */
export const STAGES = Object.freeze({
  deep: Object.freeze({ height: 1, word: 'Deep' }),
  core: Object.freeze({ height: 0.55, word: 'Core' }),
  asleep: Object.freeze({ height: 0.55, word: 'Asleep' }),
  rem: Object.freeze({ height: 0.35, word: 'REM' }),
  awake: Object.freeze({ height: 0, word: 'Awake' }),
  in_bed: Object.freeze({ height: 0, word: 'In bed' }),
});

const HEIGHT = 320;
const PAD = Object.freeze({ top: 10, right: 8, bottom: 22, left: 44 });
const INSET = 6;             /* the rows start this far right of the left rule */
const END_R = 1.75;          /* the ink dots at the top and the foot of the left rule */
const DOT_R = 0.7, STIP_R = 0.6, HOT_R = 1;
const SLIDE = 0.6;           /* a row's slide when the order changes, s */
const LIFT_D = 0.28, LIFT_B = 0.12, HOT_D = 0.22, FADE_D = 0.25;
const FADE_TO = 0.35;        /* the rows in front of the one you rest on */
const GROW = 0.5, GROW_B = 0.1;
const TAU = Math.PI * 2;

const CSS = `
.vk-ridges .vk-ridges-sort{display:inline-flex;align-items:center;gap:6px;margin-left:auto;flex:none}
.vk-ridges .vk-ridges-sort.vk-ridges-top{position:absolute;top:0;right:0;height:20px}
.vk-ridges .vk-ridges-sort button{all:unset;cursor:pointer;font:inherit;font-size:12px;line-height:18px;padding:0 2px;border-radius:2px;color:var(--vk-primary);white-space:nowrap}
.vk-ridges .vk-ridges-sort button[aria-pressed="true"]{color:var(--vk-ink);cursor:default}
.vk-ridges .vk-ridges-sort button[aria-pressed="true"]::before{content:'';display:inline-block;width:4px;height:4px;margin:0 5px 0 0;border-radius:50%;background:currentColor;vertical-align:middle;position:relative;top:-1px}
.vk-ridges .vk-ridges-sort button[aria-pressed="false"]:hover{text-decoration:underline;text-underline-offset:3px;text-decoration-thickness:1px}
.vk-ridges .vk-ridges-sort button:focus-visible{outline:2px solid var(--vk-focus);outline-offset:1px}
`;

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

/* ---------- words ---------- */

function formatter(opts) {
  try { return new Intl.DateTimeFormat(undefined, opts); } catch (err) { return null; }
}
const DAY_FMT = formatter({ weekday: 'short', month: 'short', day: 'numeric' });
const WEEKDAY_FMT = formatter({ weekday: 'short' });
const HOUR_FMT = formatter({ hour: 'numeric' });
const TIME_FMT = formatter({ hour: 'numeric', minute: '2-digit' });

const dayText = (t) => (DAY_FMT ? DAY_FMT.format(t) : kit.formatTime(t, { withDate: true, withTime: false }));
const dateText = (t) => kit.formatTime(t, { withDate: true, withTime: false });
const weekdayText = (t) => (WEEKDAY_FMT ? WEEKDAY_FMT.format(t) : ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'][t.getDay()]);

/* A clock position: 6 is "6 AM", 6.5 "6:30 AM", in the reader's own clock format. */
function clockText(h) {
  const m = Math.round(((h % 24) + 24) % 24 * 60);
  const t = new Date(2000, 0, 1, Math.floor(m / 60), m % 60);
  if (m % 60) return TIME_FMT ? TIME_FMT.format(t) : kit.formatTime(t);
  return HOUR_FMT ? HOUR_FMT.format(t) : kit.formatTime(t);
}
/* Time since the row began: "+45 m", "+1 h 30 m". */
function sinceText(min) {
  const m = Math.max(0, Math.round(min));
  const h = Math.floor(m / 60), r = m % 60;
  if (!h) return `+${r} m`;
  return r ? `+${h} h ${r} m` : `+${h} h`;
}

/* ---------- the model: rows into days, days onto one grid ---------- */

function stageOf(v) {
  if (typeof v !== 'string') return null;
  let s = v.trim().toLowerCase().replace(/[\s-]+/g, '_');
  if (s.startsWith('asleep_')) s = s.slice(7);
  if (s === 'inbed') s = 'in_bed';
  return Object.prototype.hasOwnProperty.call(STAGES, s) ? s : null;
}

const pad2 = (n) => String(n).padStart(2, '0');
const ymd = (t) => `${t.getFullYear()}-${pad2(t.getMonth() + 1)}-${pad2(t.getDate())}`;

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const [keyCol, posCol, valCol] = columns;
  const unit = typeof posCol === 'string' && /^minutes?$/i.test(posCol.trim()) ? 'minute' : 'hour';
  const byDay = new Map();
  let stages = 0, numbers = 0;
  for (const r of rows) {
    if (!r || typeof r !== 'object') continue;
    const t = kit.parseTime(r[keyCol]);
    if (!t) continue;
    const pr = r[posCol];
    const p = typeof pr === 'number' ? pr : typeof pr === 'string' && pr.trim() ? Number(pr) : NaN;
    if (!Number.isFinite(p)) continue;
    const raw = r[valCol];
    let v = null, stage = null;
    if (typeof raw === 'number' && Number.isFinite(raw)) { v = Math.max(0, raw); numbers += 1; }
    else if (typeof raw === 'string' && raw.trim()) {
      stage = stageOf(raw);
      if (stage) { v = STAGES[stage].height; stages += 1; }
      else if (Number.isFinite(Number(raw))) { v = Math.max(0, Number(raw)); numbers += 1; }
    }
    const key = ymd(t);
    let d = byDay.get(key);
    if (!d) { d = { key, t: new Date(t.getFullYear(), t.getMonth(), t.getDate()), steps: [] }; byDay.set(key, d); }
    d.steps.push({ p, v, stage });
  }
  const days = [...byDay.values()].sort((a, b) => a.t - b.t);
  const isStage = stages > 0 && stages >= numbers;

  /* The grid: the most common gap between neighboring steps in a row, so a missing step reads as a gap. */
  const gaps = new Map();
  for (const d of days) {
    d.steps.sort((a, b) => a.p - b.p);
    for (let i = 1; i < d.steps.length; i++) {
      const g = Math.round((d.steps[i].p - d.steps[i - 1].p) * 1e6) / 1e6;
      if (g > 0) gaps.set(g, (gaps.get(g) || 0) + 1);
    }
  }
  let step = 0, seen = 0;
  for (const [g, c] of gaps) if (c > seen || (c === seen && g < step)) { step = g; seen = c; }
  if (!step) step = unit === 'minute' ? 15 : 1;

  let lo = Infinity, hi = -Infinity, vmax = 0;
  for (const d of days) for (const s of d.steps) {
    lo = Math.min(lo, s.p); hi = Math.max(hi, s.p + step);
    if (s.v !== null && s.v > vmax) vmax = s.v;
  }
  if (!days.length) { lo = 0; hi = unit === 'minute' ? 60 : 24; }
  const whole = unit === 'minute' ? 60 : 1;      /* the axis runs over whole hours */
  lo = Math.floor(lo / whole + 1e-9) * whole;
  hi = Math.max(lo + whole, Math.ceil(hi / whole - 1e-9) * whole);
  if (unit === 'hour') { lo = Math.max(0, lo); hi = Math.min(24, Math.max(hi, lo + 1)); }
  /* a day holds at most 1,440 steps, a minute each, whatever gap the rows suggest */
  if ((hi - lo) / step > 1440) step = (hi - lo) / 1440;
  const nb = Math.max(1, Math.round((hi - lo) / step));
  if (isStage) vmax = 1;
  if (!(vmax > 0)) vmax = 1;

  for (const d of days) {
    const vals = new Float32Array(nb), has = new Uint8Array(nb), stageAt = new Array(nb).fill(null);
    for (const s of d.steps) {
      const k = Math.round((s.p - lo) / step);
      if (k < 0 || k >= nb || s.v === null) continue;
      if (!has[k] || s.v > vals[k]) { vals[k] = s.v; stageAt[k] = s.stage; }
      has[k] = 1;
    }
    let peak = 0;
    for (let k = 1; k < nb; k++) if (vals[k] > vals[peak]) peak = k;
    d.vals = vals; d.has = has; d.stageAt = stageAt; d.peak = peak;
    d.grp = (d.t.getDay() + 6) % 7;            /* Monday 0 ... Sunday 6 */
    d.curve = pchip(vals, vmax);
  }
  return {
    columns, keyCol, posCol, valCol, unit, isStage, days, n: days.length, step, lo, hi, nb, vmax,
    valueLabel: kit.columnLabel(valCol || 'value'),
  };
}

/* A monotone cubic through the step centers, with zero half a step outside the grid, so a ridge rises from its
   baseline and never overshoots a value: a lone tall hour stays exactly as tall as it is. u is in steps from lo. */
function pchip(vals, vmax) {
  const nb = vals.length, N = nb + 2;
  const y = new Float64Array(N), m = new Float64Array(N);
  for (let i = 0; i < nb; i++) y[i + 1] = vals[i] / vmax;
  for (let i = 1; i < N - 1; i++) {
    const a = y[i] - y[i - 1], b = y[i + 1] - y[i];
    m[i] = a * b <= 0 ? 0 : (2 * a * b) / (a + b);
  }
  return (u) => {
    const t = u + 0.5, i = Math.floor(t);
    if (i < 0 || i >= N - 1) return 0;
    const s = t - i, s2 = s * s, s3 = s2 * s;
    const v = (2 * s3 - 3 * s2 + 1) * y[i] + (s3 - 2 * s2 + s) * m[i] + (3 * s2 - 2 * s3) * y[i + 1] + (s3 - s2) * m[i + 1];
    return v > 0 ? v : 0;
  };
}

/* A Gaussian blur of one ridge's samples, sigma in samples, edges at zero. */
function blur(src, sigma) {
  const R = Math.ceil(sigma * 3), ker = new Float32Array(R * 2 + 1);
  let sum = 0;
  for (let j = -R; j <= R; j++) { const w = Math.exp(-(j * j) / (2 * sigma * sigma)); ker[j + R] = w; sum += w; }
  const n = src.length, out = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    let a = 0;
    for (let j = Math.max(-R, -i); j <= R && i + j < n; j++) a += src[i + j] * ker[j + R];
    out[i] = a / sum;
  }
  return out;
}

/* A seeded random, so a ridge's stipple is the same on every draw and every width. */
function rand(seed) {
  let s = (seed >>> 0) % 2147483647 || 1;
  return () => { s = (s * 16807) % 2147483647; return (s - 1) / 2147483646; };
}
function hashKey(str) {
  let x = 2166136261;
  for (let i = 0; i < str.length; i++) { x ^= str.charCodeAt(i); x = Math.imul(x, 16777619); }
  return x >>> 0;
}

function ariaLabel(M, sort) {
  if (!M.n) return 'Ridgeline plot with no rows';
  const what = M.isStage ? 'sleep stage' : M.valueLabel;
  const along = M.unit === 'hour' ? 'by hour of day' : 'by time since each row began';
  const range = M.n > 1 ? `from ${dateText(M.days[0].t)} to ${dateText(M.days[M.n - 1].t)}` : `for ${dateText(M.days[0].t)}`;
  const order = sort === 'weekday' ? 'grouped by weekday' : 'in date order with the latest in front';
  return `Ridgeline plot: ${what} ${along}, one row a day ${range}, ${order}. Taller is ${M.isStage ? 'deeper sleep' : 'more'}.`;
}

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('ridges', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-ridges');
  el.appendChild(root);
  const parts = kit.block(root, {
    plotHeight: HEIGHT, readout: true, foot: opts.foot !== false,
    hint: 'Use the up and down arrow keys to move between days, and left and right to read along one.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);

  /* the order control: in the foot beside See the rows, or over the readout's right end without a foot */
  const sortEl = kit.h('div', 'vk-ridges-sort', { role: 'group', 'aria-label': 'Order the rows' });
  const sortButtons = [['date', 'By date'], ['weekday', 'By weekday']].map(([v, text]) => {
    const b = kit.h('button', null, { type: 'button', 'data-sort': v, 'aria-pressed': String(v === 'date') });
    b.textContent = text;
    sortEl.appendChild(b);
    return b;
  });
  const onSortClick = (e) => {
    const b = e.target.closest('button[data-sort]');
    if (b) setSort(b.dataset.sort);
  };
  sortEl.addEventListener('click', onSortClick);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let G = null;                                      /* geometry for the current width */
  let sort = 'date';
  let base = [], target = [], goAt = [];             /* each row's baseline spring, its target and when it moves */
  let lift = [], hot = [], fade = [];
  let hover = null, hoverStep = 0, hoverFrom = null;
  let ring = { x: 1, v: 0 };
  let clock = 0;
  let labelsFrom = 'date', labelsTo = 'date', labelsOut = 0, labelsIn = 0;
  /* The reveal: 'none' draws everything at once; 'live' raises the ridges as the frame's sweep passes them. */
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = null, revealAt = 0, grown = false;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  function slots(order) {
    /* the visual order, back to front, for a sort */
    const idx = M.days.map((_, i) => i);
    if (order === 'weekday') idx.sort((a, b) => M.days[a].grp - M.days[b].grp || M.days[a].t - M.days[b].t);
    return idx;
  }
  function layout() {
    const b = box(width);
    const L = b.left, R = b.right, top = b.top, bottom = b.bottom;
    const D = kit.dpr();
    const x0 = Math.round((L + INSET) * D) / D, plotW = Math.max(1, R - x0);
    const n = M.n, span = bottom - top;
    let amp = 40, pitch = 0;
    for (let i = 0; i < 4; i++) { pitch = n > 1 ? (span - amp) / (n - 1) : 0; amp = kit.clamp(7 * pitch, 18, 56); }
    if (n <= 1) { amp = 56; pitch = 0; }
    const y0 = bottom - (n > 1 ? pitch * (n - 1) : 0);
    /* baselines for both orders */
    const dateOrder = slots('date'), wdOrder = slots('weekday');
    const at = { date: new Array(n), weekday: new Array(n) };
    dateOrder.forEach((r, s) => { at.date[r] = y0 + s * pitch; });
    const groups = [...new Set(wdOrder.map((r) => M.days[r].grp))];
    let gapW = groups.length > 1 ? kit.clamp(pitch * 1.4, 5, 12) : 0;
    let pitchW = n > 1 ? (bottom - y0 - (groups.length - 1) * gapW) / (n - 1) : 0;
    if (n > 1 && pitchW < pitch * 0.6) { gapW = Math.max(0, (bottom - y0 - pitch * 0.6 * (n - 1)) / Math.max(1, groups.length - 1)); pitchW = (bottom - y0 - (groups.length - 1) * gapW) / (n - 1); }
    wdOrder.forEach((r, s) => { at.weekday[r] = y0 + s * pitchW + groups.indexOf(M.days[r].grp) * gapW; });
    /* The draw cost is the number of round dots a frame rasterizes. A phone gets a wider pitch and a smaller stipple,
       and the pitch is a whole number of device pixels, so the flat stretches of a ridge can be stamped from one
       pattern of dots (see flatRuns) and only the rising parts are drawn dot by dot. */
    const narrow = width < 480;
    const stepD = Math.max(3, Math.round(((n > 50 ? 2.8 : 2.6) + (narrow ? 0.3 : 0)) * D));
    const lineStep = stepD / D;
    const nDots = Math.floor((plotW + 0.01) / lineStep) + 1;
    const stipBudget = (narrow ? 3000 : 8000) * (n > 50 ? 0.8 : 1);
    const cand = n ? Math.min(1400, Math.floor((stipBudget * 4) / n)) : 0;
    const stipDepth = kit.clamp(amp * 0.28, 6, 16);
    const S = Math.floor(plotW) + 1;
    /* A sleep stage's height is a code, so its steps are softened by half a step, as lab 8 softened its minutes
       by 7.5. A number keeps the monotone curve alone, so a lone tall hour keeps its true height. */
    const soften = M.isStage ? 0.5 * (plotW / M.nb) : 0;
    const profs = M.days.map((d) => {
      const p = new Float32Array(S);
      for (let i = 0; i < S; i++) p[i] = d.curve((i / plotW) * M.nb);
      return soften >= 1 ? blur(p, soften) : p;
    });
    /* the stipple: candidates along the row, kept with a chance that grows with the ridge's height, deeper ones rarer */
    const stip = M.days.map((d, r) => {
      const rnd = rand(hashKey(d.key) ^ 0x9e3779b9);
      const pts = [];
      for (let j = 0; j < cand; j++) {
        const f = (j + rnd()) / cand, hgt = profs[r][Math.min(S - 1, Math.round(f * (S - 1)))];
        const keep = rnd(), u = 1 - Math.sqrt(1 - rnd()), w = rnd();
        if (hgt > 0.04 && keep < Math.pow(hgt, 1.6) * 0.9) pts.push(f, u, w);
      }
      return new Float32Array(pts);
    });
    G = { L, R, top, bottom, x0, plotW, S, n, amp, pitch, y0, at, dateOrder, wdOrder, groups, D, stepD, lineStep, nDots, stipDepth,
      profs, stip, lift: kit.clamp(amp * 0.22, 5, 10), cache: new Array(n).fill(null), hotCache: new Array(n).fill(null), patterns: {} };
  }

  /* One period of the dotted line as a pattern, in device pixels: a dot of radius r in the primary, one pitch apart.
     A flat stretch of a ridge is one rectangle filled with it. Rows sit at fractional device pixels, so the dot is
     drawn at four vertical phases, a quarter of a device pixel apart, and a row takes the nearest one. */
  function dotPattern(ctx, r, phase) {
    const key = `${r}|${phase}|${tokens.signature}`;
    if (G.patterns[key]) return G.patterns[key];
    const stepD = G.stepD, hD = 2 * Math.ceil(r * G.D + 2);
    const c = document.createElement('canvas');
    c.width = stepD; c.height = hD;
    const g = c.getContext('2d');
    g.fillStyle = kit.rgba(tokens.rgb.primary, 1);
    g.beginPath(); g.arc(stepD / 2, hD / 2 + phase / 4, r * G.D, 0, TAU); g.fill();
    G.patterns[key] = { pattern: ctx.createPattern(c, 'repeat'), hD };
    return G.patterns[key];
  }
  const xOfStep = (k) => G.x0 + ((k + 0.5) / M.nb) * G.plotW;

  /* The paths of one ridge, with its baseline at y = 0. `grow` scales each sample's height during the reveal. */
  function buildPaths(r, dotR, grow) {
    const prof = G.profs[r], amp = G.amp, S = G.S, x0 = G.x0, plotW = G.plotW;
    const hAt = (f) => {
      const i = Math.min(S - 1, Math.max(0, Math.round(f * (S - 1))));
      return prof[i] * (grow ? grow[i] : 1);
    };
    const fill = new Path2D();
    fill.moveTo(x0, 2);
    for (let i = 0; i < S; i += 3) fill.lineTo(x0 + i, -prof[i] * amp * (grow ? grow[i] : 1));
    fill.lineTo(x0 + plotW, -prof[S - 1] * amp * (grow ? grow[S - 1] : 1));
    fill.lineTo(x0 + plotW, 2);
    fill.closePath();
    /* The dotted line along the top: dot by dot where the ridge rises, and as runs of dot indices where it lies
       within 0.3px of its baseline, which no dot can show. */
    const line = new Path2D(), flatRuns = [];
    let runFrom = -1;
    for (let k = 0; k < G.nDots; k++) {
      const x = k * G.lineStep, hh = hAt(x / plotW) * amp;
      if (hh < 0.3) { if (runFrom < 0) runFrom = k; continue; }
      if (runFrom >= 0) { flatRuns.push(runFrom, k - 1); runFrom = -1; }
      line.moveTo(x0 + x + dotR, -hh); line.arc(x0 + x, -hh, dotR, 0, TAU);
    }
    if (runFrom >= 0) flatRuns.push(runFrom, G.nDots - 1);
    const sr = dotR * (STIP_R / DOT_R);
    const stip = [new Path2D(), new Path2D(), new Path2D()];
    const pts = G.stip[r];
    for (let j = 0; j < pts.length; j += 3) {
      const f = pts[j], hh = hAt(f) * amp;
      if (hh < 1) continue;
      const y = -hh + pts[j + 1] * Math.min(G.stipDepth, hh);
      const a = (0.62 + 0.26 * pts[j + 2]) * Math.min(1, hh / 8);
      const bucket = a < 0.5 ? 0 : a < 0.7 ? 1 : 2;
      const x = x0 + f * plotW;
      stip[bucket].moveTo(x + sr, y); stip[bucket].arc(x, y, sr, 0, TAU);
    }
    return { fill, line, flatRuns, stip };
  }
  function pathsFor(r, isHot, grow) {
    if (grow) return buildPaths(r, isHot ? HOT_R : DOT_R, grow);
    const cache = isHot ? G.hotCache : G.cache;
    if (!cache[r]) cache[r] = buildPaths(r, isHot ? HOT_R : DOT_R, null);
    return cache[r];
  }

  /* ---------- the reveal the frame drives ---------- */
  function growth() {
    if (mode !== 'live' || grown || !G) return null;
    if (!born || born.length !== G.S) born = new Float64Array(G.S).fill(-1);
    const g = new Float32Array(G.S);
    let all = true;
    for (let i = 0; i < G.S; i++) {
      if (born[i] < 0 && (reveal === Infinity || G.x0 + i <= reveal)) born[i] = clock;
      if (born[i] < 0) { all = false; continue; }
      const t = clock - born[i];
      g[i] = reduced() ? 1 : Math.max(0, kit.stepSpring(t, GROW, GROW_B));
      if (t < 1.2 && !reduced()) all = false;
    }
    if (all) { grown = true; return null; }
    return g;
  }
  function rowsWait(on) {
    for (const node of [parts.readout, parts.foot]) {
      if (!node) continue;
      node.classList.toggle('vk-fade', !on && !reduced());
      node.classList.toggle('vk-wait', on);
    }
  }
  function setReveal(x) {
    if (mode === 'none' && x === Infinity) return;
    if (mode === 'none') { mode = 'live'; revealAt = clock; grown = false; born = null; }
    reveal = x;
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  /* Hour ticks at least 70px apart: every 2 hours over a 16-hour day at 640px, every 6 on a phone. */
  function axisTicks() {
    const span = M.hi - M.lo, per = M.unit === 'minute' ? 60 : 1;
    const perHour = (G.plotW / span) * per;
    const every = [1, 2, 3, 4, 6, 8, 12].find((s) => s * perHour >= 70) || 12;
    const ticks = [];
    const first = Math.ceil(M.lo / per / every) * every;
    for (let h = first; h * per <= M.hi + 1e-9; h += every) {
      const pos = h * per;
      ticks.push({ x: G.x0 + ((pos - M.lo) / span) * G.plotW, text: M.unit === 'minute' ? (h === 0 ? '0' : `+${h} h`) : clockText(pos) });
    }
    return ticks;
  }

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    if (!M.n) return;
    if (!G) layout();
    const T = tokens, P = T.rgb.primary;
    const sx = canvas.width / width, sy = canvas.height / HEIGHT;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;

    /* the hour axis under the front row */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(G.x0, G.bottom + 5, G.plotW, 1);
    ctx.globalAlpha = labelA;
    ctx.font = `9.5px ${T.mono}`; ctx.textBaseline = 'alphabetic';
    for (const tk of axisTicks()) {
      ctx.fillStyle = T.borderStrong;
      ctx.beginPath(); ctx.arc(tk.x, G.bottom + 5.5, 1.1, 0, TAU); ctx.fill();
      ctx.fillStyle = T.muted;
      ctx.textAlign = tk.x - G.x0 < 14 ? 'left' : G.x0 + G.plotW - tk.x < 14 ? 'right' : 'center';
      ctx.fillText(tk.text, tk.x, HEIGHT - 4);
    }
    ctx.restore();

    /* the left rule: fine dots from the foot to the top with an ink dot at each end */
    dotted(ctx, G.L, G.bottom, G.L, G.top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath();
    ctx.moveTo(G.L + END_R, G.top); ctx.arc(G.L, G.top, END_R, 0, TAU);
    ctx.moveTo(G.L + END_R, G.bottom); ctx.arc(G.L, G.bottom, END_R, 0, TAU);
    ctx.fill();

    /* Row labels: in date order, the date at each Monday's row (every row when there are few), and the row you rest
       on in ink; by weekday, the day's name at the middle of its group. They change once the rows have landed. */
    const outA = 1 - kit.seg(clock, labelsOut, labelsOut + 0.15), inA = kit.seg(clock, labelsIn, labelsIn + 0.25);
    const setA = (set) => (labelsFrom === labelsTo ? (set === labelsTo ? 1 : 0) : set === labelsTo ? inA : set === labelsFrom ? outA : 0);
    ctx.save();
    ctx.font = `9.5px ${T.mono}`; ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    const dA = setA('date') * labelA, wA = setA('weekday') * labelA;
    if (dA > 0.001) {
      ctx.globalAlpha = dA;
      const hasMonday = M.days.some((d) => d.grp === 0);
      const hy = hover !== null ? base[hover].x - lift[hover].x : null;
      M.days.forEach((d, r) => {
        const y = G.at.date[r];
        if (r === hover || !(M.n <= 12 || (hasMonday ? d.grp === 0 : r % 7 === 0))) return;
        if (hy !== null && Math.abs(y - hy) < 12) return;
        ctx.fillStyle = T.muted;
        ctx.fillText(dateText(d.t), G.L - 6, y);
      });
      if (hover !== null) {
        ctx.fillStyle = T.ink;
        ctx.fillText(dateText(M.days[hover].t), G.L - 6, base[hover].x - lift[hover].x);
      }
    }
    if (wA > 0.001) {
      ctx.globalAlpha = wA;
      for (const g of G.groups) {
        const rs = G.wdOrder.filter((r) => M.days[r].grp === g);
        const mid = (G.at.weekday[rs[0]] + G.at.weekday[rs[rs.length - 1]]) / 2;
        ctx.fillStyle = hover !== null && M.days[hover].grp === g ? T.ink : T.muted;
        ctx.fillText(weekdayText(M.days[rs[0]].t), G.L - 6, mid);
      }
    }
    ctx.restore();

    /* The ridges, back to front by where each row is now, so rows that pass each other swap cleanly. The ridge you
       rest on is lifted out of the stack and drawn last, so the faded rows in front of it never veil it. */
    const grow = live ? growth() : null;
    const order = M.days.map((_, i) => i).sort((a, b) => {
      const ha = hot[a].x > 0.5 ? hot[a].x : 0, hb = hot[b].x > 0.5 ? hot[b].x : 0;
      return ha - hb || base[a].x - base[b].x;
    });
    const span = G.bottom - G.y0 || 1;
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    const x0D = Math.round(G.x0 * sx), stepD = G.stepD;
    for (const r of order) {
      const yDf = (base[r].x - lift[r].x) * sy;
      const k = kit.clamp((base[r].x - G.y0) / span, 0, 1);
      const fa = fade[r].x, ho = hot[r].x;
      const isHot = hover === r;
      const p = pathsFor(r, isHot, grow);
      ctx.setTransform(sx, 0, 0, sy, 0, yDf);
      ctx.globalAlpha = kit.lerp(0.45, 1, kit.clamp((fa - FADE_TO) / (1 - FADE_TO), 0, 1));
      ctx.fillStyle = T.page;
      ctx.fill(p.fill);
      /* a lifted ridge keeps covering down to where it sat, so the row behind it is hidden, not cut in half */
      if (lift[r].x > 0.3) ctx.fillRect(G.x0, 1.5, G.plotW, lift[r].x + 0.5);
      const a = kit.lerp(kit.lerp(0.42, 0.92, k), 1, ho) * fa;
      ctx.globalAlpha = 1;
      ctx.fillStyle = kit.rgba(P, P[3] * a);
      ctx.fill(p.line);
      [0.45, 0.65, 0.85].forEach((ba, i) => { ctx.fillStyle = kit.rgba(P, P[3] * a * ba); ctx.fill(p.stip[i]); });
      if (p.flatRuns.length) {
        let yD = Math.floor(yDf), phase = Math.round((yDf - yD) * 4);
        if (phase === 4) { yD += 1; phase = 0; }
        const pat = dotPattern(ctx, isHot ? HOT_R : DOT_R, phase);
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        pat.pattern.setTransform(new DOMMatrix([1, 0, 0, 1, x0D - stepD / 2, yD - pat.hD / 2]));
        ctx.fillStyle = pat.pattern;
        ctx.globalAlpha = P[3] * a;
        for (let j = 0; j < p.flatRuns.length; j += 2) {
          const ka = p.flatRuns[j], kb = p.flatRuns[j + 1];
          ctx.fillRect(x0D + ka * stepD - stepD / 2, yD - pat.hD / 2, (kb - ka + 1) * stepD, pat.hD);
        }
        ctx.globalAlpha = 1;
      }
    }
    ctx.restore();

    /* the step you read: a ring on the ridge, and a dot on the axis below it */
    if (hover !== null) {
      const x = xOfStep(hoverStep);
      const i = Math.min(G.S - 1, Math.max(0, Math.round(x - G.x0)));
      const g = grow ? grow[i] : 1;
      const yTop = base[hover].x - lift[hover].x - G.profs[hover][i] * G.amp * g;
      ctx.save();
      ctx.strokeStyle = kit.rgba(P, P[3] * 0.9); ctx.lineWidth = 1;
      ctx.beginPath(); ctx.arc(x, yTop, 2.2 + 2 * ring.x, 0, TAU); ctx.stroke();
      ctx.fillStyle = T.primary;
      ctx.beginPath(); ctx.arc(x, G.bottom + 5.5, 1.8, 0, TAU); ctx.fill();
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    if (hover === null || !M.n) {
      if (!M.n) { kit.setReadout(parts.readout, []); return; }
      kit.setReadout(parts.readout, M.isStage
        ? [{ label: 'Taller is deeper sleep' }]
        : [{ label: `${M.valueLabel}, up to`, value: kit.formatNumber(M.vmax) }]);
      return;
    }
    const d = M.days[hover], k = hoverStep;
    const pos = M.lo + k * M.step;
    const at = M.unit === 'minute' ? sinceText(pos) : clockText(pos);
    let value = '-';
    if (d.has[k]) value = d.stageAt[k] ? STAGES[d.stageAt[k]].word : kit.formatNumber(d.vals[k]);
    kit.setReadout(parts.readout, [{ when: `${dayText(d.t)}, ${at}` }, { label: M.valueLabel, value }]);
  }

  function setHover(r, k, from) {
    if (r !== null) k = kit.clamp(k === null || k === undefined ? M.days[r].peak : k, 0, M.nb - 1);
    if (r === hover && (r === null || k === hoverStep)) { hoverFrom = from; return; }
    const rowChanged = r !== hover;
    hover = r; hoverFrom = from;
    if (r !== null) hoverStep = k;
    if (r !== null && rowChanged) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the order ---------- */
  function setSort(next) {
    if (next === sort || !G) {
      if (!G && next !== sort) { sort = next; pressSort(); kit.setLabel(parts.plot, ariaLabel(M, sort)); }
      return;
    }
    sort = next;
    pressSort();
    kit.setLabel(parts.plot, ariaLabel(M, sort));
    const order = sort === 'date' ? G.dateOrder : G.wdOrder;
    const n = M.n, stagger = n > 1 ? Math.min(0.012, 0.42 / (n - 1)) : 0;
    const prior = labelsTo;
    order.forEach((r, s) => { target[r] = G.at[sort][r]; goAt[r] = clock + (n - 1 - s) * stagger; });
    labelsFrom = prior; labelsTo = sort;
    labelsOut = clock; labelsIn = clock + (n - 1) * stagger + 0.45;
    if (reduced()) {
      for (let r = 0; r < n; r++) { base[r].x = target[r]; base[r].v = 0; goAt[r] = clock; }
      labelsFrom = labelsTo;
    }
    loop.wake();
  }
  function pressSort() {
    sortButtons.forEach((b) => b.setAttribute('aria-pressed', String(b.dataset.sort === sort)));
  }

  /* ---------- the loop: runs only while rows slide, lift or fade, labels change, or ridges rise ---------- */
  function step(dt) {
    clock += dt;
    if (!G || !M.n) { draw(); return false; }
    let moving = false;
    const still = reduced();
    const hb = hover !== null ? target[hover] : 0;
    for (let r = 0; r < M.n; r++) {
      const go = clock >= goAt[r];
      const liftTo = r === hover ? G.lift : 0, hotTo = r === hover ? 1 : 0;
      const fadeTo = hover !== null && target[r] > hb + 0.5 ? FADE_TO : 1;
      if (still) {
        if (go) { base[r].x = target[r]; base[r].v = 0; }
        lift[r].x = liftTo; lift[r].v = 0; hot[r].x = hotTo; hot[r].v = 0; fade[r].x = fadeTo; fade[r].v = 0;
      } else {
        if (go) kit.springTo(base[r], target[r], dt, SLIDE, 0);
        kit.springTo(lift[r], liftTo, dt, LIFT_D, LIFT_B);
        kit.springTo(hot[r], hotTo, dt, HOT_D, 0);
        kit.springTo(fade[r], fadeTo, dt, FADE_D, 0);
      }
      if (!go || base[r].x !== target[r] || lift[r].x !== liftTo || hot[r].x !== hotTo || fade[r].x !== fadeTo) moving = true;
    }
    if (still) { ring.x = 1; ring.v = 0; }
    else if (hover !== null) { kit.springTo(ring, 1, dt, 0.28, 0.15); if (ring.x !== 1) moving = true; }
    if (labelsFrom !== labelsTo) {
      if (still || clock >= labelsIn + 0.25) labelsFrom = labelsTo; else moving = true;
    }
    if (mode === 'live' && !grown) moving = true;
    if (mode === 'live' && !still && clock - revealAt < 0.35) moving = true;
    draw();
    return moving;
  }

  /* ---------- pointer and keys ---------- */
  function pick(px, py) {
    if (!G || !M.n) return null;
    if (py < G.top - 6 || py > G.bottom + 8 || px > G.R + 4) return null;
    const i = Math.min(G.S - 1, Math.max(0, Math.round(px - G.x0)));
    const inRows = px >= G.x0 - 2;
    const front = M.days.map((_, r) => r).sort((a, b) => base[b].x - base[a].x);
    if (inRows) {
      for (const r of front) {
        const b = base[r].x, topY = b - G.profs[r][i] * G.amp;
        if (py >= topY - 2 && py <= b + 3) return { r, k: Math.min(M.nb - 1, Math.floor(((px - G.x0) / G.plotW) * M.nb)) };
      }
    }
    let best = null, dist = Infinity;
    for (const r of front) {
      const d = Math.abs(base[r].x - py);
      if (d < dist) { dist = d; best = r; }
    }
    if (best === null || dist > Math.max(6, G.pitch)) return null;
    return { r: best, k: inRows ? Math.min(M.nb - 1, Math.max(0, Math.floor(((px - G.x0) / G.plotW) * M.nb))) : null };
  }
  const onMove = (e) => {
    if (!width || !M.n) return;
    const rect = canvas.getBoundingClientRect();
    const hit = pick(e.clientX - rect.left, e.clientY - rect.top);
    if (!hit) { if (hoverFrom === 'pointer') setHover(null, null, null); return; }
    setHover(hit.r, hit.k, 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onMove);
  parts.plot.addEventListener('pointerleave', onLeave);

  const visual = () => (G ? (sort === 'date' ? G.dateOrder : G.wdOrder) : M.days.map((_, i) => i));
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey || !M.n) return;
    const order = visual(), last = order.length - 1;
    const at = hover === null ? -1 : order.indexOf(hover);
    const front = order[last];
    let r = hover, k = hover === null ? null : hoverStep;
    switch (e.key) {
      case 'ArrowUp': r = at < 0 ? front : order[Math.max(0, at - 1)]; break;
      case 'ArrowDown': r = at < 0 ? front : order[Math.min(last, at + 1)]; break;
      case 'PageUp': r = at < 0 ? front : order[Math.max(0, at - 7)]; break;
      case 'PageDown': r = at < 0 ? front : order[Math.min(last, at + 7)]; break;
      case 'ArrowLeft': if (r === null) r = front; else k = Math.max(0, k - 1); break;
      case 'ArrowRight': if (r === null) r = front; else k = Math.min(M.nb - 1, k + 1); break;
      case 'Home': if (r === null) r = front; k = 0; break;
      case 'End': if (r === null) r = front; k = M.nb - 1; break;
      case 'Escape':
        if (hover === null) return;
        e.preventDefault(); setHover(null, null, 'key'); return;
      default: return;
    }
    e.preventDefault();
    setHover(r, k, 'key');
  };
  const onBlur = () => { if (hoverFrom === 'key') setHover(null, null, null); };
  parts.plot.addEventListener('keydown', onKey);
  parts.plot.addEventListener('blur', onBlur);

  /* ---------- setup and the handle ---------- */
  function placeSort() {
    if (parts.foot) { parts.foot.appendChild(sortEl); sortEl.classList.remove('vk-ridges-top'); }
    else { root.appendChild(sortEl); sortEl.classList.add('vk-ridges-top'); if (parts.readout) parts.readout.style.paddingRight = '150px'; }
  }
  function resetRows() {
    const n = M.n;
    base = []; target = []; goAt = []; lift = []; hot = []; fade = [];
    for (let r = 0; r < n; r++) {
      const y = G ? G.at[sort][r] : 0;
      base.push({ x: y, v: 0 }); target.push(y); goAt.push(0);
      lift.push({ x: 0, v: 0 }); hot.push({ x: 0, v: 0 }); fade.push({ x: 1, v: 0 });
    }
  }
  function apply() {
    M = model(out);
    G = null;
    if (width) layout();
    resetRows();
    hover = null; hoverFrom = null;
    labelsFrom = labelsTo = sort;
    born = null; grown = mode !== 'live';
    kit.setLabel(parts.plot, ariaLabel(M, sort));
    readout();
    kit.foot(parts.foot, out, opts);
    placeSort();
    sortEl.style.display = M.n > 1 ? '' : 'none';
    if (mode === 'live') rowsWait(reveal < Infinity);
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    layout();
    for (let r = 0; r < M.n; r++) { const y = G.at[sort][r]; target[r] = y; base[r].x = y; base[r].v = 0; }
    draw();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; draw(); }
  });
  /* the dot pitch is set in device pixels, so a move to a screen of another density lays the rows out again */
  const stopDpr = kit.watchDpr(() => { if (width) { layout(); draw(); } });
  const stopMotion = kit.watchReducedMotion(() => { if (mode === 'live') rowsWait(reveal < Infinity); loop.wake(); });
  apply();
  if (mode === 'live') { revealAt = clock; loop.wake(); }

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
      sortEl.removeEventListener('click', onSortClick);
      root.remove();
    },
  };
}
