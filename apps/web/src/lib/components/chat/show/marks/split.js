// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/split.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* split.js: the interrupted time series. A dot series split at a known moment, with each side's usual range and
   middle drawn as its own band, so a change in level at that moment shows plainly.

   A chart mark for before and after a dated change that was known before looking at the data: a new schedule, a
   move, a new medication, a new routine. The call gives mark "interrupted", split_at (a date, or the x value the
   change falls on) and split_label (the change in a few words, such as "New schedule"). The rows are a chart's:
   the x axis first, then one numeric value column, 2 to 199 rows. A row whose x is on or after split_at is after
   the change; every other row is before it.

   What it draws, in series.js's grammar:
   - One dot for each row, in the accent, with a faint dotted line joining the dots on each side. The line breaks
     at the change, so the two sides read as two runs.
   - A dotted rule at the change, with split_label at its top on whichever side is emptier there. A date axis is
     spaced by time; the rule sits between the last row before and the first row after, half a usual step before
     split_at, so it never runs through a dot.
   - Each side has a soft band for its usual range, its middle half (the 25th to the 75th percentile, as
     percentile_cont reads it), and a dotted line at its middle. A quiet label by each band says what that side
     shows, such as "Before, middle 7 h 20 m". It sits where it covers no dot; when either side's label cannot (a
     narrow column, a crowded band), both move into the readout while nothing is hovered. The words never rank the
     sides or give a cause: the chart shows the two sides and says nothing about why they differ.
   - Values in a column named in hours or minutes read in hours and minutes.
   - Resting on the chart, or tabbing to it and using the left and right arrow keys, rings the nearest row and the
     readout gives its x, its value and its side. Home and End jump to the ends and Escape clears.
   - In the frame the dots grow behind its dot, and the bands and labels fade in once it has passed them. Under
     reduced motion everything is there at once.

   Contract: see the top of kit.js. Block: readout 26 + plot 200 + foot 26 = 252px, the same as bar and line. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'split', kinds: ['chart'], marks: ['interrupted'] });

const HEIGHT = kit.PLOT.height;
const PAD = kit.PLOT.pad;
const LABEL_W = 52;
const INSET_L = 10, INSET_R = 6;   /* the first and last dots stand clear of the y axis and the edge */
const END_R = 1.75;
const GROW = 0.45, GROW_BOUNCE = 0.1;
const TAU = Math.PI * 2;
const SIDES = ['Before', 'After'];

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

function fmtHM(minutes) {
  const t = Math.round(Math.round(Math.abs(minutes) * 1000) / 1000), h = Math.floor(t / 60), m = t % 60, s = minutes < 0 ? '-' : '';
  if (!h) return `${s}${m} m`;
  return m ? `${s}${h} h ${m} m` : `${s}${h} h`;
}
function valueFormat(column) {
  const c = String(column || '').toLowerCase();
  if (/\b(hours?|hrs?)\b/.test(c)) return (v) => fmtHM(v * 60);
  if (/\b(minutes?|mins?)\b/.test(c)) return (v) => `${kit.formatNumber(v)} min`;
  return (v) => kit.formatNumber(v);
}
/* percentile_cont's interpolation, on sorted values */
function quantile(sorted, p) {
  if (!sorted.length) return null;
  const at = (sorted.length - 1) * p, a = Math.floor(at), b = Math.ceil(at);
  return sorted[a] + (sorted[b] - sorted[a]) * (at - a);
}

