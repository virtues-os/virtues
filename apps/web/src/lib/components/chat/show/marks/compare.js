// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/compare.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* compare.js: a dumbbell plot. Each category drawn as two dots joined by a line, one for each of two moments, so the
   change in every category shows at a glance: before and after, this year and last, two periods side by side.

   A new chart mark, `mark: "dumbbell"`, drawn as a dumbbell rather than a slope. A slope chart puts every category's
   two values on two shared verticals with its label at each end, so categories whose values sit close together
   (seven categories all between six and eight) pile their labels into one another, and past five or six lines
   the crossings are hard to follow. A dumbbell gives each category its own row with its label at the start, holds
   the 20 rows the mark allows, and reads its change as a length along one shared axis.
   - Three columns: the category, then exactly two numeric columns named for the two moments, the earlier first.
     2 to 20 rows, in the order the query returns them. A null leaves that moment's dot out.
   - The earlier moment is drawn in pencil, a hollow dot in the muted ink; the later one in ink, a solid dot. A line
     of ink at 45% joins them. Nothing is colored for a rise or a fall, and nothing is called better or worse.
   - The axis takes the range the values span, as a line chart's does, rounded out to whole ticks: dotted rules up
     from the floor at each tick, and the first one is the spine, with an ink dot at its top and its foot. The labels sit in a column at the left, 24% of the width and between 82px and 136px
     wide, so "Wednesday" fits on a phone; a longer label ends in an ellipsis and is read whole in the readout and
     the figure's label. The column does not follow the labels, because the frame holds the plot's place
     before any label has arrived.
   - Rest on a row, or tab to the chart and use the arrow keys: a ring grows on the later dot, the other rows dim,
     and the readout names both values and the change in words, such as "about 25 minutes shorter". The unit comes
     from the title or the column names (hours, minutes, dollars, steps); without one the change is a plain number.
     A long readout gives up the moments' names first, then the earlier value.
   Height: the rows set it. Before the result arrives the block is drawn for seven rows; a result with another
   count settles it once, as the rows land. Rows are 30px apart up to 8 rows, 26px up to 14 and 22px up to 20.
   Block: readout 26 + plot (244 for seven rows) + foot 26 = 296px.

   Contract: see the top of kit.js. While a query runs past 300 ms the frame's dot runs along the floor (box.bottom). When the
   rows arrive they appear as the frame's sweep passes, through setReveal. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'compare', kinds: ['chart'], marks: ['dumbbell'] });

const MAX_ROWS = 20;
const NOMINAL = 7;
const TOP = 6;
const FLOOR_GAP = 6;           /* between the last row and the floor */
const TICK_ROOM = 22;          /* under the floor, for the tick labels */
const LABEL_GAP = 14;          /* from the label column to the spine */
const RIGHT = 10;
const DOT_R = 3.4, END_R = 1.75;
const TAU = Math.PI * 2;
const TIERS = Object.freeze([
  Object.freeze({ upTo: 8, pitch: 30, line: 20 }),
  Object.freeze({ upTo: 14, pitch: 26, line: 20 }),
  Object.freeze({ upTo: MAX_ROWS, pitch: 22, line: 18 }),
]);
const LABEL_FONT = 15, READOUT_FONT = 13;

const tierFor = (n) => TIERS.find((t) => n <= t.upTo) || TIERS[TIERS.length - 1];
const rowsIn = (input) => (input && Array.isArray(input.rows) && input.rows.length ? kit.clamp(input.rows.length, 1, MAX_ROWS) : NOMINAL);
const floorY = (n) => TOP + n * tierFor(n).pitch + FLOOR_GAP;
const plotHeight = (n) => floorY(n) + TICK_ROOM;

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + plotHeight(rowsIn(input)) + (options.foot === false ? 0 : kit.ROWS.foot);
}

/* The spine, where the label column ends: 24% of the width, from 96px to 150px. */
const spineX = (width) => kit.clamp(Math.round(width * 0.24), 96, 150);

export function box(width, input) {
  const n = rowsIn(input), left = spineX(width);
  return { left, right: Math.max(left + 1, width - RIGHT), top: TOP, bottom: floorY(n), width, height: plotHeight(n) };
}

