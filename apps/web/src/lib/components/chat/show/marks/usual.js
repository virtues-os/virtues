// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/usual.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* usual.js: the baseline band. A measure as a line of dots over a soft band for its usual range, so each point
   reads as inside, above or below what is normal for this person, without a number. A chart mark: kind "chart",
   mark "baseline".

   It fits any measure that has a normal range of its own. The query says what usual is: each row carries its own
   usual_low and usual_high, such as the middle two thirds of the 90 days before it, so the band can be one steady
   range or one that moves with the season, and the river follows it.
   - The band is a river of fine ink dots, laid on a blue-noise field: solid inside the range, thinning past its
     edges with no edge line, over a faint wash. It is labelled in words inside the band, "Your usual", at the end
     the line leaves clear. There is no mean line, no target and no count.
   - The line is the first series, as series.js draws a line: the primary, with a dot at each value. A point
     outside the band is drawn exactly like one inside it. Nothing is colored as good or bad.
   - Rest on a point, or tab to the chart and use the left and right arrow keys (Home, End, Page Up and Page Down
     too, Escape clears): a ring grows on it, and the readout gives its x, its value and, in plain words, whether
     it is within your usual range, above it or below it, with the range itself where it fits.
   - The value and the band are numbers, or durations as sql_query writes an interval ("0 months 0 days 26100.000000
     seconds"), read as spread.js reads them, so a duration reads 7 h 15 m.
   - x is spaced by time when it is dates or times, as series.js spaces a line, else evenly. A null value breaks
     the line; a null usual_low or usual_high leaves the band out there.

   Columns: the x axis first, then the value, then usual_low and usual_high (found by name, low and high work too,
   else the third and fourth columns). 2 to 199 rows.

   Contract: see the top of kit.js. Block: readout 26 + plot 200 + foot 26 = 252px, like series, so the frame holds
   it. The river is drawn once per width, theme and density and kept as a bitmap;
   resting on a point redraws the line and the bitmap. Nothing runs while nothing moves. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'usual', kinds: ['chart'], marks: ['baseline'] });

const HEIGHT = kit.PLOT.height;
const PAD = kit.PLOT.pad;
const LABEL_W = 52;
const END_R = 1.75;
const MAX_ROWS = 199;
const SPACING = 3.1;            /* the river's blue-noise spacing */
const RAMP = 40;
const GROW = 0.45, GROW_BOUNCE = 0.1;
const TAU = Math.PI * 2;
const WORD = 'Your usual';
const HINT = 'Use the left and right arrow keys to read each point and whether it is within your usual range.';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

/* ---------- values: a number or a duration, as spread.js reads them ---------- */

const INTERVAL = /^(-?\d+) months? (-?\d+) days? (-?\d+(?:\.\d+)?) seconds?$/;
const NUMERIC = /^-?\d+(?:\.\d+)?$/;
function valueKind(values) {
  const present = values.filter((v) => v !== null && v !== undefined && v !== '');
  if (!present.length) return null;
  if (present.every((v) => (typeof v === 'number' && Number.isFinite(v)) || (typeof v === 'string' && NUMERIC.test(v.trim())))) return 'number';
  if (present.every((v) => typeof v === 'string' && INTERVAL.test(v.trim()))) return 'duration';
  return null;
}
function readValue(v, kind) {
  if (v === null || v === undefined || v === '') return null;
  if (kind === 'number') { const n = typeof v === 'number' ? v : Number(String(v).trim()); return Number.isFinite(n) ? n : null; }
  const m = String(v).trim().match(INTERVAL);
  return m ? Number(m[1]) * 30 * 86400 + Number(m[2]) * 86400 + Number(m[3]) : null;
}
function fmtDuration(sec, tick) {
  const total = Math.round(sec / 60), h = Math.floor(Math.abs(total) / 60), m = Math.abs(total) % 60, sign = total < 0 ? '-' : '';
  if (!h) return `${sign}${m} min`;
  if (tick && !m) return `${sign}${h} h`;
  return m ? `${sign}${h} h ${m} m` : `${sign}${h} h`;
}
const format = (kind, n, tick) => (kind === 'duration' ? fmtDuration(n, tick) : kit.formatNumber(n));
const DURATION_STEPS = [60, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200, 86400];
function ticksFor(kind, lo, hi) {
  if (kind !== 'duration') {
    const t = kit.niceTicks(lo, hi);
    return t.length >= 2 ? t : [lo, hi === lo ? lo + 1 : hi];
  }
  if (!(hi > lo)) { lo -= 1800; hi += 1800; }
  /* whole hours once the span passes two hours, so a label stays short enough for the margin */
  const step = DURATION_STEPS.find((s) => (hi - lo < 7200 || s >= 3600) && (hi - lo) / s <= 4) || 86400;
  const out = [];
  for (let t = Math.floor(lo / step) * step; t <= Math.ceil(hi / step) * step + 1e-9; t += step) out.push(t);
  return out;
}

