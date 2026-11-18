// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/spread.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* spread.js: a dot plot. One dot for each record, stacked by value, so the shape of a distribution shows: where most
   values fall and how far the rest reach. A new chart mark: kind "chart", mark "dotplot".

   The value runs across and the dots stack up from the floor where values crowd together, as a one-sided beeswarm:
   every dot sits at its exact value, and a dot that would touch one already placed rises just far enough to clear
   it. Where most records fall reads as the tallest part of the shape, and how far the rest reach reads as its tails.
   - The value is a number, a duration or a time of day, read from what the box writes:
       a number                        7.25, or a numeric string, which show.rs makes a number
       a duration                      an interval as sql_query writes it, "0 months 0 days 2700.000000 seconds"
       a time of day                   a timestamp (its clock time is the value and its date labels the dot), or
                                       text "HH:MM" or "HH:MM:SS"
   - A time of day wraps around midnight: the axis is cut at the widest stretch of the clock with no records, so
     times from 10 pm to 1 am run left to right across midnight rather than splitting at the two ends.
   - A soft band covers the middle two thirds of the records and is labelled in words from the first column's
     name, "Most nights" for a column named Night. Two thirds, so "most" is true. There is no mean line, no target and no count.
   - Rest on a dot, or tab to the chart and use the left and right arrow keys: a ring grows on the dot and the
     readout names its date and value. Home and End jump to the lowest and the highest, Escape clears.
   - One accent: the dots are the primary, as series.js draws its first series; the band is a neutral ink wash.
   - The dot size shrinks from 7.5px to 3px until the tallest stack fits, so 3 rows and 400 rows both fill the plot.

   Columns: one value column, or a first column naming each record (a date, a time or a label) and then the value.
   3 to 400 rows. A null value is left out.

   Contract: see the top of kit.js. Block: readout 26 + plot 160 + foot 26 = 212px. The plot is number.js's 160px
   rather than a chart's 200: stacks of a few hundred dots stay low, and a taller plot would only hold empty space. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'spread', kinds: ['chart'], marks: ['dotplot'] });

const HEIGHT = 160;          /* a dot plot's stacks rarely climb past half a chart's plot, so it is number's 160px */
const PAD = Object.freeze({ top: 8, right: 14, bottom: 22, left: 14 });
const EDGE = 12;             /* the lowest and highest values sit this far inside the plot's ends */
const LABEL_ROOM = 18;       /* the band's label sits above the stacks */
const D_MAX = 7.5, D_MIN = 3, GAP = 0.8;
const MOST = Object.freeze([1 / 6, 5 / 6]);
const GROW = 0.42, GROW_BOUNCE = 0.08, LEVEL_STEP = 0.022;
const END_R = 1.75;
const MAX_ROWS = 400;
const TAU = Math.PI * 2;
const HINT = 'Use the left and right arrow keys to read each dot, from the lowest value to the highest.';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

/* ---------- values: a number, a duration or a time of day ---------- */

const INTERVAL = /^(-?\d+) months? (-?\d+) days? (-?\d+(?:\.\d+)?) seconds?$/;
const CLOCK = /^(\d{1,2}):(\d{2})(?::(\d{2}(?:\.\d+)?))?$/;
const DATE = /^\d{4}-\d{2}-\d{2}$/;
const NUMERIC = /^-?\d+(?:\.\d+)?$/;

function valueKind(values) {
  const present = values.filter((v) => v !== null && v !== undefined && v !== '');
  if (!present.length) return null;
  if (present.every((v) => (typeof v === 'number' && Number.isFinite(v)) || (typeof v === 'string' && NUMERIC.test(v.trim())))) return 'number';
  if (present.every((v) => typeof v === 'string' && INTERVAL.test(v.trim()))) return 'duration';
  if (present.every((v) => typeof v === 'string' && (CLOCK.test(v.trim()) || (!DATE.test(v.trim()) && kit.parseTime(v.trim()))))) return 'time';
  return null;
}

/* The value as a number in its kind's unit (seconds for a duration, minutes since midnight for a time), and the
   moment it carries, when it is a timestamp. */
function readValue(v, kind) {
  if (v === null || v === undefined || v === '') return null;
  if (kind === 'number') { const n = typeof v === 'number' ? v : Number(String(v).trim()); return Number.isFinite(n) ? { n } : null; }
  const s = String(v).trim();
  if (kind === 'duration') {
    const m = s.match(INTERVAL);
    return m ? { n: Number(m[1]) * 30 * 86400 + Number(m[2]) * 86400 + Number(m[3]) } : null;
  }
  const c = s.match(CLOCK);
  if (c) {
    const h = Number(c[1]), mi = Number(c[2]), se = c[3] ? Number(c[3]) : 0;
    return h < 24 && mi < 60 ? { n: h * 60 + mi + se / 60 } : null;
  }
  const t = kit.parseTime(s);
  return t ? { n: t.getHours() * 60 + t.getMinutes() + t.getSeconds() / 60, t } : null;
}