const CSS = `
.vk-compare .vk-compare-rows { position: absolute; inset: 0; pointer-events: none; }
.vk-compare .vk-compare-lab {
  position: absolute; left: 0;
  font-family: var(--vk-sans); font-size: ${LABEL_FONT}px; color: var(--vk-ink);
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
`;

/* ---------- labels, units and words ---------- */

/* The width of the readout's text, so a long readout gives up its least needed parts first. */
let measureCtx = null;
function textWidth(text, font) {
  if (!measureCtx) measureCtx = document.createElement('canvas').getContext('2d');
  measureCtx.font = font;
  return measureCtx.measureText(text).width;
}

/* The unit, from the value columns' names first, then the title. */
const UNITS = Object.freeze([
  ['hours', /\bhours?\b|\bhrs?\b/],
  ['minutes', /\bminutes?\b|\bmins?\b/],
  ['dollars', /\$|\bdollars?\b|\bspent\b|\bspending\b/],
  ['steps', /\bsteps?\b/],
]);
function unitOf(out, columns) {
  for (const text of [columns.slice(1, 3).join(' '), out && typeof out.title === 'string' ? out.title : '', columns[0] || '']) {
    const s = String(text).toLowerCase();
    for (const [u, re] of UNITS) if (re.test(s)) return u;
  }
  return null;
}

/* Two significant figures at most, for a change said in words. */
function roughly(x) {
  if (x === 0) return 0;
  const mag = 10 ** (Math.floor(Math.log10(Math.abs(x))) - 1);
  return Math.round(x / mag) * mag;
}

function formatValue(v, unit) {
  if (v === null) return '-';
  if (unit === 'hours') {
    const total = Math.round(v * 60), h = Math.floor(Math.abs(total) / 60), m = Math.abs(total) % 60;
    const sign = total < 0 ? '-' : '';
    if (!h) return `${sign}${m} m`;
    return m ? `${sign}${h} h ${m} m` : `${sign}${h} h`;
  }
  if (unit === 'minutes') return `${kit.formatNumber(Math.round(v))} min`;
  if (unit === 'dollars') return `$${kit.formatNumber(v)}`;
  return kit.formatNumber(v);
}
function formatTick(t, unit) {
  if (unit === 'hours') return `${kit.formatNumber(t)} h`;
  if (unit === 'dollars') return `$${kit.formatNumber(t)}`;
  return kit.formatNumber(t);
}

/* The change from then to now in plain words, by how much and which way, never whether that is good. */
function changeWords(a, b, unit) {
  if (a === null || b === null) return '';
  const d = b - a;
  const minutes = unit === 'hours' ? d * 60 : unit === 'minutes' ? d : null;
  if (minutes !== null) {
    const m = Math.abs(minutes);
    if (m < 0.5) return 'about the same';
    const way = minutes > 0 ? 'longer' : 'shorter';
    if (m < 10) { const k = Math.max(1, Math.round(m)); return `about ${k} ${k === 1 ? 'minute' : 'minutes'} ${way}`; }
    if (m < 90) return `about ${Math.round(m / 5) * 5} minutes ${way}`;
    const h = Math.round(m / 30) / 2;
    return `about ${kit.formatNumber(h)} ${h === 1 ? 'hour' : 'hours'} ${way}`;
  }
  const scale = Math.max(Math.abs(a), Math.abs(b), 1e-9);
  if (Math.abs(d) / scale < 0.005) return 'about the same';
  const r = roughly(Math.abs(d));
  if (unit === 'dollars') return `about $${kit.formatNumber(r)} ${d > 0 ? 'more' : 'less'}`;
  if (unit === 'steps') return `about ${kit.formatNumber(r)} ${d > 0 ? 'more' : 'fewer'} steps`;
  return `about ${kit.formatNumber(r)} ${d > 0 ? 'more' : 'less'}`;
}

