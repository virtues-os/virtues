// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/multiples.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* multiples.js: small multiples. The same small chart once for each group, on one shared scale, so the groups
   compare by shape and by level and the one that stands apart shows next to the others. A chart mark: kind
   "chart", mark "multiples", for one pattern across groups such as weekdays, months, places, kinds of activity or
   accounts, where one chart with many lines would tangle.

   Each panel is a small line of dots in the dot grammar of series.js: a dotted grid at the shared ticks, a floor,
   and the first series' line with a dot at each value. Every panel has the same y scale and the same x span, so a
   panel that sits lower reads as lower without a number. The panels wrap into a grid that fits the column: on a
   phone seven panels sit three to a row, in the chat column four.
   - Panels keep the order they arrive in (ORDER BY the group), up to 12. x is shared across every panel: dates or
     times are spaced by time, anything else by its order of first appearance.
   - Rest on a point, or tab to the chart and use the arrow keys (left and right move along the panel, up and down
     move to the previous or next panel, Home and End go to its ends, Escape clears): the point grows a ring, the
     same x lights in every panel with a fine guide and a dot, and the readout names the panel, the x and the value.
   - One accent: every panel's line is the primary. The panel you read has its name in ink; nothing is ranked,
     scored or colored as good or bad.
   - The value is a number, or a duration as sql_query writes an interval, read as spread.js reads it, so a
     duration reads 6 h 5 m. A null value breaks a panel's line.

   Columns: panel (the group's name, found by the name panel, else the first column), then the x axis and the value.
   2 to 12 panels and up to 400 rows.

   Contract: see the top of kit.js. Block: readout 26 + plot 240 + foot 26 = 292px at any width, because the frame
   opens at blockHeight before any data. The grid wraps inside the fixed 240px: rows share it evenly. box() is the
   whole grid: its left is the shared axis, its top the first row's top and its bottom the last row's floor, so the
   frame's waiting dot runs along the last row. Nothing runs while nothing moves. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'multiples', kinds: ['chart'], marks: ['multiples'] });

const HEIGHT = 240;
const PAD = Object.freeze({ top: 6, right: 6, bottom: 22, left: 40 });
const NAME_H = 15;              /* a panel's name sits in the 15px above its plot */
const GAP_X = 14;
const INSET = 4;                /* the first and last x sit this far inside a panel */
const MIN_W = 56, MIN_H = 34;
const MAX_PANELS = 12, MAX_ROWS = 400;
const END_R = 1.75;
const GROW = 0.45, GROW_BOUNCE = 0.1;
const TAU = Math.PI * 2;
const HINT = 'Use the left and right arrow keys to read a panel point by point, and the up and down arrow keys to move between panels at the same point.';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return { left: PAD.left, right: Math.max(PAD.left, width - PAD.right), top: PAD.top + NAME_H, bottom: HEIGHT - PAD.bottom, width, height: HEIGHT };
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
/* Durations tick in whole hours once the span passes two hours, so a label stays short enough for the margin. */
const DURATION_STEPS = [60, 300, 600, 900, 1800, 3600, 7200, 10800, 21600, 43200, 86400];
function durationTicks(lo, hi, step) {
  const out = [];
  for (let t = Math.floor(lo / step) * step; t <= Math.ceil(hi / step) * step + 1e-9; t += step) out.push(t);
  return out;
}
/* The finest ticks whose dotted gridlines sit at least GRID_MIN px apart in a plot `px` tall, and how often to
   label them so the labels sit at least LABEL_MIN px apart: a short phone row keeps hourly lines and labels every
   other one, rather than stretching the scale to fit three labels. */
const GRID_MIN = 8, LABEL_MIN = 13;
function ticksFor(kind, lo, hi, px) {
  const cands = [];
  if (kind !== 'duration') for (let c = 6; c >= 1; c--) cands.push(kit.niceTicks(lo, hi, c));
  else {
    if (!(hi > lo)) { lo -= 1800; hi += 1800; }
    for (const s of DURATION_STEPS) if (hi - lo < 7200 || s >= 3600) cands.push(durationTicks(lo, hi, s));
  }
  for (const t of cands) {
    if (t.length < 2 || t.length > 7) continue;
    const gap = px / (t.length - 1);
    if (gap >= GRID_MIN) return { ticks: t, every: Math.max(1, Math.ceil(LABEL_MIN / gap)) };
  }
  const t = cands.filter((c) => c.length >= 2).pop() || [lo, hi === lo ? lo + 1 : hi];
  return { ticks: [t[0], t[t.length - 1]], every: 1 };
}

/* ---------- the model ---------- */

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = (Array.isArray(out && out.rows) ? out.rows : []).slice(0, MAX_ROWS);
  const pKey = columns.find((c) => typeof c === 'string' && c.trim().toLowerCase() === 'panel') || columns[0];
  const rest = columns.filter((c) => c !== pKey);
  const xKey = rest[0], vKey = rest[1];
  const kind = vKey ? valueKind(rows.map((r) => r[vKey])) : null;
  /* panels in the order they arrive */
  const panels = [];
  const byName = new Map();
  for (const r of rows) {
    const name = r[pKey] === null || r[pKey] === undefined ? '' : String(r[pKey]);
    if (!byName.has(name)) {
      if (panels.length >= MAX_PANELS) continue;
      const p = { k: panels.length, name: name || 'No group', pts: [] };
      byName.set(name, p); panels.push(p);
    }
  }
  /* the shared x: a time, or the order each x first appears in */
  const xsAll = rows.filter((r) => byName.has(String(r[pKey] ?? ''))).map((r) => r[xKey]);
  const dates = kit.isDateColumn(xsAll) || kit.isTimeColumn(xsAll);
  const domain = [];
  const index = new Map();
  for (const x of xsAll) { const id = String(x); if (!index.has(id)) { index.set(id, domain.length); domain.push(x); } }
  let pos;
  if (dates) {
    const ts = domain.map((x) => kit.parseTime(x).getTime());
    const order = domain.map((_, i) => i).sort((a, b) => ts[a] - ts[b]);
    const lo = Math.min(...ts), hi = Math.max(...ts);
    pos = domain.map((_, i) => (hi === lo ? 0.5 : (ts[i] - lo) / (hi - lo)));
    /* the domain in time order, so keys and labels run left to right */
    const sorted = order.map((i) => domain[i]), sortedPos = order.map((i) => pos[i]);
    domain.length = 0; domain.push(...sorted); pos = sortedPos;
    index.clear(); domain.forEach((x, i) => index.set(String(x), i));
  } else pos = domain.map((_, i) => (domain.length === 1 ? 0.5 : i / (domain.length - 1)));
  for (const r of rows) {
    const p = byName.get(String(r[pKey] ?? ''));
    if (!p || !index.has(String(r[xKey]))) continue;
    const xi = index.get(String(r[xKey]));
    p.pts.push({ xi, f: pos[xi], v: kind ? readValue(r[vKey], kind) : null });
  }
  for (const p of panels) p.pts.sort((a, b) => a.xi - b.xi);
  const values = panels.flatMap((p) => p.pts.map((q) => q.v)).filter((v) => v !== null);
  return { columns, rows, pKey, xKey, vKey, kind, panels, domain, pos, dates, values, lo: values.length ? Math.min(...values) : 0, hi: values.length ? Math.max(...values) : 1 };
}