/* What one point is, from the first column's name: "Day" gives day and days. */
function nouns(label) {
  const w = String(label || '').trim().toLowerCase();
  if (!/^[a-z]+$/.test(w)) return ['point', 'points'];
  if (w === 'date') return ['day', 'days'];
  if (/[^aeiou]y$/.test(w)) return [w, `${w.slice(0, -1)}ies`];
  if (/(s|x|z|ch|sh)$/.test(w)) return [w, `${w}es`];
  return [w, `${w}s`];
}

/* ---------- the model ---------- */

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = (Array.isArray(out && out.rows) ? out.rows : []).slice(0, MAX_ROWS);
  const named = (...want) => { for (const w of want) { const c = columns.find((k) => typeof k === 'string' && k.trim().toLowerCase() === w); if (c !== undefined) return c; } return undefined; };
  const xKey = columns[0];
  const lowKey = named('usual_low', 'low') || columns[2], highKey = named('usual_high', 'high') || columns[3];
  const vKey = columns.find((c, i) => i > 0 && c !== lowKey && c !== highKey);
  const kind = vKey ? valueKind(rows.flatMap((r) => [r[vKey], r[lowKey], r[highKey]])) : null;
  const xs = rows.map((r) => r[xKey]);
  const dates = kit.isDateColumn(xs) || kit.isTimeColumn(xs);
  let times = null;
  if (dates) {
    const ts = xs.map((v) => { const t = kit.parseTime(v); return t ? t.getTime() : undefined; });
    if (ts.every((t) => t !== undefined)) times = ts;
  }
  const order = times ? rows.map((_, i) => i).sort((a, b) => times[a] - times[b]) : rows.map((_, i) => i);
  const pts = rows.map((r, i) => {
    let lo = kind ? readValue(r[lowKey], kind) : null, hi = kind ? readValue(r[highKey], kind) : null;
    if (lo !== null && hi !== null && lo > hi) [lo, hi] = [hi, lo];
    if (lo === null || hi === null) lo = hi = null;
    return { i, x: xs[i], v: kind ? readValue(r[vKey], kind) : null, lo, hi };
  });
  const values = pts.flatMap((p) => [p.v, p.lo, p.hi]).filter((v) => v !== null);
  const ticks = values.length ? ticksFor(kind, Math.min(...values), Math.max(...values)) : [0, 1];
  const [one, many] = nouns(kit.columnLabel(xKey || ''));
  return {
    columns, rows, xKey, vKey, lowKey, highKey, kind, xs, dates, times, order, pts, ticks, one, many,
    tLo: times ? Math.min(...times) : 0, tHi: times ? Math.max(...times) : 0,
    yMin: ticks[0], yMax: ticks[ticks.length - 1],
  };
}

const sideOf = (p) => (p.v === null || p.lo === null ? null : p.v < p.lo ? 'below' : p.v > p.hi ? 'above' : 'within');