/* ---------- the model ---------- */
function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const all = Array.isArray(out && out.rows) ? out.rows : [];
  const [catCol, thenCol, nowCol] = columns;
  const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null);
  const rows = all.slice(0, MAX_ROWS).map((r) => ({ label: (r ? kit.formatCell(r[catCol]) : '') || '-', a: r ? num(r[thenCol]) : null, b: r ? num(r[nowCol]) : null }));
  const unit = unitOf(out, columns);
  const values = rows.flatMap((r) => [r.a, r.b]).filter((v) => v !== null);
  let ticks = [0, 1];
  if (values.length) {
    const lo = Math.min(...values), hi = Math.max(...values);
    ticks = kit.niceTicks(lo, hi, 4);
    if (ticks.length < 2) ticks = [lo, hi === lo ? lo + 1 : hi];
  }
  rows.forEach((r) => { r.change = changeWords(r.a, r.b, unit); });
  return { columns, rows, catCol, thenCol, nowCol, unit, ticks, lo: ticks[0], hi: ticks[ticks.length - 1], tier: tierFor(Math.max(1, rows.length)) };
}

function ariaLabel(M) {
  if (!M.rows.length) return 'Dumbbell plot, with no rows';
  const a = kit.columnLabel(M.thenCol || 'then'), b = kit.columnLabel(M.nowCol || 'now');
  const by = kit.columnLabel(M.catCol || 'category').toLowerCase();
  const rows = M.rows.map((r) => `${r.label}: ${a} ${formatValue(r.a, M.unit)}, ${b} ${formatValue(r.b, M.unit)}${r.change ? `, ${r.change}` : ''}`);
  return `Dumbbell plot of ${a} against ${b} for each ${by}. ${rows.join('; ')}.`;
}