const clockDate = (min) => { const m = ((min % 1440) + 1440) % 1440; return new Date(2000, 0, 1, Math.floor(m / 60), Math.floor(m % 60)); };
function fmtClock(min, { tick = false } = {}) {
  const d = clockDate(Math.round(min));
  return tick && d.getMinutes() === 0
    ? d.toLocaleTimeString(undefined, { hour: 'numeric' })
    : d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}
function fmtDuration(sec) {
  if (Math.abs(sec) < 60) return `${Math.round(sec)} s`;
  const total = Math.round(sec / 60), h = Math.floor(Math.abs(total) / 60), m = Math.abs(total) % 60, sign = total < 0 ? '-' : '';
  if (!h) return `${sign}${m} min`;
  return m ? `${sign}${h} h ${m} m` : `${sign}${h} h`;
}
function format(kind, n, opts) {
  if (kind === 'time') return fmtClock(n, opts);
  if (kind === 'duration') return fmtDuration(n);
  return kit.formatNumber(n);
}
/* The band's ends as the readout says them: times to five minutes, durations to the minute. */
function roundFor(kind, n) {
  if (kind === 'time') return Math.round(n / 5) * 5;
  if (kind === 'duration') return Math.round(n / 60) * 60;
  return n;
}

const fmtDay = (t) => t.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });

/* What one record is, from the first column's name: "Night" gives night and nights, so the band reads "Most
   nights". A name of more than one word, or no first column, gives record and records. */
function nouns(label) {
  const w = String(label || '').trim().toLowerCase();
  if (!/^[a-z]+$/.test(w)) return ['record', 'records'];
  if (w === 'date') return ['day', 'days'];
  if (/[^aeiou]y$/.test(w)) return [w, `${w.slice(0, -1)}ies`];
  if (/(s|x|z|ch|sh)$/.test(w)) return [w, `${w}es`];
  return [w, `${w}s`];
}

function quantile(sorted, p) {
  if (!sorted.length) return NaN;
  const at = (sorted.length - 1) * p, lo = Math.floor(at), hi = Math.ceil(at);
  return sorted[lo] + (sorted[hi] - sorted[lo]) * (at - lo);
}

/* Step sizes for the axis, in the kind's unit. */
const DURATION_STEPS = [1, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1200, 1800, 3600, 5400, 7200, 10800, 14400, 21600, 43200, 86400];
const CLOCK_STEPS = [5, 10, 15, 30, 60, 120, 180, 240, 360, 720];
function ticksFor(kind, lo, hi, count) {
  if (!(hi > lo)) return [lo];
  if (kind === 'number') return kit.niceTicks(lo, hi, count).filter((t) => t >= lo - 1e-9 && t <= hi + 1e-9);
  const steps = kind === 'time' ? CLOCK_STEPS : DURATION_STEPS;
  const step = steps.find((s) => (hi - lo) / s <= count) || steps[steps.length - 1] * Math.ceil((hi - lo) / count / steps[steps.length - 1]);
  const out = [];
  for (let t = Math.ceil(lo / step) * step; t <= hi + 1e-9; t += step) out.push(t);
  return out;
}

