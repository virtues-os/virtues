// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/aligned.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* aligned.js: Event-aligned overlay. kind "chart", mark "aligned".

   Every occurrence of a recurring event lined up at time zero, each one a pale trace, with the middle of them all as
   one line. It shows the typical course of a measure before and after something that happens again and again (a
   session, a meal, a dose, a commute, a payday) and how much it varies from one time to the next. It describes the
   pattern around the event and claims nothing about cause.

   The call gives mark "aligned" and event_label, the event in a few words ("Rowing starts", "Payday"). The rows are
   one value for each occurrence at each offset from its start:
     event    a date (or a time) for each occurrence; every row with the same event is one trace
     offset   minutes from that occurrence's start, negative before it; 1,440 is a day
     value    the measure: the one other column, aliased as the owner reads it (AS "Heart rate")
   Up to 1,200 rows and 40 events. Offsets are binned by the query: to 5 minutes across a few hours, or to a day
   across a few weeks. A value of null, or a step between two offsets more than 2.5 times the usual step, breaks a
   trace, so a stretch with no readings draws a gap rather than a straight line across it.

   Offsets read in the unit the bins call for. When the usual step between offsets is a whole number of days, the
   words read "3 days after", "1 day before" and "On the day", and the axis never ticks between two days. Otherwise
   the words read "20 min after", "1 h 30 m before" and "At the start", with days in front past a day ("2 days 4 h
   after"). The axis reads in days when its own step is whole days (−7 d, 0, +7 d), else in hours and minutes
   (−1 h, −30 m, 0, +30 m).

   What it draws, in series.js's grammar (the dotted grid, the y axis of fine dots with an ink dot at each end):
   - Each occurrence is a pale dotted trace in ink. Where the traces agree, their dots pile up and the ink darkens.
   - The middle of them all, the median at each offset, is one solid line in the accent. An offset that fewer than
     a quarter of the occurrences reach has no middle, so the line never stands for one stray trace.
   - A dotted rule at zero carries event_label at the top, on whichever side of the rule is emptier there.
   - Resting on a trace lifts it in ink, dims the rest, and the readout gives its date and its value there. Resting
     anywhere else reads the middle at that offset, with a hairline and a ring.
   - Keys: left and right step through the offsets (along the lifted trace, or the middle), up and down lift each
     occurrence in date order and come back to the middle past either end, Home and End jump, Escape lets go.
   - In the frame the traces and the middle draw behind the frame's sweep; under reduced motion they are there at once.

   Contract: see the top of kit.js. Block: readout 26 + plot 200 + foot 26 = 252px, the same as bar and line. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'aligned', kinds: ['chart'], marks: ['aligned'] });

const HEIGHT = kit.PLOT.height;
const PAD = kit.PLOT.pad;
const LABEL_W = 56;          /* one offset label, such as "+30 m", gets 56px */
const END_R = 1.75;          /* the ink dots at the top and the foot of the y axis */
const HIT = 7, HIT_TOUCH = 12;
const MAX_EXACT = 160, BINS = 120;
const TAU = Math.PI * 2;
const HINT = 'Use the left and right arrow keys to read the middle at each offset, and the up and down arrow keys to lift each time on its own.';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

/* ---------- words for values and offsets ---------- */