/* The readout's marks: the earlier moment's pencil ring and the later one's ink dot. */
const SW_THEN = '<svg class="vk-sw" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><circle cx="5" cy="5" r="3.4" style="fill:var(--vk-page);stroke:var(--vk-muted);stroke-width:1.25"/></svg>';
const SW_NOW = '<svg class="vk-sw" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><circle cx="5" cy="5" r="3.4" style="fill:var(--vk-ink)"/></svg>';

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('compare', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-compare');
  el.appendChild(root);
  let out = output || {};
  let tokens = kit.readTokens(root);
  let M = model(out);
  const parts = kit.block(root, {
    plotHeight: plotHeight(rowsIn(out)), readout: true, foot: opts.foot !== false,
    hint: 'Use the arrow keys to read each row.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  const layer = kit.h('div', 'vk-compare-rows', { 'aria-hidden': 'true' });
  parts.plot.append(canvas, layer);

  let width = 0;
  let rowEls = [];
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  const n = () => Math.max(1, M.rows.length);
  const H = () => plotHeight(n());
  const rowY = (i) => TOP + M.tier.pitch * (i + 0.5);
  const L = () => spineX(width);
  const R = () => Math.max(L() + 1, width - RIGHT);
  const X = (v) => { const l = L(), r = R(); return l + ((v - M.lo) / (M.hi - M.lo || 1)) * (r - l); };

  function buildRows() {
    layer.textContent = '';
    rowEls = M.rows.map((r, i) => {
      const lab = kit.h('span', 'vk-compare-lab');
      lab.textContent = r.label;
      lab.style.top = `${rowY(i) - M.tier.line / 2}px`;
      lab.style.height = lab.style.lineHeight = `${M.tier.line}px`;
      layer.appendChild(lab);
      return { lab, a: -1 };
    });
    sizeLabels();
  }
  function sizeLabels() {
    const w = Math.max(0, L() - LABEL_GAP);
    for (const e of rowEls) e.lab.style.width = `${w}px`;
  }

  function growth(i) {
    if (mode === 'none' || reduced()) return 1;
    if (born[i] === undefined) return 0;
    return Math.max(0, kit.stepSpring(clock - born[i], 0.45, 0.1));
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, H());
    const T = tokens;
    if (!M.rows.length) return;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const l = L(), r = R(), floor = floorY(n()), top = TOP;
    const hv = hover;
    const dimA = hv !== null ? kit.lerp(1, 0.4, dim.x) : 1;

    /* the rules at each tick and the floor, arriving behind the frame's sweep */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, H()); ctx.clip(); }
    for (const t of M.ticks) {
      const x = Math.round(X(t)) + 0.5;
      if (Math.abs(x - l) < 2) continue;
      dotted(ctx, x, floor, x, top, 5, 1.1, T.borderStrong);
    }
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(l, floor, r - l, 1);
    ctx.restore();

    /* the tick labels, each fading in once the dot has passed it */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textBaseline = 'alphabetic';
    let lastEnd = -Infinity;
    M.ticks.forEach((t, k) => {
      const x = X(t), text = formatTick(t, M.unit), w = ctx.measureText(text).width;
      const align = k === 0 ? 'left' : k === M.ticks.length - 1 ? 'right' : 'center';
      const x0 = align === 'left' ? x : align === 'right' ? x - w : x - w / 2;
      if (x0 < lastEnd + 8) return;
      lastEnd = x0 + w;
      const a = labelA * (clipX < Infinity ? kit.seg(clipX - x, 0, 24) : 1);
      if (a <= 0) return;
      ctx.globalAlpha = a; ctx.textAlign = align;
      ctx.fillText(text, x, H() - 6);
    });
    ctx.restore();

    /* the spine: fine dots from the floor to the top with an ink dot at each end */
    dotted(ctx, l, floor, l, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath();
    ctx.moveTo(l + END_R, top); ctx.arc(l, top, END_R, 0, TAU);
    ctx.moveTo(l + END_R, floor); ctx.arc(l, floor, END_R, 0, TAU);
    ctx.fill();

    /* each row: the line, then the earlier dot in pencil and the later dot in ink */
    const ink = T.rgb.ink, muted = T.rgb.muted, page = T.rgb.page;
    for (let i = 0; i < M.rows.length; i++) {
      const row = M.rows[i], y = rowY(i);
      const a = hv !== null && hv !== i ? dimA : 1;
      const xa = row.a === null ? null : X(row.a), xb = row.b === null ? null : X(row.b);
      const first = Math.min(xa ?? Infinity, xb ?? Infinity);
      if (live && born[i] === undefined && first !== Infinity && (reveal === Infinity || reveal >= first)) born[i] = clock;
      const g = growth(i);
      /* a leader of pinpricks from the spine to the row's first dot, so a label finds its dots */
      if (first !== Infinity && first - l > 14) {
        const pins = new Path2D(), end = Math.min(first - DOT_R - 5, clipX);
        for (let x = l + 4.5; x <= end; x += 4.5) { pins.moveTo(x + 0.6, y); pins.arc(x, y, 0.6, 0, TAU); }
        const pc = T.rgb.borderStrong;
        ctx.fillStyle = kit.rgba(pc, pc[3] * a * labelA);
        ctx.fill(pins);
      }
      if (xa !== null && xb !== null && Math.abs(xb - xa) > DOT_R * 2 + 2) {
        /* the line runs between the two dots' edges, and grows with the sweep */
        const x0 = Math.min(xa, xb) + DOT_R + 1, x1 = Math.min(Math.max(xa, xb) - DOT_R - 1, clipX);
        if (x1 > x0) {
          ctx.strokeStyle = kit.rgba(ink, ink[3] * 0.45 * a); ctx.lineWidth = 1.5; ctx.lineCap = 'butt';
          ctx.beginPath(); ctx.moveTo(x0, y); ctx.lineTo(x1, y); ctx.stroke();
        }
      }
      const shown = (x) => x !== null && x <= clipX;
      const rr = DOT_R * kit.clamp(g * 3, 0, 1.15);
      if (shown(xa) && rr > 0.1) {
        ctx.fillStyle = kit.rgba(page);
        ctx.strokeStyle = kit.rgba(muted, muted[3] * a); ctx.lineWidth = 1.25;
        ctx.beginPath(); ctx.arc(xa, y, Math.max(0.1, rr), 0, TAU); ctx.fill(); ctx.stroke();
      }
      if (shown(xb) && rr > 0.1) {
        ctx.fillStyle = kit.rgba(ink, ink[3] * a);
        ctx.beginPath(); ctx.arc(xb, y, Math.max(0.1, rr) + (hv === i ? 0.3 : 0), 0, TAU); ctx.fill();
      }
      if (hv === i) {
        const at = xb !== null ? xb : xa;
        if (at !== null) {
          const rad = DOT_R + 1.5 + 3.5 * ring.x;
          ctx.strokeStyle = kit.rgba(ink, ink[3] * 0.85); ctx.lineWidth = 1;
          ctx.beginPath(); ctx.arc(at, y, rad, 0, TAU); ctx.stroke();
        }
      }
      const e = rowEls[i];
      if (e) {
        const la = labelA * a;
        if (Math.abs(la - e.a) > 0.004) { e.a = la; e.lab.style.opacity = la >= 0.999 ? '' : la.toFixed(3); }
      }
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    if (!M.rows.length) { kit.setReadout(parts.readout, []); return; }
    const a = kit.columnLabel(M.thenCol || 'then'), b = kit.columnLabel(M.nowCol || 'now');
    if (hover === null) { kit.setReadout(parts.readout, [{ swatch: SW_THEN, label: a }, { swatch: SW_NOW, label: b }]); return; }
    const r = M.rows[hover];
    const va = formatValue(r.a, M.unit), vb = formatValue(r.b, M.unit);
    const f = `${READOUT_FONT}px ${tokens.sans}`, fb = `500 ${READOUT_FONT}px ${tokens.sans}`;
    const w = (xs) => xs.reduce((s, [t, font]) => s + textWidth(t, font), 0);
    const change = r.change ? [{ label: r.change }] : [];
    const cw = r.change ? 16 + textWidth(r.change, f) : 0;
    const forms = [
      { parts: [{ when: r.label }, { swatch: SW_THEN, label: a, value: va }, { swatch: SW_NOW, label: b, value: vb }, ...change],
        w: w([[r.label, f], [a, f], [va, fb], [b, f], [vb, fb]]) + 16 * 2 + (10 + 6 + 6) * 2 + cw },
      { parts: [{ when: r.label }, { value: `${va} to ${vb}` }, ...change], w: w([[r.label, f], [`${va} to ${vb}`, fb]]) + 16 + cw },
      { parts: [{ when: r.label }, { swatch: SW_NOW, value: vb }, ...change], w: w([[r.label, f], [vb, fb]]) + 16 + 16 + cw },
      { parts: [{ when: r.label }, ...change], w: w([[r.label, f]]) + cw },
    ];
    const room = (parts.readout.clientWidth || width) - 2;
    const pick = forms.find((x) => x.w <= room) || { parts: [{ label: r.label, value: r.change || vb }] };
    kit.setReadout(parts.readout, pick.parts);
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
    const target = hover !== null ? 1 : 0;
    if (reduced()) { ring.x = 1; ring.v = 0; dim.x = target; dim.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      kit.springTo(dim, target, dt, 0.2, 0);
      moving = ring.x !== 1 || dim.x !== target;
    }
    if (mode === 'live' && !reduced()) {
      if (clock - revealAt < 0.35 || reveal < Infinity) moving = true;
      for (let i = 0; i < born.length; i++) if (born[i] !== undefined && clock - born[i] < 1.2) { moving = true; break; }
    }
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
    reveal = reduced() ? Infinity : x;
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  const rowAt = (py) => (M.rows.length ? kit.clamp(Math.floor((py - TOP) / M.tier.pitch), 0, M.rows.length - 1) : null);
  const pick = (e) => { const r = canvas.getBoundingClientRect(); return rowAt(e.clientY - r.top); };
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
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const count = M.rows.length;
    if (!count) return;
    const i = hover;
    let j;
    switch (e.key) {
      case 'ArrowDown': case 'ArrowRight': j = i === null ? 0 : Math.min(count - 1, i + 1); break;
      case 'ArrowUp': case 'ArrowLeft': j = i === null ? count - 1 : Math.max(0, i - 1); break;
      case 'Home': j = 0; break;
      case 'End': j = count - 1; break;
      case 'Escape':
        if (i === null) return;
        e.preventDefault(); setHover(null, 'key'); return;
      default: return;
    }
    e.preventDefault();
    setHover(j, 'key');
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
    born = [];
    hover = null; hoverFrom = null;
    parts.plot.style.height = `${H()}px`;
    buildRows();
    kit.setLabel(parts.plot, ariaLabel(M));
    readout();
    kit.foot(parts.foot, out, opts);
    if (mode === 'live') rowsWait(reveal < Infinity);
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => { width = w; sizeLabels(); readout(); draw(); });
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
      parts.plot.removeEventListener('pointerdown', onDown);
      parts.plot.removeEventListener('pointerleave', onLeave);
      parts.plot.removeEventListener('keydown', onKey);
      parts.plot.removeEventListener('blur', onBlur);
      root.remove();
    },
  };
}