/* The group's name for the words, from the panel column's name: "Weekday" gives weekday, and a column named panel
   gives group. */
const groupWord = (M) => { const w = kit.columnLabel(M.pKey || 'group').toLowerCase(); return w === 'panel' ? 'group' : w; };
const median = (a) => { const s = a.slice().sort((x, y) => x - y); const m = (s.length - 1) / 2; return (s[Math.floor(m)] + s[Math.ceil(m)]) / 2; };

function ariaLabel(M) {
  const what = kit.columnLabel(M.vKey || 'value');
  if (!M.kind || !M.panels.length) return `Small multiples of ${what}, with no values to draw.`;
  const group = groupWord(M);
  const names = M.panels.map((p) => p.name);
  const list = names.length === 1 ? names[0] : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
  let s = `Small multiples of ${what} by ${kit.columnLabel(M.xKey || 'row')}, one panel for each ${group}, all on one scale: ${list}.`;
  const typical = M.panels.map((p) => { const v = p.pts.map((q) => q.v).filter((x) => x !== null); return v.length ? { name: p.name, m: median(v) } : null; }).filter(Boolean);
  if (typical.length >= 3 && M.hi > M.lo) {
    typical.sort((a, b) => a.m - b.m);
    const low = typical[0], high = typical[typical.length - 1];
    if ((typical[1].m - low.m) / (M.hi - M.lo) > 0.08) s += ` ${low.name} sits lowest, apart from the others.`;
    else if ((high.m - typical[typical.length - 2].m) / (M.hi - M.lo) > 0.08) s += ` ${high.name} sits highest, apart from the others.`;
    else s += ` Typically ${low.name} is lowest and ${high.name} highest.`;
  }
  return s;
}