/* ---------- the model ---------- */

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = (Array.isArray(out && out.rows) ? out.rows : []).slice(0, MAX_ROWS);
  const vKey = columns.length ? columns[columns.length - 1] : null;
  const kKey = columns.length > 1 ? columns[0] : null;
  const kind = vKey ? valueKind(rows.map((r) => r[vKey])) : null;
  const keys = kKey ? rows.map((r) => r[kKey]) : [];
  const keyDates = kKey ? kit.isTimeColumn(keys) : false;
  const recs = [];
  if (kind) {
    rows.forEach((r, i) => {
      const val = readValue(r[vKey], kind);
      if (!val) return;
      let when = '';
      if (val.t) when = fmtDay(val.t);
      else if (kKey && keyDates) {
        const t = kit.parseTime(r[kKey]);
        if (t) when = DATE.test(String(r[kKey])) ? fmtDay(t) : `${fmtDay(t)}, ${kit.formatTime(t)}`;
      } else if (kKey) when = kit.formatCell(r[kKey]);
      recs.push({ i, raw: val.n, v: val.n, when, t: val.t || (keyDates ? kit.parseTime(r[kKey]) : null) });
    });
  }
  /* A time of day is cut at the widest stretch of the clock that holds no record, so the records run unbroken. */
  if (kind === 'time' && recs.length > 1) {
    const ms = recs.map((r) => r.raw).sort((a, b) => a - b);
    let start = ms[0], widest = ms[0] + 1440 - ms[ms.length - 1];
    for (let k = 1; k < ms.length; k++) if (ms[k] - ms[k - 1] > widest) { widest = ms[k] - ms[k - 1]; start = ms[k]; }
    for (const r of recs) r.v = r.raw >= start ? r.raw : r.raw + 1440;
  }
  const sorted = recs.map((r) => r.v).sort((a, b) => a - b);
  const lo = sorted.length ? sorted[0] : 0, hi = sorted.length ? sorted[sorted.length - 1] : 1;
  const most = sorted.length >= 3 ? [quantile(sorted, MOST[0]), quantile(sorted, MOST[1])] : null;
  const [one, noun] = nouns(kKey ? kit.columnLabel(kKey) : '');
  /* the window the records cover: the first column's dates when it has them, else the values' own */
  const times = (keyDates ? keys.map((k) => kit.parseTime(k)) : recs.map((r) => r.t)).filter(Boolean).sort((a, b) => a - b);
  return { columns, rows, vKey, kKey, kind, recs, lo, hi, most, one, noun, first: times[0] || null, last: times[times.length - 1] || null };
}

/* A one-sided beeswarm: each dot, taken in value order, sits on the floor or on the lowest spot that clears every dot
   already placed. xs are sorted; returns each dot's height above the floor, in px. */
function swarm(xs, D) {
  const n = xs.length, ys = new Float64Array(n);
  const D2 = D * D - 1e-6;
  for (let i = 0; i < n; i++) {
    const near = [], cands = [0];
    for (let j = i - 1; j >= 0 && xs[i] - xs[j] < D; j--) {
      const dx = xs[i] - xs[j], dy = Math.sqrt(D * D - dx * dx);
      near.push(j);
      cands.push(ys[j] + dy);
      if (ys[j] - dy >= 0) cands.push(ys[j] - dy);
    }
    cands.sort((a, b) => a - b);
    let y = cands[cands.length - 1];
    for (const c of cands) {
      if (c < 0) continue;
      let ok = true;
      for (const j of near) { const dx = xs[i] - xs[j], dy = c - ys[j]; if (dx * dx + dy * dy < D2) { ok = false; break; } }
      if (ok) { y = c; break; }
    }
    ys[i] = y;
  }
  return ys;
}