function fmtHM(minutes) {
  const t = Math.round(Math.round(Math.abs(minutes) * 1000) / 1000), h = Math.floor(t / 60), m = t % 60, s = minutes < 0 ? '-' : '';
  if (!h) return `${s}${m} m`;
  return m ? `${s}${h} h ${m} m` : `${s}${h} h`;
}
/* A value column named in hours or minutes reads in hours and minutes; anything else is a plain number. */
function valueFormat(column) {
  const c = String(column || '').toLowerCase();
  if (/\b(hours?|hrs?)\b/.test(c)) return (v) => fmtHM(v * 60);
  if (/\b(minutes?|mins?)\b/.test(c)) return (v) => `${kit.formatNumber(v)} min`;
  return (v) => kit.formatNumber(v);
}
const DAY = 1440;
/* Whether offsets read in days: the bins are a whole number of days apart. */
const inDays = (step) => step >= DAY && Math.abs(step / DAY - Math.round(step / DAY)) < 1e-6;
const plural = (n, one) => `${n} ${one}${n === 1 ? '' : 's'}`;
function spanWords(min) {
  const a = Math.round(Math.abs(min));
  if (a < 60) return `${a} min`;
  const d = Math.floor(a / DAY), rest = a - d * DAY;
  if (d) {
    const h = Math.round(rest / 60);
    return h ? `${plural(d, 'day')} ${h} h` : plural(d, 'day');
  }
  const h = Math.floor(a / 60), m = a % 60;
  return m ? `${h} h ${m} m` : `${h} h`;
}
/* An offset in words. In days, an offset is read to the nearest day, and zero is the day of the event itself. */
function offsetWords(o, days) {
  if (days) {
    const d = Math.round(o / DAY);
    if (!d) return 'On the day';
    return `${plural(Math.abs(d), 'day')} ${d < 0 ? 'before' : 'after'}`;
  }
  if (Math.abs(o) < 0.5) return 'At the start';
  return `${spanWords(o)} ${o < 0 ? 'before' : 'after'}`;
}
/* An axis label. A step of whole days reads in days; otherwise whole hours read in hours and the rest in minutes. */
function offsetTick(o, step) {
  if (Math.abs(o) < 1e-9) return '0';
  const s = o < 0 ? '−' : '+', a = Math.round(Math.abs(o));
  if (inDays(step)) return `${s}${Math.round(a / DAY)} d`;
  return a % 60 === 0 ? `${s}${a / 60} h` : `${s}${a} m`;
}
/* Round offsets for the axis: the smallest step that keeps the labels about LABEL_W apart. Offsets binned by the day
   never get a tick between two days. */
function offsetTicks(lo, hi, max, days) {
  const steps = [1, 2, 5, 10, 15, 30, 60, 90, 120, 180, 240, 360, 480, 720, DAY, 2 * DAY, 7 * DAY, 14 * DAY, 28 * DAY]
    .filter((s) => !days || s >= DAY);
  const span = hi - lo || 1;
  const step = steps.find((s) => span / s <= Math.max(1, max - 1)) || steps[steps.length - 1];
  const out = [];
  for (let v = Math.ceil(lo / step - 1e-9) * step; v <= hi + 1e-9; v += step) out.push(Math.round(v * 1000) / 1000);
  return { step, ticks: out };
}
const median = (sorted) => {
  const n = sorted.length;
  if (!n) return null;
  return n % 2 ? sorted[(n - 1) / 2] : (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
};

/* ---------- the model ---------- */

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const eKey = columns.includes('event') ? 'event' : columns[0];
  const oKey = columns.includes('offset') ? 'offset' : columns[1];
  const vKey = columns.includes('value') ? 'value' : columns.find((c) => c !== eKey && c !== oKey) || null;
  const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null);
  const byKey = new Map();
  for (const r of rows) {
    const raw = r[eKey];
    const o = num(r[oKey]);
    if (raw === null || raw === undefined || o === null) continue;
    const key = String(raw);
    let e = byKey.get(key);
    if (!e) { e = { key, raw, pts: [] }; byKey.set(key, e); }
    e.pts.push({ o, v: vKey ? num(r[vKey]) : null });
  }
  const events = [...byKey.values()];
  for (const e of events) {
    e.pts.sort((a, b) => a.o - b.o);
    const t = kit.parseTime(typeof e.raw === 'string' ? e.raw : '');
    e.t = t ? t.getTime() : null;
    e.label = kit.formatCell(e.raw);
  }
  if (events.length && events.every((e) => e.t !== null)) events.sort((a, b) => a.t - b.t);

  /* The usual step between two offsets of one occurrence; a step much longer than it is a gap. */
  const gaps = [];
  for (const e of events) for (let k = 1; k < e.pts.length; k++) gaps.push(e.pts[k].o - e.pts[k - 1].o);
  gaps.sort((a, b) => a - b);
  const step = median(gaps) || 1;
  for (const e of events) {
    e.vals = [];
    let prev = null;
    for (const p of e.pts) {
      if (p.v === null) { prev = null; continue; }
      e.vals.push({ o: p.o, v: p.v, cut: !prev || p.o - prev.o > 2.5 * step });
      prev = p;
    }
  }
  const pts = events.flatMap((e) => e.vals);
  const oLo = pts.length ? Math.min(...pts.map((p) => p.o)) : 0;
  const oHi = pts.length ? Math.max(...pts.map((p) => p.o)) : 1;

  /* The middle: the median of every occurrence's value at each offset, or in each of BINS bins when the offsets
     do not line up (raw samples rather than binned minutes). */
  const exact = [...new Set(pts.map((p) => p.o))].sort((a, b) => a - b);
  const quorum = Math.max(1, Math.ceil(events.length * 0.25));
  let middle = [];
  let midStep = step;
  if (exact.length <= MAX_EXACT) {
    const at = new Map(exact.map((o) => [o, []]));
    for (const p of pts) at.get(p.o).push(p.v);
    middle = exact.map((o) => ({ o, vs: at.get(o) }));
  } else {
    const w = (oHi - oLo) / BINS || 1;
    midStep = w;
    const bins = Array.from({ length: BINS }, (_, i) => ({ o: oLo + w * (i + 0.5), vs: [] }));
    for (const p of pts) bins[Math.min(BINS - 1, Math.floor((p.o - oLo) / w))].vs.push(p.v);
    middle = bins;
  }
  middle = middle.filter((b) => b.vs.length >= quorum).map((b) => ({ o: b.o, v: median(b.vs.sort((x, y) => x - y)), n: b.vs.length }));
  for (let k = 0; k < middle.length; k++) middle[k].cut = k === 0 || middle[k].o - middle[k - 1].o > 2.5 * midStep;

  const values = pts.map((p) => p.v);
  let ticks = [0, 1];
  if (values.length) {
    const lo = Math.min(...values), hi = Math.max(...values);
    ticks = kit.niceTicks(lo, hi);
    if (ticks.length < 2) ticks = [lo, hi === lo ? lo + 1 : hi];
  }
  const label = typeof (out && out.event_label) === 'string' && out.event_label.trim() ? out.event_label.trim() : 'The start';
  return { columns, rows, eKey, oKey, vKey, events, step, days: inDays(step), oLo, oHi, middle, ticks, yMin: ticks[0], yMax: ticks[ticks.length - 1],
    label, vLabel: vKey ? kit.columnLabel(vKey) : 'Value', fmt: valueFormat(vKey) };
}