/* ---------- the grid: as many columns as give each panel a comfortable shape ---------- */

function grid(width, n) {
  const inner = Math.max(1, width - PAD.left - PAD.right);
  let best = null;
  for (let c = 1; c <= Math.max(1, n); c++) {
    const r = Math.ceil(n / c);
    const pw = (inner - (c - 1) * GAP_X) / c, rowH = (HEIGHT - PAD.top) / r, ph = rowH - NAME_H - PAD.bottom;
    const ok = pw >= MIN_W && ph >= MIN_H;
    const score = (ok ? 0 : 100) + Math.abs(Math.log(pw / Math.max(1, ph) / 1.5)) + 0.04 * (r * c - n);
    if (!best || score < best.score) best = { c, r, pw, rowH, ph, score };
  }
  return best;
}

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-multiples');
  el.appendChild(root);
  const parts = kit.block(root, { plotHeight: HEIGHT, readout: true, foot: opts.foot !== false, hint: HINT });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let G = null;                       /* the layout at this width */
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null; /* { p, j }: a panel and an index into its points */
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = new Map();
  let clock = 0, revealAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);
  const measure = document.createElement('canvas').getContext('2d');

  /* ---------- layout ---------- */
  function layout() {
    G = null;
    if (!width || !M.panels.length || !M.kind) return;
    const g = grid(width, M.panels.length);
    const { ticks, every } = ticksFor(M.kind, M.lo, M.hi, g.ph);
    const yMin = ticks[0], yMax = ticks[ticks.length - 1];
    const cells = M.panels.map((p, k) => {
      const col = k % g.c, row = Math.floor(k / g.c);
      const x0 = PAD.left + col * (g.pw + GAP_X), rowTop = PAD.top + row * g.rowH;
      const top = rowTop + NAME_H, floor = rowTop + g.rowH - PAD.bottom;
      return { p, col, row, x0, x1: x0 + g.pw, top, floor, nameY: rowTop + 10, below: k + g.c >= M.panels.length };
    });
    G = { ...g, cells, ticks, every, yMin, yMax };
  }
  const X = (cell, f) => cell.x0 + INSET + f * (G.pw - 2 * INSET);
  const Y = (cell, v) => cell.floor - ((v - G.yMin) / (G.yMax - G.yMin || 1)) * (cell.floor - cell.top);
  /* how far the frame's sweep has come, as a share of the grid's width: every panel grows together */
  const revealFrac = () => {
    if (reveal === Infinity) return Infinity;
    const b = box(width);
    return (reveal - b.left) / Math.max(1, b.right - b.left);
  };

  function growth(key) {
    if (mode === 'none') return 1;
    const t = born.get(key);
    if (t === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - t, GROW, GROW_BOUNCE));
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (p, x, y, r) => { p.moveTo(x + r, y); p.arc(x, y, r, 0, TAU); };
  const fit = (ctx, text, w) => {
    if (ctx.measureText(text).width <= w) return text;
    let t = text;
    while (t.length > 1 && ctx.measureText(`${t}…`).width > w) t = t.slice(0, -1);
    return `${t}…`;
  };

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    if (!G) return;
    const T = tokens, b = box(width);
    const live = mode === 'live';
    const frac = live ? revealFrac() : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const hv = hover;
    const hxi = hv ? M.panels[hv.p].pts[hv.j].xi : null;
    const dimA = hv ? kit.lerp(1, 0.45, dim.x) : 1;
    const st = kit.SERIES_STYLE[0], col = T.rgb.s0;
    ctx.font = `9.5px ${T.mono}`;

    /* the shared axis: dotted down each row's plot, with an ink dot at the top and the foot */
    const rows = new Map();
    for (const c of G.cells) rows.set(c.row, c);
    for (const c of rows.values()) dotted(ctx, b.left, c.floor, b.left, c.top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    const ends = new Path2D();
    disc(ends, b.left, b.top, END_R); disc(ends, b.left, b.bottom, END_R);
    ctx.fill(ends);
    ctx.save();
    ctx.globalAlpha = labelA;
    ctx.fillStyle = T.muted; ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    for (const c of rows.values()) G.ticks.forEach((t, k) => { if (k % G.every === 0) ctx.fillText(format(M.kind, t, true), b.left - 6, Y(c, t)); });
    ctx.restore();

    for (const cell of G.cells) {
      const p = cell.p;
      const clipX = frac === Infinity ? Infinity : X(cell, kit.clamp(frac, 0, 1)) + 3;
      /* the grid and the floor, arriving behind the sweep */
      ctx.save();
      if (clipX < Infinity) { ctx.beginPath(); ctx.rect(cell.x0 - 1, 0, clipX - cell.x0 + 1, HEIGHT); ctx.clip(); }
      for (const t of G.ticks) {
        const y = Math.round(Y(cell, t));
        if (Math.abs(y - cell.floor) < 1) continue;
        dotted(ctx, cell.x0, y + 0.5, cell.x1, y + 0.5, 5, 1.1, T.borderStrong);
      }
      ctx.fillStyle = T.borderStrong;
      ctx.fillRect(cell.x0, Math.round(cell.floor), cell.x1 - cell.x0, 1);
      ctx.restore();

      /* the panel's name, in ink while you read it */
      ctx.save();
      ctx.globalAlpha = labelA;
      ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic';
      ctx.fillStyle = hv && hv.p === p.k ? T.ink : T.muted;
      ctx.fillText(fit(ctx, p.name, G.pw), cell.x0, cell.nameY);
      /* the first and last x under the panels with none below them */
      if (cell.below && M.domain.length) {
        const first = kit.formatX(M.domain[0], M.dates), last = kit.formatX(M.domain[M.domain.length - 1], M.dates);
        const w1 = ctx.measureText(first).width, w2 = ctx.measureText(last).width;
        ctx.fillStyle = T.muted;
        const y = cell.floor + 16;
        const a = clipX < Infinity ? kit.seg(clipX - cell.x0, 0, 24) : 1;
        ctx.globalAlpha = labelA * a;
        ctx.fillText(first, cell.x0, y);
        if (M.domain.length > 1 && w1 + w2 + 8 <= G.pw) {
          ctx.globalAlpha = labelA * (clipX < Infinity ? kit.seg(clipX - cell.x1, 0, 24) : 1);
          ctx.textAlign = 'right'; ctx.fillText(last, cell.x1, y);
        }
      }
      ctx.restore();

      /* the same x in every panel: a fine guide */
      const same = hxi === null ? -1 : p.pts.findIndex((q) => q.xi === hxi);
      if (hxi !== null) {
        const x = X(cell, M.pos[hxi]);
        dotted(ctx, x, cell.top, x, cell.floor, 4, 1, T.borderStrong);
      }

      /* the line and its dots */
      ctx.save();
      if (clipX < Infinity) { ctx.beginPath(); ctx.rect(cell.x0 - 4, 0, clipX - cell.x0 + 4, HEIGHT); ctx.clip(); }
      ctx.strokeStyle = kit.rgba(col, col[3] * (hv ? kit.lerp(1, 0.7, dim.x) : 1));
      ctx.lineWidth = st.line * 0.85; ctx.lineJoin = 'round'; ctx.lineCap = 'round';
      ctx.beginPath();
      let pen = false;
      const pts = [];
      p.pts.forEach((q, j) => {
        const g = growth(`${p.k}:${j}`);
        if (q.v === null || g <= 0) { pen = false; return; }
        const x = X(cell, q.f), y = cell.floor + (Y(cell, q.v) - cell.floor) * g;
        if (pen) ctx.lineTo(x, y); else ctx.moveTo(x, y);
        pen = true;
        pts.push([j, x, y, g]);
      });
      ctx.stroke();
      const spacing = (G.pw - 2 * INSET) / Math.max(1, M.domain.length - 1);
      const r0 = spacing >= 6 ? 2 : 0;
      const heads = [new Path2D(), new Path2D()];
      for (const [j, x, y, g] of pts) {
        const lit = j === same;
        const r = (lit ? 2.2 : r0) * kit.clamp(g * 3, 0, 1) + (lit && hv.p === p.k ? 0.6 : 0);
        if (r <= 0) continue;
        disc(heads[hv && !lit ? 1 : 0], x, y, r);
      }
      ctx.fillStyle = kit.rgba(col); ctx.fill(heads[0]);
      ctx.fillStyle = kit.rgba(col, col[3] * dimA); ctx.fill(heads[1]);
      ctx.restore();
      if (hv && hv.p === p.k && same >= 0) {
        const at = pts.find(([j]) => j === same);
        if (at) {
          ctx.save();
          ctx.lineWidth = 1;
          ctx.strokeStyle = kit.rgba(col, Math.min(1, col[3] * 1.1) * 0.9);
          ctx.beginPath(); ctx.arc(at[1], at[2], 2.2 + 1.5 + 3.5 * ring.x, 0, TAU); ctx.stroke();
          ctx.restore();
        }
      }
    }
  }

  /* ---------- the readout ---------- */
  function fits(form) {
    let w = 0;
    form.forEach((p, k) => {
      if (k) w += 16;
      measure.font = `13px ${tokens.sans}`;
      if (p.when !== undefined) { w += measure.measureText(p.when).width; return; }
      if (p.label) w += measure.measureText(p.label).width + 6;
      measure.font = `500 13px ${tokens.sans}`;
      if (p.value !== undefined) w += measure.measureText(p.value).width;
    });
    return w <= Math.max(0, width - 4);
  }
  function readout() {
    if (!parts.readout) return;
    const vLabel = kit.columnLabel(M.vKey || 'value');
    if (!M.kind || !M.panels.length) { kit.setReadout(parts.readout, M.vKey ? [{ label: vLabel }] : []); return; }
    if (hover === null) {
      const group = groupWord(M);
      const forms = [[{ label: vLabel }, { label: `One panel for each ${group}, on one scale` }], [{ label: vLabel }, { label: 'On one scale' }], [{ label: vLabel }]];
      kit.setReadout(parts.readout, forms.find(fits) || forms[forms.length - 1]);
      return;
    }
    const p = M.panels[hover.p], q = p.pts[hover.j];
    const x = kit.formatX(M.domain[q.xi], M.dates);
    const v = q.v === null ? 'No value' : format(M.kind, q.v);
    const forms = [
      [{ when: p.name }, { label: kit.columnLabel(M.xKey || 'x'), value: x }, { label: vLabel, value: v }],
      [{ when: p.name }, { label: '', value: x }, { label: vLabel, value: v }],
      [{ when: p.name }, { label: x, value: v }],
    ];
    kit.setReadout(parts.readout, forms.find(fits) || forms[forms.length - 1]);
  }

  function setHover(next, from) {
    const same = (next === null && hover === null) || (next && hover && next.p === hover.p && next.j === hover.j);
    hoverFrom = next ? from : null;
    if (same) return;
    const sameX = next && hover && M.panels[next.p].pts[next.j].xi === M.panels[hover.p].pts[hover.j].xi;
    hover = next;
    if (next && !sameX) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    else if (next) { ring.x = reduced() ? 1 : 0.4; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows, the others dim, or the panels grow ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    const want = hover ? 1 : 0;
    if (reduced()) { ring.x = 1; ring.v = 0; dim.x = want; dim.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      kit.springTo(dim, want, dt, 0.2, 0);
      moving = ring.x !== 1 || dim.x !== want;
    }
    if (mode === 'live' && !reduced()) {
      if (clock - revealAt < 0.35 || reveal < Infinity) moving = true;
      for (const t of born.values()) if (clock - t < 1.2) { moving = true; break; }
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    if (!G) return;
    const frac = revealFrac();
    for (const p of M.panels) p.pts.forEach((q, j) => {
      const key = `${p.k}:${j}`;
      if (!born.has(key) && (frac === Infinity || q.f <= frac)) born.set(key, clock);
    });
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
    markBorn();
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  function pickAt(px, py) {
    if (!G) return null;
    let best = null, dist = Infinity;
    for (const cell of G.cells) {
      const rowTop = cell.top - NAME_H;
      if (py < rowTop - 2 || py > cell.floor + PAD.bottom) continue;
      const dx = px < cell.x0 ? cell.x0 - px : px > cell.x1 ? px - cell.x1 : 0;
      if (dx < dist) { dist = dx; best = cell; }
    }
    if (!best || dist > GAP_X) return null;
    let j = null, d = Infinity;
    best.p.pts.forEach((q, k) => { if (q.v === null) return; const dd = Math.abs(X(best, q.f) - px); if (dd < d) { d = dd; j = k; } });
    return j === null ? null : { p: best.p.k, j };
  }
  const pick = (e) => { const r = canvas.getBoundingClientRect(); return pickAt(e.clientX - r.left, e.clientY - r.top); };
  const onMove = (e) => {
    if (!G) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    const hit = pick(e);
    if (!hit) { if (hoverFrom === 'pointer') setHover(null, null); return; }
    setHover(hit, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!G) return;
    const hit = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hit && hover && hit.p === hover.p && hit.j === hover.j) { setHover(null, null); return; }
    if (hit) setHover(hit, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointerleave', onLeave);

  /* Left and right move along a panel's points, up and down to the previous or next panel at the same x. */
  const valued = (p) => M.panels[p].pts.map((q, j) => (q.v === null ? -1 : j)).filter((j) => j >= 0);
  function inPanel(p, xi, dirn) {
    const js = valued(p);
    if (!js.length) return null;
    const pts = M.panels[p].pts;
    const exact = js.find((j) => pts[j].xi === xi);
    if (exact !== undefined) return exact;
    let best = js[0], d = Infinity;
    for (const j of js) { const dd = Math.abs(pts[j].xi - xi); if (dd < d || (dd === d && dirn > 0)) { d = dd; best = j; } }
    return best;
  }
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey || !G) return;
    const n = M.panels.length;
    let next = hover ? { ...hover } : null;
    const first = (end) => { for (let p = 0; p < n; p++) { const js = valued(p); if (js.length) return { p, j: end ? js[js.length - 1] : js[0] }; } return null; };
    switch (e.key) {
      case 'ArrowRight': case 'ArrowLeft': {
        if (!next) { next = first(e.key === 'ArrowLeft'); break; }
        const js = valued(next.p), at = js.indexOf(next.j);
        next.j = js[kit.clamp(at + (e.key === 'ArrowRight' ? 1 : -1), 0, js.length - 1)];
        break;
      }
      case 'ArrowUp': case 'ArrowDown': {
        if (!next) { next = first(false); break; }
        const xi = M.panels[next.p].pts[next.j].xi, dirn = e.key === 'ArrowDown' ? 1 : -1;
        for (let p = next.p + dirn; p >= 0 && p < n; p += dirn) { const j = inPanel(p, xi, dirn); if (j !== null) { next = { p, j }; break; } }
        break;
      }
      case 'Home': case 'End': {
        const p = next ? next.p : 0, js = valued(p);
        if (!js.length) return;
        next = { p, j: e.key === 'Home' ? js[0] : js[js.length - 1] };
        break;
      }
      case 'Escape':
        if (!hover) return;
        e.preventDefault(); setHover(null, 'key'); return;
      default: return;
    }
    e.preventDefault();
    if (next) setHover(next, 'key');
  };
  const onBlur = () => { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); };
  parts.plot.addEventListener('keydown', onKey);
  parts.plot.addEventListener('blur', onBlur);

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    born = new Map();
    hover = null; hoverFrom = null;
    layout();
    kit.setLabel(parts.plot, ariaLabel(M));
    readout();
    kit.foot(parts.foot, out, opts);
    if (mode === 'live') { markBorn(); rowsWait(reveal < Infinity); }
    loop.wake();
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    layout();
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