function ariaLabel(M) {
  const what = kit.columnLabel(M.vKey || 'value');
  if (!M.recs.length) return `Dot plot of ${what}, with no values to draw.`;
  const span = M.first && M.last && M.first !== M.last ? `, from ${kit.formatTime(M.first, { withDate: true, withTime: false })} to ${kit.formatTime(M.last, { withDate: true, withTime: false })}` : '';
  let s = `Dot plot of ${what}, one dot for each ${M.one}${span}.`;
  if (M.most) s += ` Most ${M.noun} fall between ${format(M.kind, roundFor(M.kind, M.most[0]))} and ${format(M.kind, roundFor(M.kind, M.most[1]))}.`;
  const lowest = M.recs.reduce((a, b) => (b.v < a.v ? b : a)), highest = M.recs.reduce((a, b) => (b.v > a.v ? b : a));
  const [lw, hw] = M.kind === 'time' ? ['earliest', 'latest'] : M.kind === 'duration' ? ['shortest', 'longest'] : ['lowest', 'highest'];
  const at = (r) => (r.when ? ` on ${r.when}` : '');
  s += ` The ${lw} is ${format(M.kind, lowest.v)}${at(lowest)}, and the ${hw} ${format(M.kind, highest.v)}${at(highest)}.`;
  return s;
}

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-spread');
  el.appendChild(root);
  const parts = kit.block(root, { plotHeight: HEIGHT, readout: true, foot: opts.foot !== false, hint: HINT });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let G = null;                       /* the layout at this width: x, the dots, the ticks */
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null; /* an index into G.order */
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- layout ---------- */
  function layout() {
    G = null;
    if (!width || !M.recs.length) return;
    const b = box(width), L = b.left, R = b.right, floor = b.bottom;
    const span = M.hi - M.lo;
    const pad = span > 0 ? 0 : 1;
    const lo = M.lo - pad, hi = M.hi + pad;
    const x0 = L + EDGE, x1 = R - EDGE;
    const X = (v) => x0 + ((v - lo) / (hi - lo || 1)) * (x1 - x0);
    const V = (x) => lo + ((x - x0) / (x1 - x0 || 1)) * (hi - lo);
    const order = M.recs.map((_, k) => k).sort((a, c) => M.recs[a].v - M.recs[c].v);
    const xs = order.map((k) => X(M.recs[k].v));
    const room = floor - b.top - LABEL_ROOM - 1.5;
    let D = D_MAX, ys = swarm(xs, D), top = Math.max(0, ...ys);
    while (top + D > room && D > D_MIN) { D = Math.max(D_MIN, D - 0.5); ys = swarm(xs, D); top = Math.max(0, ...ys); }
    const squeeze = top + D > room ? (room - D) / Math.max(1, top) : 1;
    const r = Math.max(1, (D - GAP) / 2);
    const dots = order.map((k, o) => {
      const y = ys[o] * squeeze;
      return { k, x: xs[o], y: floor - 1.5 - r - y, lift: y, level: Math.round(ys[o] / D) };
    });
    /* keyboard order: across, then up each stack */
    const keyOrder = dots.map((_, o) => o).sort((a, c) => (Math.abs(dots[a].x - dots[c].x) < 0.01 ? dots[c].y - dots[a].y : dots[a].x - dots[c].x));
    const count = Math.max(2, Math.floor((R - L) / 72));
    const ticks = ticksFor(M.kind, V(L), V(R), count).filter((t) => X(t) >= L + 2 && X(t) <= R - 2);
    G = { b, X, V, r, D, dots, keyOrder, ticks };
  }

  function growth(o) {
    if (mode === 'none') return 1;
    if (born[o] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[o] - G.dots[o].level * LEVEL_STEP, GROW, GROW_BOUNCE));
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (p, x, y, r) => { p.moveTo(x + r, y); p.arc(x, y, r, 0, TAU); };

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    if (!G) return;
    const T = tokens, b = G.b, L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const bandA = live && !reduced() ? (reveal < Infinity ? 0 : kit.seg(clock - revealAt - 0.15, 0, 0.35)) : 1;

    /* the band over where most records fall, a neutral wash with dotted edges */
    if (M.most && bandA > 0) {
      const xa = G.X(M.most[0]), xb = Math.max(G.X(M.most[1]), xa + 1);
      ctx.save();
      ctx.globalAlpha = bandA;
      ctx.fillStyle = kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.035);
      ctx.fillRect(xa, top + 2, xb - xa, floor - top - 2);
      dotted(ctx, xa + 0.5, floor - 1, xa + 0.5, top + 2, 3.5, 1, T.borderStrong);
      dotted(ctx, xb - 0.5, floor - 1, xb - 0.5, top + 2, 3.5, 1, T.borderStrong);
      ctx.font = `9.5px ${T.mono}`;
      ctx.fillStyle = T.muted;
      ctx.textBaseline = 'alphabetic';
      const text = `Most ${M.noun}`;
      const w = ctx.measureText(text).width;
      let tx = xb - xa >= w + 10 ? xa + 5 : (xa + xb) / 2 - w / 2;
      tx = kit.clamp(tx, L + 4, R - w - 2);
      ctx.fillText(text, tx, top + 12);
      ctx.restore();
    }

    /* the floor and the value axis arrive behind the sweep */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(L, floor, R - L, 1);
    ctx.font = `9.5px ${T.mono}`;
    ctx.textBaseline = 'alphabetic';
    let edge = -Infinity;
    const labels = G.ticks.map((t) => {
      const text = format(M.kind, t, { tick: true });
      const w = ctx.measureText(text).width, x = G.X(t);
      return { t, x, text, x0: kit.clamp(x - w / 2, 0, width - w), w };
    });
    for (const p of labels) {
      ctx.fillStyle = T.borderStrong;
      ctx.beginPath(); disc(ctx, p.x, floor + 4, 0.9); ctx.fill();
      if (p.x0 < edge + 8) continue;
      edge = p.x0 + p.w;
      const a = labelA * (clipX < Infinity ? kit.seg(clipX - p.x, 0, 24) : 1);
      if (a <= 0) continue;
      ctx.globalAlpha = a;
      ctx.fillStyle = T.muted;
      ctx.textAlign = 'left';
      ctx.fillText(p.text, p.x0, HEIGHT - 6);
      ctx.globalAlpha = 1;
    }
    ctx.restore();

    /* the axis at the left: fine dots from the foot to the top with an ink dot at each end */
    dotted(ctx, L, floor, L, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath(); disc(ctx, L, top, END_R); disc(ctx, L, floor, END_R); ctx.fill();

    /* the dots */
    const hv = hover === null ? null : G.keyOrder[hover];
    const dimA = hv !== null ? kit.lerp(1, 0.4, dim.x) : 1;
    const col = T.rgb.s0;
    if (hv !== null) {
      const d = G.dots[hv];
      dotted(ctx, d.x, d.y + G.r + 2, d.x, floor - 1, 3, 1, T.borderStrong);
    }
    const heads = [new Path2D(), new Path2D()];
    for (let o = 0; o < G.dots.length; o++) {
      const d = G.dots[o];
      const g = growth(o);
      if (g <= 0) continue;
      const y = floor - 1.5 - G.r - d.lift * g;
      const r = G.r * kit.clamp(g * 3, 0, 1) + (o === hv ? 0.5 : 0);
      disc(heads[hv !== null && o !== hv ? 1 : 0], d.x, y, r);
    }
    ctx.fillStyle = kit.rgba(col); ctx.fill(heads[0]);
    ctx.fillStyle = kit.rgba(col, col[3] * dimA); ctx.fill(heads[1]);
    if (hv !== null) {
      const d = G.dots[hv];
      ctx.save();
      ctx.lineWidth = 1;
      ctx.strokeStyle = kit.rgba(col, Math.min(1, col[3] * 1.1) * 0.9);
      ctx.beginPath(); ctx.arc(d.x, d.y, G.r + 1.5 + 3 * ring.x, 0, TAU); ctx.stroke();
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    const label = kit.columnLabel(M.vKey || 'value');
    if (!M.recs.length) { kit.setReadout(parts.readout, M.vKey ? [{ label }] : []); return; }
    if (hover === null || !G) {
      const rest = [{ label }];
      if (M.most) rest.push({ label: `Most ${M.noun}`, value: `${format(M.kind, roundFor(M.kind, M.most[0]))} to ${format(M.kind, roundFor(M.kind, M.most[1]))}` });
      kit.setReadout(parts.readout, rest);
      return;
    }
    const rec = M.recs[G.dots[G.keyOrder[hover]].k];
    const bits = rec.when ? [{ when: rec.when }] : [];
    kit.setReadout(parts.readout, bits.concat([{ label, value: format(M.kind, rec.v) }]));
  }

  function setHover(i, from) {
    if (i === hover) { hoverFrom = from; return; }
    hover = i; hoverFrom = from;
    if (i !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows, the others dim, or the dots arrive ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    if (reduced()) { ring.x = 1; ring.v = 0; dim.x = hover !== null ? 1 : 0; dim.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      kit.springTo(dim, hover !== null ? 1 : 0, dt, 0.2, 0);
      moving = ring.x !== 1 || dim.x !== (hover !== null ? 1 : 0);
    }
    if (mode === 'live' && !reduced()) {
      if (clock - revealAt < 0.6 || reveal < Infinity) moving = true;
      for (let o = 0; o < born.length; o++) if (born[o] !== undefined && clock - born[o] < 1.4) { moving = true; break; }
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    if (!G) return;
    for (let o = 0; o < G.dots.length; o++) if (born[o] === undefined && (reveal === Infinity || G.dots[o].x <= reveal)) born[o] = clock;
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
    if (mode === 'none') { mode = 'live'; revealAt = clock; }
    reveal = x;
    if (reveal === Infinity) revealAt = clock;
    markBorn();
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  function nearest(px, py) {
    if (!G) return null;
    let best = null, dist = Infinity;
    for (let n = 0; n < G.keyOrder.length; n++) {
      const d = G.dots[G.keyOrder[n]];
      const dx = d.x - px, dy = (d.y - py) * 0.5;
      const dd = dx * dx + dy * dy;
      if (dd < dist) { dist = dd; best = n; }
    }
    return best;
  }
  const pick = (e) => { const r = canvas.getBoundingClientRect(); return nearest(e.clientX - r.left, e.clientY - r.top); };
  const onMove = (e) => {
    if (!G) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!G) return;
    const i = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === i) { setHover(null, null); return; }
    setHover(i, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(parts.plot, {
    count: () => (G ? G.keyOrder.length : 0),
    get: () => hover,
    set: (k, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(k, 'key');
    },
  });

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    born = [];
    hover = null; hoverFrom = null;
    layout();
    kit.setLabel(parts.plot, ariaLabel(M));
    readout();
    kit.foot(parts.foot, out, opts);
    if (mode === 'live') { markBorn(); rowsWait(reveal < Infinity); loop.wake(); }
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    const was = hover;
    layout();
    if (was !== null && G && was >= G.keyOrder.length) hover = null;
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