/* ---------- the model ---------- */

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const xKey = columns[0];
  const vKey = columns[1] || null;
  const xs = rows.map((r) => r[xKey]);
  const dates = kit.isDateColumn(xs) || kit.isTimeColumn(xs);
  let times = null;
  if (dates) {
    const ts = xs.map((v) => { const t = kit.parseTime(v); return t ? t.getTime() : undefined; });
    if (ts.every((t) => t !== undefined)) times = ts;
  }
  const order = times ? rows.map((_, i) => i).sort((a, b) => times[a] - times[b]) : rows.map((_, i) => i);
  const rank = new Array(rows.length);
  order.forEach((i, k) => { rank[i] = k; });
  const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null);
  const values = rows.map((r) => (vKey ? num(r[vKey]) : null));

  /* which side each row is on */
  const at = out && out.split_at !== undefined && out.split_at !== null ? out.split_at : null;
  let tS = null;
  if (times && typeof at === 'string') { const t = kit.parseTime(at); tS = t ? t.getTime() : null; }
  const side = rows.map((r, i) => {
    if (at === null) return 0;
    if (tS !== null) return times[i] >= tS ? 1 : 0;
    const x = xs[i];
    if (typeof x === 'number' && typeof at === 'number') return x >= at ? 1 : 0;
    const k = order.findIndex((j) => String(xs[j]) === String(at));
    return k >= 0 && rank[i] >= k ? 1 : 0;
  });
  const fmt = valueFormat(vKey);
  const sides = [0, 1].map((s) => {
    const idx = order.filter((i) => side[i] === s && values[i] !== null);
    const sorted = idx.map((i) => values[i]).sort((a, b) => a - b);
    return { idx, mid: quantile(sorted, 0.5), q1: quantile(sorted, 0.25), q3: quantile(sorted, 0.75) };
  });
  const present = values.filter((v) => v !== null);
  let ticks = [0, 1];
  if (present.length) {
    const lo = Math.min(...present), hi = Math.max(...present);
    ticks = kit.niceTicks(lo, hi);
    if (ticks.length < 2) ticks = [lo, hi === lo ? lo + 1 : hi];
  }
  const label = typeof (out && out.split_label) === 'string' && out.split_label.trim() ? out.split_label.trim() : 'The change';
  return { columns, rows, xKey, vKey, xs, dates, times, tLo: times ? Math.min(...times) : 0, tHi: times ? Math.max(...times) : 0,
    order, rank, values, side, at, tS, sides, ticks, yMin: ticks[0], yMax: ticks[ticks.length - 1], label, fmt,
    vLabel: vKey ? kit.columnLabel(vKey) : 'Value' };
}

function sideWords(M, s) {
  const S = M.sides[s];
  if (S.mid === null) return `No rows ${s ? 'after' : 'before'} the change.`;
  return `${SIDES[s]}, the middle is ${M.fmt(S.mid)}, usually ${M.fmt(S.q1)} to ${M.fmt(S.q3)}.`;
}
function ariaLabel(M) {
  const what = M.vLabel.charAt(0).toLowerCase() + M.vLabel.slice(1);
  let label = `Dot chart of ${what} by ${kit.columnLabel(M.xKey || 'row').toLowerCase()}`;
  if (M.rows.length > 1) {
    const first = kit.formatX(M.xs[M.order[0]], M.dates), last = kit.formatX(M.xs[M.order[M.order.length - 1]], M.dates);
    label += `, from ${first} to ${last}`;
  }
  if (M.at !== null) label += `, split at ${kit.formatX(M.at, M.dates)}: ${M.label}.`;
  return `${label} ${sideWords(M, 0)} ${sideWords(M, 1)}`;
}