function ariaLabel(M) {
  const what = kit.columnLabel(M.vKey || 'value');
  if (!M.kind || !M.pts.some((p) => p.v !== null)) return `${what}, with no values to draw.`;
  let s = `Line chart of ${what} by ${kit.columnLabel(M.xKey || 'row')}`;
  if (M.order.length > 1) s += `, from ${kit.formatX(M.xs[M.order[0]], M.dates)} to ${kit.formatX(M.xs[M.order[M.order.length - 1]], M.dates)}`;
  const banded = M.order.map((i) => M.pts[i]).filter((p) => sideOf(p));
  if (!banded.length) return `${s}.`;
  s += ', over a band for your usual range.';
  const inside = banded.filter((p) => sideOf(p) === 'within').length, share = inside / banded.length;
  s += share === 1 ? ` Every ${M.one} sits within it.`
    : share >= 2 / 3 ? ` Most ${M.many} sit within it.`
      : share >= 0.4 ? ` About half of the ${M.many} sit within it.`
        : ` Most ${M.many} sit outside it.`;
  if (share < 0.4) return s;
  const sides = ['above', 'below'].map((side) => {
    const out = banded.filter((p) => sideOf(p) === side).map((p) => kit.formatX(p.x, M.dates));
    if (!out.length) return null;
    const list = out.length <= 4 ? (out.length === 1 ? out[0] : `${out.slice(0, -1).join(', ')} and ${out[out.length - 1]}`) : `several ${M.many}, from ${out[0]} to ${out[out.length - 1]}`;
    return `${side} it on ${list}`;
  }).filter(Boolean);
  if (sides.length) s += ` It runs ${sides.join(', and ')}.`;
  return s;
}

/* ---------- blue noise for the river: Bridson's Poisson disc, seeded, so the river is the same on every draw ---------- */

function poisson(w, h, r, seed) {
  let st = seed >>> 0;
  const rnd = () => { st = (st * 1664525 + 1013904223) >>> 0; return st / 4294967296; };
  const cell = r / Math.SQRT2, gw = Math.ceil(w / cell), gh = Math.ceil(h / cell);
  const grid = new Int32Array(gw * gh).fill(-1), xs = [], ys = [], act = [];
  const add = (x, y) => { const id = xs.length; xs.push(x); ys.push(y); act.push(id); grid[Math.floor(y / cell) * gw + Math.floor(x / cell)] = id; };
  add(w / 2, h / 2);
  while (act.length) {
    const ai = Math.floor(rnd() * act.length), px = xs[act[ai]], py = ys[act[ai]];
    let found = false;
    for (let t = 0; t < 20; t++) {
      const a = rnd() * TAU, d = r * (1 + rnd()), x = px + Math.cos(a) * d, y = py + Math.sin(a) * d;
      if (x < 0 || y < 0 || x >= w || y >= h) continue;
      const gx = Math.floor(x / cell), gy = Math.floor(y / cell);
      let ok = true;
      for (let yy = Math.max(0, gy - 2); yy <= Math.min(gh - 1, gy + 2) && ok; yy++) {
        for (let xx = Math.max(0, gx - 2); xx <= Math.min(gw - 1, gx + 2); xx++) {
          const q = grid[yy * gw + xx];
          if (q >= 0 && (xs[q] - x) ** 2 + (ys[q] - y) ** 2 < r * r) { ok = false; break; }
        }
      }
      if (ok) { add(x, y); found = true; break; }
    }
    if (!found) { act[ai] = act[act.length - 1]; act.pop(); }
  }
  const thr = new Float32Array(xs.length);
  for (let i = 0; i < xs.length; i++) thr[i] = rnd();
  return { xs, ys, thr };
}