function ariaLabel(M) {
  if (!M.events.length) return `Chart of ${M.vLabel.charAt(0).toLowerCase() + M.vLabel.slice(1)} lined up on ${M.label}, with no rows to draw`;
  const first = M.events[0].label, last = M.events[M.events.length - 1].label;
  const when = M.events.length > 1 ? `every time from ${first} to ${last}` : `on ${first}`;
  let text = `Chart of ${M.vLabel.charAt(0).toLowerCase() + M.vLabel.slice(1)}, ${when} lined up on ${M.label}, from ${offsetWords(M.oLo, M.days).toLowerCase()} to ${offsetWords(M.oHi, M.days).toLowerCase()}.`;
  if (M.middle.length > 1) {
    const lo = M.middle.reduce((a, p) => (p.v < a.v ? p : a), M.middle[0]);
    const hi = M.middle.reduce((a, p) => (p.v > a.v ? p : a), M.middle[0]);
    text += ` The middle of them all is lowest, ${M.fmt(lo.v)}, ${offsetWords(lo.o, M.days).toLowerCase()}, and highest, ${M.fmt(hi.v)}, ${offsetWords(hi.o, M.days).toLowerCase()}.`;
  }
  return text;
}

const SW_TRACE = '<svg class="vk-sw" width="16" height="8" viewBox="0 0 16 8" aria-hidden="true"><line x1="1.5" y1="4" x2="14.5" y2="4" style="stroke:var(--vk-muted);stroke-width:1.6;stroke-linecap:round;stroke-dasharray:0 3.2;opacity:.8"/></svg>';
const SW_LIFT = '<svg class="vk-sw" width="16" height="8" viewBox="0 0 16 8" aria-hidden="true"><line x1="1.5" y1="4" x2="14.5" y2="4" style="stroke:var(--vk-ink);stroke-width:1.8;stroke-linecap:round;stroke-dasharray:0 2.4"/></svg>';
const SW_MID = '<svg class="vk-sw" width="16" height="8" viewBox="0 0 16 8" aria-hidden="true"><line x1="1.5" y1="4" x2="14.5" y2="4" style="stroke:var(--vk-s0);stroke-width:2;stroke-linecap:round"/></svg>';

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-aligned');
  el.appendChild(root);
  const parts = kit.block(root, { plotHeight: HEIGHT, readout: true, foot: opts.foot !== false, hint: HINT });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  /* hover: null, or { ev, k }: ev is an occurrence's index (null for the middle) and k a point on it */
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let clock = 0, revealAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  const B = () => box(width);
  const X = (o) => { const b = B(); return b.left + (M.oHi === M.oLo ? 0.5 : (o - M.oLo) / (M.oHi - M.oLo)) * (b.right - b.left); };
  const Y = (v) => { const b = B(), h = b.bottom - b.top; return b.top + h - ((v - M.yMin) / (M.yMax - M.yMin || 1)) * h; };
  const list = (ev) => (ev === null ? M.middle : (M.events[ev] ? M.events[ev].vals : []));
  const pointOf = (hv) => (hv ? list(hv.ev)[hv.k] || null : null);
  function nearestK(L, o) {
    let best = null, d = Infinity;
    for (let k = 0; k < L.length; k++) { const e = Math.abs(L[k].o - o); if (e < d) { d = e; best = k; } }
    return best;
  }
  /* The value of one trace at offset o, between two of its points, or null across a gap. */
  function traceAt(vals, o) {
    if (!vals.length || o < vals[0].o || o > vals[vals.length - 1].o) return null;
    let a = 0, b = vals.length - 1;
    while (b - a > 1) { const m = (a + b) >> 1; if (vals[m].o <= o) a = m; else b = m; }
    const p = vals[a], q = vals[b];
    if (p.o === o) return p.v;
    if (q.o === o) return q.v;
    if (q.cut) return null;
    return kit.lerp(p.v, q.v, (o - p.o) / (q.o - p.o || 1));
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  function tracePath(ctx, vals) {
    ctx.beginPath();
    for (const p of vals) { if (p.cut) ctx.moveTo(X(p.o), Y(p.v)); else ctx.lineTo(X(p.o), Y(p.v)); }
  }
  function halo(ctx, text, x, y, color) {
    ctx.save();
    ctx.lineJoin = 'round'; ctx.lineWidth = 3; ctx.strokeStyle = tokens.page;
    ctx.strokeText(text, x, y);
    ctx.fillStyle = color; ctx.fillText(text, x, y);
    ctx.restore();
  }

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const T = tokens, b = B();
    const L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const passed = (x) => (clipX < Infinity ? kit.seg(clipX - x, 0, 24) : 1);

    /* the grid and the floor */
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

    /* the offsets along the floor */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textBaseline = 'alphabetic';
    const xt = offsetTicks(M.oLo, M.oHi, Math.max(2, Math.floor((R - L) / LABEL_W)), M.days);
    const placed = xt.ticks.map((o) => {
      const x = X(o), text = offsetTick(o, xt.step), w = ctx.measureText(text).width;
      const x0 = Math.abs(x - L) < 1 ? x : Math.abs(x - R) < 1 ? x - w : x - w / 2;
      return { x, text, x0, x1: x0 + w };
    });
    const kept = [];
    for (const p of placed) if (!kept.length || p.x0 >= kept[kept.length - 1].x1 + 6) kept.push(p);
    for (const p of kept) {
      const a = labelA * passed(p.x);
      if (a <= 0) continue;
      ctx.globalAlpha = a; ctx.textAlign = 'left';
      ctx.fillText(p.text, p.x0, HEIGHT - 6);
    }
    ctx.restore();

    /* the y axis: fine dots from the foot to the top with an ink dot at each end */
    dotted(ctx, L, floor, L, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath(); ctx.moveTo(L + END_R, top); ctx.arc(L, top, END_R, 0, TAU); ctx.moveTo(L + END_R, floor); ctx.arc(L, floor, END_R, 0, TAU); ctx.fill();
    ctx.save();
    ctx.globalAlpha = labelA;
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    for (const t of M.ticks) ctx.fillText(kit.formatNumber(t), L - 6, Y(t));
    ctx.restore();

    if (!M.events.length) return;
    const hv = hover, hp = pointOf(hv);
    const lifted = hv && hv.ev !== null ? hv.ev : null;
    const dimA = kit.lerp(1, lifted !== null ? 0.45 : 1, dim.x);

    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }

    /* the rule at zero */
    const x0 = M.oLo <= 0 && M.oHi >= 0 ? Math.round(X(0)) + 0.5 : null;
    if (x0 !== null) dotted(ctx, x0, top, x0, floor, 3.5, 1.25, kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.55));

    /* a hairline at the offset you read, when you read the middle */
    if (hp && lifted === null) dotted(ctx, X(hp.o), top, X(hp.o), floor, 4, 1, T.borderStrong);

    /* each occurrence, pale and dotted: where they agree, the dots pile up */
    const n = M.events.length;
    const paleA = kit.clamp(1.1 / Math.sqrt(n), 0.18, 0.5);
    ctx.lineCap = 'round'; ctx.lineJoin = 'round';
    ctx.setLineDash([0, 3.2]); ctx.lineWidth = 1.5;
    ctx.strokeStyle = kit.rgba(T.rgb.muted, T.rgb.muted[3] * paleA * (lifted !== null ? kit.lerp(1, 0.55, dim.x) : 1));
    for (let i = 0; i < n; i++) { if (i === lifted) continue; tracePath(ctx, M.events[i].vals); ctx.stroke(); }

    /* the middle of them all, in the accent */
    ctx.setLineDash([]); ctx.lineWidth = 2;
    ctx.strokeStyle = kit.rgba(T.rgb.s0, T.rgb.s0[3] * dimA);
    tracePath(ctx, M.middle); ctx.stroke();

    /* the occurrence you rest on, lifted in ink above the rest */
    if (lifted !== null) {
      ctx.setLineDash([0, 2.4]); ctx.lineWidth = 1.8;
      ctx.strokeStyle = kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.9);
      tracePath(ctx, M.events[lifted].vals); ctx.stroke();
      ctx.setLineDash([]);
    }
    ctx.restore();

    /* the ring on the point you read */
    if (hp) {
      const col = lifted !== null ? T.rgb.ink : T.rgb.s0;
      const x = X(hp.o), y = Y(hp.v);
      ctx.fillStyle = kit.rgba(col);
      ctx.beginPath(); ctx.arc(x, y, 2.4, 0, TAU); ctx.fill();
      ctx.lineWidth = 1; ctx.strokeStyle = kit.rgba(col, col[3] * 0.9);
      ctx.beginPath(); ctx.arc(x, y, 3.9 + 3.5 * ring.x, 0, TAU); ctx.stroke();
    }

    /* the event's name at the top of the rule, on the emptier side */
    if (x0 !== null) {
      ctx.save();
      ctx.font = `11px ${T.sans}`; ctx.textBaseline = 'alphabetic';
      const w = ctx.measureText(M.label).width, y = top + 10;
      const crowd = (a, z) => {
        let c = 0;
        for (const e of M.events) for (const p of e.vals) { const px = X(p.o), py = Y(p.v); if (px >= a && px <= z && py <= y + 5) c += 1; }
        for (const p of M.middle) { const px = X(p.o), py = Y(p.v); if (px >= a && px <= z && py <= y + 5) c += 4; }
        return c;
      };
      const right = x0 + 6 + w <= R + PAD.right, left = x0 - 6 - w >= L + 4;
      let side = right ? 'right' : 'left';
      if (right && left && crowd(x0 - 6 - w, x0 - 2) < crowd(x0 + 2, x0 + 6 + w)) side = 'left';
      const tx = side === 'right' ? x0 + 6 : x0 - 6 - w;
      ctx.globalAlpha = labelA * passed(x0);
      if (ctx.globalAlpha > 0) halo(ctx, M.label, tx, y, kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.78));
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    const p = pointOf(hover);
    if (!p) {
      kit.setReadout(parts.readout, [{ label: M.vLabel }, { swatch: SW_TRACE, label: 'Each time' }, { swatch: SW_MID, label: 'The middle' }]);
      return;
    }
    if (hover.ev === null) {
      kit.setReadout(parts.readout, [{ when: offsetWords(p.o, M.days) }, { swatch: SW_MID, label: 'Middle', value: M.fmt(p.v) }]);
      return;
    }
    const e = M.events[hover.ev];
    const at = offsetWords(p.o, M.days);
    kit.setReadout(parts.readout, [{ when: `${e.label}, ${at.charAt(0).toLowerCase() + at.slice(1)}` }, { swatch: SW_LIFT, label: M.vLabel, value: M.fmt(p.v) }]);
  }

  function setHover(next, from) {
    const same = (a, b) => (a === b) || (a && b && a.ev === b.ev && a.k === b.k);
    if (same(next, hover)) { hoverFrom = from; return; }
    const moved = !hover || !next || hover.ev !== next.ev;
    hover = next; hoverFrom = next ? from : null;
    if (next && moved) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows, a trace lifts or the chart is revealed ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    const lifting = hover && hover.ev !== null ? 1 : 0;
    if (reduced()) { ring.x = 1; ring.v = 0; dim.x = lifting; dim.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      kit.springTo(dim, lifting, dt, 0.2, 0);
      moving = ring.x !== 1 || dim.x !== lifting;
    }
    if (mode === 'live' && !reduced() && (clock - revealAt < 0.35 || reveal < Infinity)) moving = true;
    draw();
    return moving;
  }

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
    reveal = x;
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer ---------- */
  /* A trace within a few pixels of the pointer is lifted; anywhere else reads the middle at that offset. */
  function pick(e) {
    const r = canvas.getBoundingClientRect(), b = B();
    const px = e.clientX - r.left, py = e.clientY - r.top;
    const o = M.oLo + kit.clamp((px - b.left) / Math.max(1, b.right - b.left), 0, 1) * (M.oHi - M.oLo);
    const reach = e.pointerType === 'touch' ? HIT_TOUCH : HIT;
    let best = null, d = reach;
    M.events.forEach((ev, i) => {
      const v = traceAt(ev.vals, o);
      if (v === null) return;
      const dy = Math.abs(Y(v) - py);
      if (dy < d) { d = dy; best = i; }
    });
    if (best !== null) return { ev: best, k: nearestK(M.events[best].vals, o) };
    const k = nearestK(M.middle, o);
    return k === null ? null : { ev: null, k };
  }
  const ready = () => width && M.events.length;
  const onMove = (e) => {
    if (!ready()) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!ready()) return;
    const next = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover && next && hover.ev === next.ev && hover.k === next.k) { setHover(null, null); return; }
    setHover(next, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };

  /* ---------- keys: left and right along the offsets, up and down through the occurrences ---------- */
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey || !M.events.length) return;
    const ev = hover ? hover.ev : null;
    const at = pointOf(hover);
    const o = at ? at.o : 0;
    let next;
    switch (e.key) {
      case 'ArrowRight': case 'ArrowLeft': case 'Home': case 'End': case 'PageUp': case 'PageDown': {
        const L = list(ev), n = L.length;
        if (!n) return;
        const k = hover ? hover.k : null;
        let j;
        if (e.key === 'Home') j = 0;
        else if (e.key === 'End') j = n - 1;
        else if (k === null) j = e.key === 'ArrowLeft' || e.key === 'PageUp' ? n - 1 : 0;
        else if (e.key === 'ArrowRight') j = Math.min(n - 1, k + 1);
        else if (e.key === 'ArrowLeft') j = Math.max(0, k - 1);
        else if (e.key === 'PageDown') j = Math.min(n - 1, k + 10);
        else j = Math.max(0, k - 10);
        next = { ev, k: j };
        break;
      }
      case 'ArrowDown': case 'ArrowUp': {
        const n = M.events.length, down = e.key === 'ArrowDown';
        let to;
        if (ev === null) to = down ? 0 : n - 1;
        else { to = ev + (down ? 1 : -1); if (to < 0 || to >= n) to = null; }
        const k = nearestK(list(to), o);
        next = k === null ? null : { ev: to, k };
        break;
      }
      case 'Escape': {
        if (!hover) return;
        if (ev !== null) { const k = nearestK(M.middle, o); next = k === null ? null : { ev: null, k }; }
        else next = null;
        break;
      }
      default: return;
    }
    e.preventDefault();
    setHover(next, 'key');
  };
  const onBlur = () => { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointerleave', onLeave);
  parts.plot.addEventListener('keydown', onKey);
  parts.plot.addEventListener('blur', onBlur);

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
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
      loop.stop(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerdown', onDown);
      parts.plot.removeEventListener('pointerleave', onLeave);
      parts.plot.removeEventListener('keydown', onKey);
      parts.plot.removeEventListener('blur', onBlur);
      root.remove();
    },
  };
}