const SW_DOT = '<svg class="vk-sw" width="8" height="8" viewBox="0 0 8 8" aria-hidden="true"><circle cx="4" cy="4" r="2.6" style="fill:var(--vk-s0)"/></svg>';
const SW_BAND = '<svg class="vk-sw" width="14" height="10" viewBox="0 0 14 10" aria-hidden="true"><rect x="0" y="1" width="14" height="8" rx="1" style="fill:var(--vk-ink);opacity:.09"/><line x1="1.5" y1="5" x2="12.5" y2="5" style="stroke:var(--vk-ink);stroke-width:1.2;stroke-linecap:round;stroke-dasharray:0 3;opacity:.5"/></svg>';

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-split');
  el.appendChild(root);
  const parts = kit.block(root, { plotHeight: HEIGHT, readout: true, foot: opts.foot !== false });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;
  let inPlot = true;   /* whether the two side labels sit in the plot, or in the readout */
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  const sideLabel = (s) => `${SIDES[s]}, middle ${M.fmt(M.sides[s].mid)}`;
  let clock = 0, revealAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  const B = () => box(width);
  const Y = (v) => { const b = B(), h = b.bottom - b.top; return b.top + h - ((v - M.yMin) / (M.yMax - M.yMin || 1)) * h; };
  const xOfTime = (t) => { const b = B(); return b.left + INSET_L + (M.tHi === M.tLo ? 0.5 : (t - M.tLo) / (M.tHi - M.tLo)) * (b.right - b.left - INSET_L - INSET_R); };
  function xAt(i) {
    const b = B(), n = M.rows.length;
    if (M.times) return xOfTime(M.times[i]);
    return b.left + INSET_L + (n === 1 ? 0.5 : M.rank[i] / (n - 1)) * (b.right - b.left - INSET_L - INSET_R);
  }
  /* The rule: between the last row before and the first row after, half a usual step before split_at. */
  function ruleX() {
    const b = B();
    const before = M.order.filter((i) => M.side[i] === 0), after = M.order.filter((i) => M.side[i] === 1);
    const xs = M.order.map(xAt), gaps = [];
    for (let k = 1; k < xs.length; k++) gaps.push(xs[k] - xs[k - 1]);
    gaps.sort((a, c) => a - c);
    const half = gaps.length ? gaps[Math.floor(gaps.length / 2)] / 2 : 0;
    const xa = before.length ? xAt(before[before.length - 1]) : b.left;
    const xb = after.length ? xAt(after[0]) : b.right;
    if (!after.length) return M.tS !== null ? kit.clamp(xOfTime(M.tS), xa, b.right) : b.right;
    if (!before.length) return M.tS !== null ? kit.clamp(xOfTime(M.tS) - half, b.left, xb) : b.left;
    if (M.tS !== null) return kit.clamp(xOfTime(M.tS) - half, xa + (xb - xa) * 0.25, xb - (xb - xa) * 0.25);
    return (xa + xb) / 2;
  }

  function growth(i) {
    if (mode === 'none') return 1;
    if (born[i] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[i], GROW, GROW_BOUNCE));
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  function halo(ctx, text, x, y, color) {
    ctx.save();
    ctx.lineJoin = 'round'; ctx.lineWidth = 3; ctx.strokeStyle = tokens.page;
    ctx.strokeText(text, x, y);
    ctx.fillStyle = color; ctx.fillText(text, x, y);
    ctx.restore();
  }
  const overlap = (a, c) => a.x0 < c.x1 + 4 && c.x0 < a.x1 + 4 && a.y0 < c.y1 + 2 && c.y0 < a.y1 + 2;

  function draw() {
    if (!width || !M.rows.length) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const T = tokens, b = B();
    const L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const n = M.rows.length;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const passed = (x) => (clipX < Infinity ? kit.seg(clipX - x, 0, 24) : 1);
    const rx = Math.round(ruleX()) + 0.5;

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

    /* x labels, as series spaces them, each fading in once the dot has passed it */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textBaseline = 'alphabetic';
    /* every k-th row, so the labels keep an even rhythm, and the last row */
    const most = Math.max(2, Math.floor((R - L) / LABEL_W));
    const every = Math.max(1, Math.ceil((n - 1) / (most - 1)));
    const labelled = [];
    for (let k = 0; k < n; k += every) labelled.push(M.order[k]);
    if (labelled[labelled.length - 1] !== M.order[n - 1]) labelled.push(M.order[n - 1]);
    const placed = labelled.map((i) => {
      const x = xAt(i), text = kit.formatX(M.xs[i], M.dates), w = ctx.measureText(text).width;
      const x0 = kit.clamp(x - w / 2, L, R + PAD.right - w);
      return { x, text, x0, x1: x0 + w };
    });
    const kept = [];
    placed.forEach((p, k) => {
      if (k === placed.length - 1) while (kept.length > 1 && p.x0 < kept[kept.length - 1].x1 + 6) kept.pop();
      if (!kept.length || p.x0 >= kept[kept.length - 1].x1 + 6) kept.push(p);
    });
    for (const p of kept) {
      const a = labelA * passed(p.x);
      if (a <= 0) continue;
      ctx.globalAlpha = a;
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

    /* each side's usual range, and its middle */
    const spans = [[L + 1, rx - 3], [rx + 3, R]];
    for (const s of [0, 1]) {
      const S = M.sides[s];
      if (S.mid === null) continue;
      const [a, z] = spans[s];
      if (z - a < 2) continue;
      const fade = labelA * passed(s ? z : a + (z - a) * 0.5);
      if (fade <= 0) continue;
      ctx.save();
      ctx.globalAlpha = fade;
      const y1 = Y(S.q3), y2 = Y(S.q1);
      ctx.fillStyle = kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.075);
      ctx.fillRect(a, Math.min(y1, y2), z - a, Math.max(1, Math.abs(y2 - y1)));
      dotted(ctx, a + 1.5, Math.round(Y(S.mid)) + 0.5, z - 1.5, Math.round(Y(S.mid)) + 0.5, 3, 1.2, kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.45));
      ctx.restore();
    }

    /* the rule at the change */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    dotted(ctx, rx, top, rx, floor, 3.5, 1.25, kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.55));
    ctx.restore();

    /* the dots, joined faintly on each side and never across the change */
    const hv = hover;
    const dimA = hv !== null ? kit.lerp(1, 0.45, dim.x) : 1;
    const col = T.rgb.s0, st = kit.SERIES_STYLE[0];
    const spacing = (R - L) / Math.max(1, n - 1);
    const headR = kit.clamp(spacing * 0.36, 1.2, st.head);
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    if (spacing >= 7) {
      ctx.strokeStyle = kit.rgba(col, col[3] * 0.32 * dimA); ctx.lineWidth = 1.2; ctx.lineCap = 'round'; ctx.lineJoin = 'round'; ctx.setLineDash([0, 3]);
      for (const s of [0, 1]) {
        ctx.beginPath();
        let pen = false;
        for (const i of M.sides[s].idx) {
          const g = growth(i);
          if (g <= 0) { pen = false; continue; }
          const x = xAt(i), y = floor + (Y(M.values[i]) - floor) * g;
          if (pen) ctx.lineTo(x, y); else ctx.moveTo(x, y);
          pen = true;
        }
        ctx.stroke();
      }
      ctx.setLineDash([]);
    }
    ctx.restore();
    /* the words: the change at the top of its rule, and each side's middle by its band, each placed where it
       covers the fewest dots */
    ctx.save();
    ctx.font = `11px ${T.sans}`; ctx.textBaseline = 'alphabetic';
    const dots = [];
    for (let i = 0; i < n; i++) if (M.values[i] !== null) dots.push({ x: xAt(i), y: Y(M.values[i]) });
    const covers = (bx) => dots.reduce((c, d) => c + (d.x + headR + 2 > bx.x0 && d.x - headR - 2 < bx.x1 && d.y + headR + 1 > bx.y0 && d.y - headR - 1 < bx.y1 ? 1 : 0), 0);
    /* no words across the rule at the change: a line through text reads as struck through */
    const fits = (bx) => bx.x0 >= L + 2 && bx.x1 <= R + PAD.right && bx.y0 >= top - 4 && bx.y1 <= floor - 2 && !(bx.x0 < rx + 5 && bx.x1 > rx - 5);
    const taken = [];
    const at = (x0, y, w, cost) => ({ x0, x1: x0 + w, y0: y - 9, y1: y + 3, y, cost });
    function choose(cands) {
      let best = null, score = Infinity;
      cands.forEach((c, k) => {
        if (!fits(c)) return;
        const sc = covers(c) * 10 + (taken.some((o) => overlap(o, c)) ? 1000 : 0) + c.cost + k * 0.01;
        if (sc < score) { score = sc; best = c; }
      });
      if (best) taken.push(best);
      return best;
    }
    const ruleW = ctx.measureText(M.label).width;
    const rb = choose([at(rx + 6, top + 10, ruleW, 0), at(rx - 6 - ruleW, top + 10, ruleW, 0.05)]);
    const ruleA = labelA * passed(rx);
    if (rb && ruleA > 0) { ctx.globalAlpha = ruleA; halo(ctx, M.label, rb.x0, rb.y, kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.78)); }

    /* A side's label sits by its band when it can do so without covering a dot. When either cannot (a narrow
       column, a crowded band), both go to the readout instead, so a label never floats away from its band. */
    const chosen = [0, 1].map((s) => {
      const S = M.sides[s];
      if (S.mid === null) return null;
      const text = sideLabel(s);
      const w = ctx.measureText(text).width;
      const [a, z] = spans[s];
      const ya = Math.min(Y(S.q3), Y(S.mid) - 3) - 6, yb = Math.max(Y(S.q1), Y(S.mid) + 3) + 13;
      const xs = [a + 4, z - 4 - w, (a + z - w) / 2];
      /* leaving its own side costs a little, so a label stays over its band when it can */
      const spill = (x0) => (x0 < a - 1 || x0 + w > z + 1 ? 0.5 : 0);
      const cands = [];
      [ya, yb, ya - 14].forEach((y, k) => { for (const x0 of xs) cands.push(at(x0, y, w, spill(x0) + k * 0.2)); });
      const bx = choose(cands);
      return bx && covers(bx) === 0 && !taken.some((o) => o !== bx && overlap(o, bx)) ? { bx, text, z } : false;
    });
    const fit = !chosen.includes(false);
    if (fit !== inPlot) { inPlot = fit; readout(); }
    if (fit) for (const c of chosen) {
      if (!c) continue;
      const fade = labelA * passed(c === chosen[1] ? c.z : c.bx.x1);
      if (fade <= 0) continue;
      ctx.globalAlpha = fade;
      halo(ctx, c.text, c.bx.x0, c.bx.y, T.muted);
    }
    ctx.restore();

    /* the dots last, so a label never hides one */
    const heads = [new Path2D(), new Path2D()];
    let ringAt = null;
    for (const s of [0, 1]) for (const i of M.sides[s].idx) {
      const g = growth(i);
      if (g <= 0) continue;
      const x = xAt(i), y = floor + (Y(M.values[i]) - floor) * g;
      const r = headR * kit.clamp(g * 3, 0, 1) + (hv === i ? 0.6 : 0);
      const k = hv !== null && hv !== i ? 1 : 0;
      heads[k].moveTo(x + r, y); heads[k].arc(x, y, r, 0, TAU);
      if (hv === i) ringAt = [x, y];
    }
    ctx.fillStyle = kit.rgba(col); ctx.fill(heads[0]);
    ctx.fillStyle = kit.rgba(col, col[3] * dimA); ctx.fill(heads[1]);
    if (ringAt) {
      ctx.lineWidth = 1; ctx.strokeStyle = kit.rgba(col, Math.min(1, col[3] * 1.1) * 0.9);
      ctx.beginPath(); ctx.arc(ringAt[0], ringAt[1], headR + 1.5 + 3.5 * ring.x, 0, TAU); ctx.stroke();
    }

  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    if (hover === null) {
      if (inPlot) kit.setReadout(parts.readout, [{ swatch: SW_DOT, label: M.vLabel }, { swatch: SW_BAND, label: 'Usual range on each side' }]);
      else kit.setReadout(parts.readout, [0, 1].filter((s) => M.sides[s].mid !== null).map((s, k) => ({ swatch: k ? '' : SW_BAND, label: `${SIDES[s]}, middle`, value: M.fmt(M.sides[s].mid) })));
      return;
    }
    const v = M.values[hover];
    kit.setReadout(parts.readout, [
      { when: kit.formatX(M.xs[hover], M.dates) },
      { swatch: SW_DOT, label: '', value: v === null ? '-' : M.fmt(v) },
      { label: M.side[hover] ? 'After the change' : 'Before the change' },
    ]);
  }

  function setHover(i, from) {
    if (i === hover) { hoverFrom = from; return; }
    hover = i; hoverFrom = from;
    if (i !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop ---------- */
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
      if (clock - revealAt < 0.35 || reveal < Infinity) moving = true;
      for (let i = 0; i < born.length; i++) if (born[i] !== undefined && clock - born[i] < 1.2) { moving = true; break; }
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
    if (mode === 'none') { mode = 'live'; revealAt = clock; }
    reveal = x;
    if (width) markBorn();
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  function nearest(px) {
    let best = null, d = Infinity;
    for (let i = 0; i < M.rows.length; i++) { if (M.values[i] === null) continue; const e = Math.abs(xAt(i) - px); if (e < d) { d = e; best = i; } }
    return best;
  }
  const pick = (e) => nearest(e.clientX - canvas.getBoundingClientRect().left);
  const onMove = (e) => {
    if (!width || !M.rows.length) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!width || !M.rows.length) return;
    const i = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === i) { setHover(null, null); return; }
    setHover(i, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointerleave', onLeave);
  const drawn = () => M.order.filter((i) => M.values[i] !== null);
  const stopKeys = kit.keyNav(parts.plot, {
    count: () => drawn().length,
    get: () => (hover === null ? null : drawn().indexOf(hover)),
    set: (k, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(k === null ? null : drawn()[k], 'key');
    },
  });

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    born = [];
    hover = null; hoverFrom = null;
    kit.setLabel(parts.plot, ariaLabel(M));
    readout();
    kit.foot(parts.foot, out, opts);
    if (mode === 'live') { if (width) markBorn(); rowsWait(reveal < Infinity); }
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    if (mode === 'live') markBorn();
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