const smooth = (a, b, x) => { const t = kit.clamp((x - a) / (b - a), 0, 1); return t * t * (3 - 2 * t); };

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-usual');
  el.appendChild(root);
  const parts = kit.block(root, { plotHeight: HEIGHT, readout: true, foot: opts.foot !== false, hint: HINT });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let riverCv = null, cacheKey = '', noise = null, noiseKey = '';
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let revealDraw = reveal;
  let born = [];
  let clock = 0, revealAt = 0, riverAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);
  const measure = document.createElement('canvas').getContext('2d');

  /* ---------- geometry, as series.js spaces a line ---------- */
  const B = () => box(width);
  const Y = (v) => { const b = B(), h = b.bottom - b.top; return b.top + h - ((v - M.yMin) / (M.yMax - M.yMin || 1)) * h; };
  function xAt(i) {
    const b = B(), w = b.right - b.left, n = M.rows.length;
    if (M.times) return b.left + (M.tHi === M.tLo ? w / 2 : ((M.times[i] - M.tLo) / (M.tHi - M.tLo)) * w);
    return b.left + (n === 1 ? w / 2 : (i / (n - 1)) * w);
  }

  /* The band's top and bottom in px at each whole px of x, NaN where there is no band: straight between two points
     that both have one, and half a step past a point at either end of a run. */
  function bandRows() {
    const top = new Float32Array(width + 1).fill(NaN), bot = new Float32Array(width + 1).fill(NaN);
    const seq = M.order.map((i) => M.pts[i]);
    const b = B();
    const put = (x, yt, yb) => { const k = Math.round(x); if (k >= b.left && k <= b.right) { top[k] = yt; bot[k] = yb; } };
    for (let k = 0; k < seq.length; k++) {
      const p = seq[k];
      if (p.lo === null) continue;
      const x = xAt(p.i), yt = Y(p.hi), yb = Y(p.lo);
      const q = seq[k + 1];
      if (q && q.lo !== null) {
        const x2 = xAt(q.i), yt2 = Y(q.hi), yb2 = Y(q.lo);
        for (let xx = Math.ceil(x); xx <= Math.floor(x2); xx++) { const t = (xx - x) / (x2 - x || 1); put(xx, kit.lerp(yt, yt2, t), kit.lerp(yb, yb2, t)); }
      } else put(x, yt, yb);
      const prev = seq[k - 1];
      const half = Math.max(3, Math.min(10, ((q ? xAt(q.i) : x) - (prev ? xAt(prev.i) : x)) / 4));
      if (!prev || prev.lo === null) for (let xx = Math.ceil(x - half); xx < x; xx++) put(xx, yt, yb);
      if (!q || q.lo === null) for (let xx = Math.floor(x) + 1; xx <= x + half; xx++) put(xx, yt, yb);
    }
    return { top, bot };
  }

  /* ---------- the river ---------- */
  function renderRiver(k) {
    const c = document.createElement('canvas');
    c.width = Math.max(1, Math.round(width * k)); c.height = Math.max(1, Math.round(HEIGHT * k));
    const ctx = c.getContext('2d');
    ctx.setTransform(k, 0, 0, k, 0, 0);
    if (!M.pts.some((p) => p.lo !== null)) return c;
    const { top, bot } = bandRows();
    const I = tokens.rgb.ink;
    /* a faint wash with soft ends, then the dots */
    for (let x = 0; x <= width; x++) {
      if (!Number.isFinite(top[x])) continue;
      const edge = Math.min(Number.isFinite(top[x - 6]) ? 1 : 0.5, Number.isFinite(top[x + 6]) ? 1 : 0.5);
      ctx.fillStyle = kit.rgba(I, I[3] * 0.03 * edge);
      ctx.fillRect(x - 0.5, top[x], 1, Math.max(1, bot[x] - top[x]));
    }
    const key = `${width}`;
    if (noiseKey !== key) { noise = poisson(width, HEIGHT, SPACING, 0x51a7e); noiseKey = key; }
    const tiers = [new Path2D(), new Path2D(), new Path2D()];
    for (let n = 0; n < noise.xs.length; n++) {
      const x = noise.xs[n], y = noise.ys[n], col = Math.round(x);
      const yt = top[col], yb = bot[col];
      if (!Number.isFinite(yt)) continue;
      const mid = (yt + yb) / 2, half = Math.max(2.5, (yb - yt) / 2);
      const u = Math.abs(y - mid) / half;
      const p = (1 - smooth(0.82, 1.45, u)) * (0.8 + 0.2 * (1 - Math.min(1, u) ** 2));
      if (noise.thr[n] >= p) continue;
      const t = p > 0.85 ? 2 : p > 0.5 ? 1 : 0;
      const r = 0.7 + 0.15 * t;
      tiers[t].moveTo(x + r, y); tiers[t].arc(x, y, r, 0, TAU);
    }
    [0.2, 0.28, 0.36].forEach((a, t) => { ctx.fillStyle = kit.rgba(I, I[3] * a); ctx.fill(tiers[t]); });
    return c;
  }
  function river() {
    const k = kit.dpr();
    const key = `${width}|${k}|${tokens.signature}`;
    if (key !== cacheKey || !riverCv) { riverCv = renderRiver(k); cacheKey = key; }
    return riverCv;
  }

  /* Where the words "Your usual" go: inside the band at the end of it the line leaves clear, else just above it. */
  function labelSpot(ctx) {
    const seq = M.order.map((i) => M.pts[i]).filter((p) => p.lo !== null);
    if (!seq.length) return null;
    ctx.font = `9.5px ${tokens.mono}`;
    const w = ctx.measureText(WORD).width, b = B();
    const line = M.order.map((i) => M.pts[i]).filter((p) => p.v !== null).map((p) => [xAt(p.i), Y(p.v)]);
    const clear = (x0, yb) => line.every(([x, y]) => x < x0 - 5 || x > x0 + w + 5 || y < yb - 12 || y > yb + 5);
    const spots = [];
    for (const [p, align] of [[seq[0], 'left'], [seq[seq.length - 1], 'right']]) {
      const x = xAt(p.i), yt = Y(p.hi), yb = Y(p.lo);
      const x0 = align === 'left' ? kit.clamp(x + 4, b.left + 4, b.right - w - 2) : kit.clamp(x - 4 - w, b.left + 4, b.right - w - 2);
      if (yb - yt >= 16) spots.push([x0, yt + 11], [x0, yb - 4]);
      spots.push([x0, yt - 4]);
    }
    return spots.find(([x0, y]) => clear(x0, y) && y > b.top + 8) || spots[0];
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (p, x, y, r) => { p.moveTo(x + r, y); p.arc(x, y, r, 0, TAU); };

  function growth(i) {
    if (mode === 'none') return 1;
    if (born[i] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[i], GROW, GROW_BOUNCE));
  }

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    if (!M.rows.length || !M.kind) return;
    const T = tokens, b = B(), L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const hv = hover;

    /* the grid and the floor arrive behind the sweep */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    for (const t of M.ticks) {
      const y = Math.round(Y(t));
      if (Math.abs(y - floor) < 1) continue;
      dotted(ctx, L, y + 0.5, R, y + 0.5, 5, 1.1, T.borderStrong);
    }
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(L, floor, R - L, 1);
    ctx.restore();

    /* the river, arriving behind the sweep with a soft edge */
    const rc = river(), k = rc.width / width;
    if (revealDraw >= width + RAMP) ctx.drawImage(rc, 0, 0, width, HEIGHT);
    else {
      const slice = (xa, xb, a) => {
        xa = Math.max(0, xa); xb = Math.min(width, xb);
        if (xb - xa < 0.01 || a <= 0) return;
        ctx.globalAlpha = a;
        ctx.drawImage(rc, xa * k, 0, (xb - xa) * k, rc.height, xa, 0, xb - xa, HEIGHT);
      };
      slice(0, revealDraw - RAMP, 1);
      for (let i = 0; i < 10; i++) slice(revealDraw - RAMP + (i * RAMP) / 10, revealDraw - RAMP + ((i + 1) * RAMP) / 10, 1 - (i + 0.5) / 10);
      ctx.globalAlpha = 1;
    }

    /* x labels, as series.js places them on a line over times */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`;
    ctx.fillStyle = T.muted;
    ctx.textBaseline = 'alphabetic';
    const n = M.rows.length;
    const labelled = kit.labelIndices(n, Math.max(2, Math.floor((R - L) / LABEL_W))).map((j) => M.order[j]);
    const placed = labelled.map((i, j) => {
      const x = xAt(i), align = j === labelled.length - 1 && labelled.length > 1 ? 'right' : j === 0 ? 'left' : 'center';
      const text = kit.formatX(M.xs[i], M.dates);
      const w = ctx.measureText(text).width;
      const x0 = align === 'left' ? x : align === 'right' ? x - w : x - w / 2;
      return { x, align, text, x0, x1: x0 + w };
    });
    const kept = [];
    placed.forEach((p, j) => {
      if (j === placed.length - 1) while (kept.length > 1 && p.x0 < kept[kept.length - 1].x1 + 6) kept.pop();
      if (!kept.length || p.x0 >= kept[kept.length - 1].x1 + 6) kept.push(p);
    });
    for (const p of kept) {
      const a = labelA * (clipX < Infinity ? kit.seg(clipX - p.x, 0, 24) : 1);
      if (a <= 0) continue;
      ctx.globalAlpha = a; ctx.textAlign = p.align;
      ctx.fillText(p.text, p.x, HEIGHT - 6);
    }
    ctx.restore();

    /* the y axis: fine dots from the foot to the top with an ink dot at each end */
    dotted(ctx, L, floor, L, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    const ends = new Path2D();
    disc(ends, L, top, END_R); disc(ends, L, floor, END_R);
    ctx.fill(ends);
    ctx.save();
    ctx.globalAlpha = labelA;
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    for (const t of M.ticks) ctx.fillText(format(M.kind, t, true), L - 6, Y(t));
    ctx.restore();

    /* the band's words, once the river has arrived */
    const wordA = revealDraw >= width + RAMP ? (live && !reduced() ? kit.seg(clock - riverAt - 0.1, 0, 0.3) : 1) : 0;
    const spot = wordA > 0 ? labelSpot(ctx) : null;
    if (spot) {
      ctx.save();
      ctx.globalAlpha = wordA;
      ctx.font = `9.5px ${T.mono}`; ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic';
      ctx.lineJoin = 'round'; ctx.lineWidth = 3; ctx.strokeStyle = T.page;
      ctx.strokeText(WORD, spot[0], spot[1]);
      ctx.fillStyle = T.muted; ctx.fillText(WORD, spot[0], spot[1]);
      ctx.restore();
    }

    /* the guide at the point you rest on */
    if (hv !== null && M.pts[hv].v !== null) dotted(ctx, xAt(hv), top, xAt(hv), floor, 4, 1, T.borderStrong);

    /* the line and its dots, the first series as series.js draws it */
    const st = kit.SERIES_STYLE[0], col = T.rgb.s0;
    const dimA = hv !== null ? kit.lerp(1, 0.45, dim.x) : 1;
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    ctx.strokeStyle = kit.rgba(col); ctx.lineWidth = st.line; ctx.lineJoin = 'round'; ctx.lineCap = 'round';
    ctx.beginPath();
    let pen = false;
    const pts = [];
    for (const i of M.order) {
      const v = M.pts[i].v, g = growth(i);
      if (v === null || g <= 0) { pen = false; continue; }
      const x = xAt(i), y = floor + (Y(v) - floor) * g;
      if (pen) ctx.lineTo(x, y); else ctx.moveTo(x, y);
      pen = true;
      pts.push([i, x, y, g]);
    }
    ctx.stroke();
    const dense = (R - L) / Math.max(1, n) < 6;
    const r0 = st.head * 0.75;
    const heads = [new Path2D(), new Path2D()];
    let ringAt = null;
    for (const [i, x, y, g] of pts) {
      if (dense && i !== hv) continue;
      const r = r0 * kit.clamp(g * 3, 0, 1) + (i === hv ? 0.6 : 0);
      disc(heads[hv !== null && i !== hv ? 1 : 0], x, y, r);
      if (i === hv) ringAt = [x, y];
    }
    ctx.fillStyle = kit.rgba(col); ctx.fill(heads[0]);
    ctx.fillStyle = kit.rgba(col, col[3] * dimA); ctx.fill(heads[1]);
    ctx.restore();
    if (ringAt) {
      ctx.save();
      ctx.lineWidth = 1;
      ctx.strokeStyle = kit.rgba(col, Math.min(1, col[3] * 1.1) * 0.9);
      ctx.beginPath(); ctx.arc(ringAt[0], ringAt[1], r0 + 1.5 + 3.5 * ring.x, 0, TAU); ctx.stroke();
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  const bandSwatch = '<svg class="vk-sw" width="12" height="10" viewBox="0 0 12 10" aria-hidden="true">'
    + [[2, 2], [6, 1.5], [10, 2.2], [4, 5], [8, 5.2], [2, 8], [6, 8.4], [10, 7.8]].map(([x, y]) => `<circle cx="${x}" cy="${y}" r=".95" style="fill:var(--vk-ink);opacity:.38"/>`).join('')
    + '</svg>';
  function fits(form) {
    let w = 0;
    form.forEach((p, k) => {
      if (k) w += 16;
      measure.font = `13px ${tokens.sans}`;
      if (p.when !== undefined) { w += measure.measureText(p.when).width; return; }
      if (p.swatch) w += 18;
      if (p.label) w += measure.measureText(p.label).width + 6;
      measure.font = `500 13px ${tokens.sans}`;
      if (p.value !== undefined) w += measure.measureText(p.value).width;
    });
    return w <= Math.max(0, width - 4);
  }
  const range = (p) => `${format(M.kind, p.lo)} to ${format(M.kind, p.hi)}`;
  function readout() {
    if (!parts.readout) return;
    const label = kit.columnLabel(M.vKey || 'value');
    if (!M.kind) { kit.setReadout(parts.readout, M.vKey ? [{ label }] : []); return; }
    if (hover === null) {
      const last = M.order.map((i) => M.pts[i]).filter((p) => p.lo !== null).pop();
      const forms = last
        ? [[{ label }, { swatch: bandSwatch, label: WORD, value: range(last) }], [{ label }, { swatch: bandSwatch, label: WORD }]]
        : [[{ label }]];
      kit.setReadout(parts.readout, forms.find(fits) || forms[forms.length - 1]);
      return;
    }
    const p = M.pts[hover], side = sideOf(p);
    const when = { when: kit.formatX(p.x, M.dates) };
    const value = { label, value: p.v === null ? 'No value' : format(M.kind, p.v) };
    const words = { within: 'Within your usual', above: 'Above your usual', below: 'Below your usual' }[side];
    const forms = side ? [
      [when, value, { label: `${words}, ${range(p)}` }],
      [when, value, { label: words }],
      [when, { label: '', value: value.value }, { label: words }],
    ] : [[when, value], [when, { label: '', value: value.value }]];
    kit.setReadout(parts.readout, forms.find(fits) || forms[forms.length - 1]);
  }

  function setHover(i, from) {
    if (i === hover) { hoverFrom = from; return; }
    hover = i; hoverFrom = from;
    if (i !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows, the others dim, or the chart arrives ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    const want = hover !== null ? 1 : 0;
    if (reduced()) { ring.x = 1; ring.v = 0; dim.x = want; dim.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      kit.springTo(dim, want, dt, 0.2, 0);
      moving = ring.x !== 1 || dim.x !== want;
    }
    if (mode === 'live') {
      if (revealDraw < width + RAMP) {
        revealDraw = reveal < Infinity ? reveal : reduced() ? Infinity : revealDraw + 1400 * dt;
        if (revealDraw >= width + RAMP) riverAt = clock;
        moving = true;
      }
      if (!reduced()) {
        if (clock - revealAt < 0.45 || clock - riverAt < 0.45) moving = true;
        for (let i = 0; i < born.length; i++) if (born[i] !== undefined && clock - born[i] < 1.2) { moving = true; break; }
      }
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    for (let i = 0; i < M.rows.length; i++) if (born[i] === undefined && (reveal === Infinity || xAt(i) <= reveal)) born[i] = clock;
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
    if (mode === 'none') { mode = 'live'; revealDraw = x; revealAt = clock; }
    reveal = x;
    if (x < Infinity) revealDraw = x;
    else if (reduced()) revealDraw = Infinity;
    if (width) markBorn();
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  function nearest(px) {
    let best = null, dist = Infinity;
    for (const i of M.order) { const d = Math.abs(xAt(i) - px); if (d < dist) { best = i; dist = d; } }
    return best;
  }
  const pick = (e) => nearest(e.clientX - canvas.getBoundingClientRect().left);
  const onMove = (e) => {
    if (!width || !M.rows.length || !M.kind) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!width || !M.rows.length || !M.kind) return;
    const i = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === i) { setHover(null, null); return; }
    setHover(i, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(parts.plot, {
    count: () => (M.kind ? M.rows.length : 0),
    get: () => (hover === null ? null : M.order.indexOf(hover)),
    set: (k, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(k === null ? null : M.order[k], 'key');
    },
    page: 7,
  });

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    born = [];
    cacheKey = '';
    hover = null; hoverFrom = null;
    kit.setLabel(parts.plot, ariaLabel(M));
    readout();
    kit.foot(parts.foot, out, opts);
    if (mode === 'live') { if (width) markBorn(); rowsWait(reveal < Infinity); }
    loop.wake();
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    if (mode === 'live') markBorn();
    readout();
    draw();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; draw(); }
  });
  const stopDpr = kit.watchDpr(() => draw());
  const stopMotion = kit.watchReducedMotion(() => { rowsWait(mode === 'live' && reveal < Infinity); loop.wake(); });
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
      loop.stop(); stopKeys(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerdown', onDown);
      parts.plot.removeEventListener('pointerleave', onLeave);
      root.remove();
    },
  };
}
