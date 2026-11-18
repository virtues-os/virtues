// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/number.js in the design folder).
// Plain JavaScript drawn on a canvas; ShowSeries.svelte mounts it after the paragraph that quotes the figures.
// Type checking is off because the module is untyped JS, not because of an error.

/* number.js: the drill-down figure. A figure in a reply that opens into the rows it summarizes.

   A `numbers` result can name, for each figure, the series that figure summarizes (the `series` field below, which
   show.rs does not have yet). The figure stays in the reply's sentence, where the model quoted it. It carries a fine
   dotted underline in the accent and it is a real button. Pressing it opens the series under the paragraph:
   - The space opens at its final height first, so the words below glide down once and then stay put.
   - The figure's own glyphs are sampled into dots in their own font (lab 12's number tile). The dots lift off the
     line, pour down into the chart and form the same digits again as the label of the rule the figure stands for.
     The rule then runs out of the label across the chart.
   - The rows grow as dots on pinprick stems, in series.js's bar grammar: ink for the rows the figure summarizes,
     pale for the rows shown around them. Stems start at zero, as series bars do.
   - The figure in the sentence keeps a pale trace while its ink is down in the chart.
   Pressing it again plays the same timeline backward: the dots pour back up into the figure, then the space closes.
   One series is open at a time, and pressing another figure folds the open one first.

   The contract it proposes:
     call    { kind: "numbers", sql, series: { "<figure column>": "<SELECT for its series>", ... } }
     result  the numbers result as today, plus
               series: { "<figure column>": { columns, rows, row_count, ref } }
             Each series is the x axis as its first column, one numeric column, and `figure`: the level the figure
             stands for, a number on the rows it summarizes and null on the rows shown around them. A level that
             changes (last month's level, then this week's) draws a step, and the label sits over the step. A series
             has 2 to 199 rows, like a chart. Its ref is the call's ref with the figure's place as the fragment,
             /chat/{chat}/tool/{call}#1, and the model links the figure in its sentence to that ref.
     A date x axis is spaced by day, so a day with no row is an empty slot and two windows stand apart.
   The validation and its error messages are in this component's notes, charts/entries/number.json.

   mount(el, output, options) -> { update(output, patch), destroy(), plot, open(column), close(), opened, progress }
     el        an empty element right after the paragraph that holds the figures. The series opens here.
     output    the numbers result with `series`.
     options   reply      the element that holds the reply's sentence, where the figures are.
               figures    optional: which words in it are figures. Either an array of elements carrying
                          data-vk-figure="<column>" (or the figure's place, 1 to 4), or an object
                          { "<column>": "<the figure's text in the sentence>" }, whose first match is wrapped in a
                          button. Without it, every [data-vk-figure] inside the reply is used, and without those,
                          each figure is found in the reply's text by its value: "7 h 12 m" reads as 7.2 hours and
                          "24 minutes" as 24. In the app, Markdown.svelte marks the figures from the links the model
                          writes, so the value match is the fallback for a reply that did not link them.
               ...and the shared options: onOpenRows, reduced, foot.
   Heights: nothing is drawn where the call sits, so blockHeight is 0. The opened series is openHeight(): a 12px gap,
   the 26px readout, a 160px plot, the 26px foot and an 8px gap, 232px in all. The plot is 160px rather than 200
   because it opens inside the prose between two paragraphs, where a shorter space moves the words below less. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'number', kinds: ['numbers'], marks: [] });

const PLOT_H = 160;
const PAD = Object.freeze({ top: 10, right: 8, bottom: 22, left: 40 });
const GAP_TOP = 12, GAP_BOTTOM = 8;
const LABEL_GAP = 10;        /* from a margin label to the y axis */
const PIN_STEP = 4.95, PIN_R = 0.75, END_R = 1.75;
const HEAD_IN = 3, HEAD_OUT = 2.6;
const RULE_STEP = 4, RULE_R = 0.95;
const PITCH = 1.2, AREA = Math.sqrt(0.866 / Math.PI) * PITCH;
/* One opening, in seconds (lab 12's number tile). Folding plays it backward, faster. */
const OPEN_S = 0.42;
const POUR = 0.55, RULE0 = 0.6, RULE1 = 1.0, CRISP0 = 0.8, CRISP1 = 0.98;
const MARK0 = 0.86, MARK_STEP = 0.018, MARK_GROW = 0.62;
const AXES0 = 0.95, AXES1 = 1.2, ROWS0 = 1.0, ROWS1 = 1.3;
const GHOST = 0.3;
const FOLD_SPEED = 1.5, SWITCH_SPEED = 1.9;
const TAU = Math.PI * 2;

export function blockHeight() {
  return 0;
}

export function openHeight(options = {}) {
  return GAP_TOP + kit.ROWS.readout + PLOT_H + (options.foot === false ? 0 : kit.ROWS.foot) + GAP_BOTTOM;
}

export function box(width) {
  return kit.plotBox(width, PLOT_H, PAD);
}

/* ---------- the series, as the proposed `series` field shapes it ---------- */

function dayNumber(v) {
  const [y, m, d] = String(v).split('-').map(Number);
  return Math.round(Date.UTC(y, m - 1, d) / 864e5);
}

function seriesModel(s) {
  const columns = Array.isArray(s && s.columns) ? s.columns : [];
  const rows = Array.isArray(s && s.rows) ? s.rows : [];
  const xKey = columns[0];
  const vKey = columns.slice(1).find((c) => c !== 'figure') || null;
  const fKey = columns.includes('figure') ? 'figure' : null;
  const xs = rows.map((r) => r[xKey]);
  const dates = rows.length > 0 && kit.isDateColumn(xs) && xs.every((v) => typeof v === 'string');
  let slot, nSlots;
  if (dates) {
    const days = xs.map(dayNumber);
    const first = Math.min(...days);
    slot = days.map((d) => d - first);
    nSlots = Math.max(...days) - first + 1;
  } else {
    slot = rows.map((_, i) => i);
    nSlots = rows.length;
  }
  const order = rows.map((_, i) => i).sort((a, b) => slot[a] - slot[b]);
  const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null);
  const values = rows.map((r) => num(r[vKey]));
  const levels = rows.map((r) => (fKey ? num(r[fKey]) : null));
  const all = values.concat(levels).filter((v) => v !== null);
  let ticks = [0, 1];
  if (all.length) {
    const lo = Math.min(0, ...all), hi = Math.max(0, ...all);
    ticks = kit.niceTicks(lo, hi);
    if (ticks.length < 2) ticks = [lo, hi === lo ? lo + 1 : hi];
  }
  /* The rule: runs of one level in slot order. A level that changes starts a new run. */
  const runs = [];
  for (const i of order) {
    const lv = levels[i];
    if (lv === null) continue;
    const last = runs[runs.length - 1];
    if (last && Math.abs(last.level - lv) < 1e-9) { last.to = slot[i]; last.rows.push(i); }
    else runs.push({ level: lv, from: slot[i], to: slot[i], rows: [i] });
  }
  const marks = order.filter((i) => values[i] !== null);
  /* The usual distance between rows, in slots: 1 for days, 7 for weeks. Dots are sized to it, and a stretch of more
     than three of it is a gap the labels mark. */
  const gaps = marks.slice(1).map((i, k) => slot[i] - slot[marks[k]]).sort((a, b) => a - b);
  const gap = gaps.length ? Math.max(1, gaps[Math.floor(gaps.length / 2)]) : 1;
  /* The rows grow left to right, 18ms apart, all of them within 0.9s. */
  const stagger = Math.min(MARK_STEP, 0.9 / Math.max(1, nSlots));
  return { columns, rows, xKey, vKey, fKey, xs, dates, slot, nSlots, order, values, levels, ticks, yMin: ticks[0], yMax: ticks[ticks.length - 1], runs, marks, gap, stagger };
}

/* ---------- the figure's glyphs as dots, in its own font (lab 12's sampler, from the Edge lab) ---------- */

let scratch = null, sctx = null;
function sampleGlyphs(text, font) {
  if (!scratch) { scratch = document.createElement('canvas'); sctx = scratch.getContext('2d', { willReadFrequently: true }); }
  const S = 4, pad = 3;
  sctx.setTransform(1, 0, 0, 1, 0, 0);
  sctx.font = font;
  const m = sctx.measureText(text);
  const asc = m.fontBoundingBoxAscent || m.actualBoundingBoxAscent || 12;
  const desc = m.fontBoundingBoxDescent || m.actualBoundingBoxDescent || 4;
  const w = Math.ceil(m.width) + pad * 2, hh = Math.ceil(asc + desc) + pad * 2;
  scratch.width = w * S; scratch.height = hh * S;
  sctx.setTransform(S, 0, 0, S, 0, 0);
  sctx.clearRect(0, 0, w, hh);
  /* a coverage mask that is read back and never shown, so any opaque fill will do */
  sctx.font = font; sctx.fillStyle = '#000'; sctx.textBaseline = 'alphabetic';
  sctx.fillText(text, pad, pad + asc);
  const data = sctx.getImageData(0, 0, w * S, hh * S).data, W = w * S, H = hh * S;
  const half = Math.max(1, Math.round((PITCH * S) / 2));
  const ink = (x, y) => {
    let a = 0, n = 0;
    const cx = Math.round((x + pad) * S), cy = Math.round((y + pad) * S);
    for (let j = -half; j < half; j++) for (let i = -half; i < half; i++) {
      const px = cx + i, py = cy + j;
      if (px >= 0 && py >= 0 && px < W && py < H) a += data[(py * W + px) * 4 + 3];
      n++;
    }
    return a / n / 255;
  };
  const pts = [], rowH = PITCH * 0.866;
  for (let y = -1, row = 0; y < asc + desc + 1; y += rowH, row++) {
    for (let x = (row % 2) * PITCH / 2 - 1; x < m.width + 1; x += PITCH) { const a = ink(x, y); if (a > 0.05) pts.push({ x, y, a }); }
  }
  return { pts, w: m.width, asc };
}

function mkRand(seed) {
  let s = seed >>> 0;
  return () => { s = (s + 0x6D2B79F5) >>> 0; let t = s; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}

const bez3 = (a, b, c, d, t) => { const s = 1 - t; return s * s * s * a + 3 * s * s * t * b + 3 * s * t * t * c + t * t * t * d; };

/* ---------- the figure's underline: a button in the prose, outside any .vk root ---------- */

/* The figure sits in the reply's prose, outside the component's root, so it carries the --vk-* variables it uses,
   mapped from the app's tokens the way kit.css maps them. */
function figureCss() {
  const v = (key) => `${kit.TOKENS[key][0]}:var(${kit.TOKENS[key][1]},${kit.TOKENS[key][2]});`;
  return `
.vk-figure{${v('primary')}${v('focus')}
  display:inline;margin:0;padding:0;border:0;border-radius:3px;font:inherit;line-height:inherit;letter-spacing:inherit;
  color:inherit;text-align:inherit;vertical-align:baseline;white-space:nowrap;cursor:pointer;-webkit-tap-highlight-color:transparent;
  background-color:transparent;background-image:linear-gradient(90deg,var(--vk-primary) 1px,transparent 1px);
  background-size:3px 1px;background-repeat:repeat-x;background-position:0 calc(100% - 3px)}
.vk-figure:hover{background-color:color-mix(in srgb,var(--vk-primary) 8%,transparent)}
.vk-figure:focus{outline:none}
.vk-figure:focus-visible{outline:2px solid var(--vk-focus);outline-offset:2px}
.vk-number-plot{user-select:none;-webkit-user-select:none}`;
}

/* ---------- figures in plain prose, found by their value ---------- */

const DURATION = /(\d+)\s*(?:h|hr|hrs|hours?)\s+(?:and\s+)?(\d+)\s*(?:m|min|mins|minutes?)(?![a-z])/gi;
const QUANTITY = /(\d{1,3}(?:,\d{3})+|\d+(?:\.\d+)?)(?:\s*(?:minutes?|mins?|hours?|hrs?|h|m|nights?|days?|weeks?|steps|km|miles?|kg|lb|%))?(?![\w.])/gi;

/* Each candidate figure in a text: its span and the values it can stand for. */
function candidates(text) {
  const out = [];
  for (const m of text.matchAll(DURATION)) {
    const h = Number(m[1]), mm = Number(m[2]);
    out.push({ at: m.index, end: m.index + m[0].length, values: [h + mm / 60, h * 60 + mm] });
  }
  for (const m of text.matchAll(QUANTITY)) {
    const at = m.index, end = at + m[0].length;
    if (out.some((c) => at < c.end && end > c.at)) continue;
    out.push({ at, end, values: [Number(m[1].replace(/,/g, ''))] });
  }
  return out.sort((a, b) => a.at - b.at);
}
const sameValue = (a, b) => Math.abs(a - b) <= Math.max(0.005, Math.abs(b) * 1e-6);

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('number', figureCss());
  let opts = { ...options };
  let out = output || {};
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());

  const root = kit.h('div', 'vk vk-number');
  root.style.position = 'relative';
  root.style.height = '0px';
  /* The open space clips its block while it opens and closes. It reaches 8px past the column on each side so the
     plot's focus ring, drawn 4px outside it, is not cut. */
  const clip = kit.h('div', 'vk-number-open', { id: kit.uid('vk-number') });
  clip.style.cssText = 'position:absolute;left:-8px;right:-8px;top:0;bottom:0;overflow:hidden;visibility:hidden';
  clip.inert = true;
  const inner = kit.h('div');
  inner.style.padding = `${GAP_TOP}px 8px 0`;
  clip.appendChild(inner);
  root.appendChild(clip);
  el.appendChild(root);
  const parts = kit.block(inner, { plotHeight: PLOT_H, readout: true, foot: opts.foot !== false, hint: 'Use the left and right arrow keys to read each row.' });
  parts.plot.classList.add('vk-number-plot');
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);
  let pourCv = null;

  let tokens = kit.readTokens(root);
  let width = 0;
  let figs = [];                    /* { el, column, made, onClick, onKey } */
  let models = {};                  /* column -> series model */
  let want = null, cur = null, u = 0;
  let prep = null;                  /* the open figure's geometry: label, glyph dots, rule dots */
  const fh = { x: 0, v: 0 };
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  const loop = kit.loop(step);

  /* ---------- the figures in the sentence ---------- */
  const seriesOf = (column) => (out && out.series && out.series[column]) || null;
  function resolveColumn(v) {
    if (typeof v !== 'string' || !v) return null;
    if (seriesOf(v)) return v;
    if (/^\d+$/.test(v)) { const c = (out.columns || [])[Number(v) - 1]; return c && seriesOf(c) ? c : null; }
    return null;
  }
  function wrapText(container, text, node, offset) {
    if (!container || !text) return null;
    const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    for (let n = node || walker.nextNode(); n; n = node ? null : walker.nextNode()) {
      if (n.parentElement && n.parentElement.closest('.vk-figure')) continue;
      const at = node ? offset : n.data.indexOf(text);
      if (at < 0) continue;
      const hit = n.splitText(at);
      hit.splitText(text.length);
      const b = kit.h('button', null, { type: 'button' });
      hit.parentNode.insertBefore(b, hit);
      b.appendChild(hit);
      return b;
    }
    return null;
  }
  const prose = () => (opts.reply && opts.reply.nodeType === 1 ? opts.reply : null);
  /* The first unclaimed number in the reply's text whose value is the figure's. */
  function wrapValue(container, value) {
    if (!container || typeof value !== 'number') return null;
    const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (n.parentElement && n.parentElement.closest('.vk-figure')) continue;
      const hit = candidates(n.data).find((c) => c.values.some((v) => sameValue(v, value)));
      if (!hit) continue;
      return wrapText(n.parentNode, n.data.slice(hit.at, hit.end), n, hit.at);
    }
    return null;
  }
  function linkFigures() {
    unlinkFigures();
    const list = [];
    const f = opts.figures, reply = prose();
    const marked = reply ? [...reply.querySelectorAll('[data-vk-figure]')] : [];
    if (Array.isArray(f)) {
      for (const e of f) if (e && e.nodeType === 1) list.push({ el: e, column: resolveColumn(e.getAttribute('data-vk-figure')), made: false });
    } else if (f && typeof f === 'object') {
      for (const column of Object.keys(f)) {
        if (!seriesOf(column)) continue;
        const b = wrapText(reply, String(f[column]));
        if (b) list.push({ el: b, column, made: true });
      }
    } else if (marked.length) {
      for (const e of marked) list.push({ el: e, column: resolveColumn(e.getAttribute('data-vk-figure')), made: false });
    } else if (reply) {
      const row = (out.rows || [])[0] || {};
      for (const column of out.columns || []) {
        if (!seriesOf(column)) continue;
        const b = wrapValue(reply, row[column]);
        if (b) list.push({ el: b, column, made: true });
      }
    }
    for (const g of list) {
      if (!g.column) continue;
      const e = g.el;
      e.classList.add('vk-figure');
      e.setAttribute('data-vk-figure', g.column);
      e.setAttribute('aria-expanded', 'false');
      e.setAttribute('aria-controls', clip.id);
      if (e.tagName !== 'BUTTON') {
        e.setAttribute('role', 'button');
        if (!e.hasAttribute('tabindex')) { e.tabIndex = 0; g.tabbed = true; }
        g.onKey = (ev) => { if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); press(g.column); } };
        e.addEventListener('keydown', g.onKey);
      }
      g.onClick = (ev) => { ev.preventDefault(); press(g.column); };
      e.addEventListener('click', g.onClick);
      figs.push(g);
    }
  }
  function unlinkFigures() {
    for (const g of figs) {
      const e = g.el;
      e.removeEventListener('click', g.onClick);
      if (g.onKey) e.removeEventListener('keydown', g.onKey);
      e.style.color = '';
      if (!e.getAttribute('style')) e.removeAttribute('style');
      if (g.made && e.parentNode) {
        const text = document.createTextNode(e.textContent);
        e.parentNode.replaceChild(text, e);
        if (text.parentNode) text.parentNode.normalize();
        continue;
      }
      e.classList.remove('vk-figure');
      e.removeAttribute('aria-expanded');
      e.removeAttribute('aria-controls');
      if (e.tagName !== 'BUTTON') { e.removeAttribute('role'); if (g.tabbed) e.removeAttribute('tabindex'); }
    }
    figs = [];
  }
  const figFor = (column) => figs.find((g) => g.column === column) || null;
  function syncExpanded() {
    for (const g of figs) {
      const v = String(g.column === want);
      if (g.el.getAttribute('aria-expanded') !== v) g.el.setAttribute('aria-expanded', v);
    }
  }

  function press(column) {
    if (!models[column]) return;
    want = want === column ? null : column;
    syncExpanded();
    loop.wake();
  }

  /* ---------- geometry of the open series ---------- */
  function geometry(M, column) {
    const g = figFor(column);
    const text = g ? g.el.textContent.trim() : '';
    let font = `16px ${tokens.sans}`;
    if (g) { const cs = getComputedStyle(g.el); font = `${cs.fontStyle === 'italic' ? 'normal' : cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`; }
    const mctx = canvas.getContext('2d');
    mctx.font = font;
    const tm = mctx.measureText(text);
    const asc = tm.fontBoundingBoxAscent || tm.actualBoundingBoxAscent || 12;
    const single = M.runs.length === 1;
    const left = single && text ? Math.max(PAD.left, Math.ceil(tm.width) + LABEL_GAP + 6) : PAD.left;
    const b = { left, right: Math.max(left + 10, width - PAD.right), top: PAD.top, bottom: PLOT_H - PAD.bottom };
    const slotW = (b.right - b.left) / Math.max(1, M.nSlots);
    const X = (s) => b.left + (s + 0.5) * slotW;
    const Y = (v) => b.bottom - ((v - M.yMin) / (M.yMax - M.yMin || 1)) * (b.bottom - b.top);
    /* the rule's path and where its label sits */
    const path = [];
    let label = null, from = null;
    if (M.runs.length) {
      const r0 = M.runs[0];
      path.push([b.left, Y(r0.level)]);
      for (let k = 1; k < M.runs.length; k++) {
        const a = M.runs[k - 1], c = M.runs[k];
        const sx = (X(a.to) + X(c.from)) / 2;
        path.push([sx, Y(a.level)], [sx, Y(c.level)]);
        if (from === null) {
          from = sx;
          label = { x: sx - tm.width / 2, base: Math.min(Y(a.level), Y(c.level)) - 9, center: true };
        }
      }
      path.push([b.right, Y(M.runs[M.runs.length - 1].level)]);
      if (!label) label = { x: b.left - LABEL_GAP - tm.width, base: Y(r0.level) + asc * 0.33, center: false };
    }
    if (label) label.x = kit.clamp(label.x, 2, Math.max(2, width - tm.width - 2));
    /* the rule as dots every 4px, each with the moment the flow reaches it */
    const flow = [];
    for (let i = 1; i < path.length; i++) {
      const [ax, ay] = path[i - 1], [bx, by] = path[i], len = Math.hypot(bx - ax, by - ay);
      for (let s = 0; s < len; s += RULE_STEP) { const f = s / len; flow.push({ x: ax + (bx - ax) * f, y: ay + (by - ay) * f, at: 0 }); }
    }
    if (path.length) { const last = path[path.length - 1]; flow.push({ x: last[0], y: last[1], at: 0 }); }
    for (const f of flow) {
      f.at = from === null ? (f.x - b.left) / Math.max(1, b.right - b.left)
        : (Math.abs(f.x - from) / Math.max(1, from - b.left, b.right - from)) * 0.9 + (Math.abs(f.x - from) < 0.5 ? 0 : 0.1);
    }
    return { b, slotW, X, Y, path, flow, label, text, font, asc, labelW: tm.width, single };
  }

  /* Glyph dots: from the figure in the sentence to the label in the chart, in root coordinates. */
  function preparePour(column) {
    const M = models[column];
    const G = geometry(M, column);
    const g = figFor(column);
    const p = { column, M, G, dots: [], top: 0, height: 0, uEnd: MARK0 + M.stagger * M.nSlots + MARK_GROW };
    if (!g || !G.text || !G.label) return p;
    const rr = root.getBoundingClientRect();
    const range = document.createRange();
    range.selectNodeContents(g.el);
    const rect = range.getClientRects()[0] || g.el.getBoundingClientRect();
    if (!rect || !rect.width) return p;
    const smp = sampleGlyphs(G.text, G.font);
    const x0 = rect.left - rr.left, y0 = rect.top - rr.top;
    const plotTop = GAP_TOP + kit.ROWS.readout;
    const lx = G.label.x, ly = plotTop + G.label.base - smp.asc;
    const n = smp.pts.length;
    const order = smp.pts.map((_, i) => i).sort((a, b) => smp.pts[a].x - smp.pts[b].x);
    const rank = new Float32Array(n);
    order.forEach((i, k) => { rank[i] = n > 1 ? k / (n - 1) : 0; });
    const rnd = mkRand(column.length * 97 + n);
    p.dots = smp.pts.map((pt, i) => ({
      sx: x0 + pt.x, sy: y0 + pt.y, tx: lx + pt.x, ty: ly + pt.y, r: AREA * Math.sqrt(pt.a),
      s0: 0.1 + 0.2 * rank[i] + (rnd() - 0.5) * 0.04, bx: (rnd() - 0.5) * 14, drop: 34 + rnd() * 26,
    }));
    p.top = Math.floor(Math.min(y0, plotTop) - 12);
    p.height = Math.ceil(plotTop + PLOT_H - p.top);
    return p;
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (path, x, y, r) => { path.moveTo(x + r, y); path.arc(x, y, r, 0, TAU); };

  function drawChart() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, PLOT_H);
    if (!cur || !prep) return;
    const T = tokens, M = prep.M, G = prep.G, b = G.b;
    const L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const ax = kit.seg(u, AXES0, AXES1);
    const zy = Math.round(G.Y(Math.max(M.yMin, Math.min(0, M.yMax)))) + 0.5;

    /* the grid, the floor and the labels come in once the rule is drawn */
    if (ax > 0) {
      ctx.save();
      ctx.globalAlpha = ax;
      for (const t of M.ticks) {
        const y = Math.round(G.Y(t));
        if (Math.abs(y - floor) < 1) continue;
        dotted(ctx, L, y + 0.5, R, y + 0.5, 5, 1.1, T.borderStrong);
      }
      ctx.fillStyle = T.borderStrong;
      ctx.fillRect(L, floor, R - L, 1);
      ctx.font = `9.5px ${T.mono}`;
      ctx.fillStyle = T.muted;
      ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
      const ly = G.label && !G.label.center ? G.label.base - G.asc * 0.33 : null;
      for (const t of M.ticks) {
        const y = G.Y(t);
        if (ly !== null && Math.abs(y - ly) < 12) continue;
        ctx.fillText(kit.formatNumber(t), L - 6, y);
      }
      ctx.textBaseline = 'alphabetic';
      xLabels(ctx, M, G).forEach(([text, x, align]) => { ctx.textAlign = align; ctx.fillText(text, x, PLOT_H - 6); });
      dotted(ctx, L, floor, L, top, 5.5, 1.3, T.muted);
      ctx.fillStyle = T.ink;
      const ends = new Path2D(); disc(ends, L, top, END_R); disc(ends, L, floor, END_R);
      ctx.fill(ends);
      ctx.restore();
    }

    /* The rows grow as dots on pinprick stems, left to right. A stem is one dashed stroke with round caps, a
       pinprick every 4.95px, which a canvas draws several times faster than an arc per pinprick. */
    const hv = hover;
    const dimA = hv !== null ? kit.lerp(1, 0.45, dim.x) : 1;
    const groups = new Map();
    const group = (key) => { if (!groups.has(key)) groups.set(key, { pins: new Path2D(), heads: new Path2D() }); return groups.get(key); };
    const rings = [];
    for (const i of M.marks) {
      const s = M.slot[i];
      const p = reduced() ? (u >= prep.uEnd ? 1 : 0) : kit.stepSpring(u - (MARK0 + M.stagger * s), 0.45, 0.1);
      if (p <= 0.002) continue;
      const x = G.X(s);
      const vy = zy + (G.Y(M.values[i]) - zy) * p;
      const inFig = M.levels[i] !== null;
      const k = `${inFig ? 'in' : 'out'}${hv !== null && hv !== i ? '-dim' : ''}`;
      const gr = group(k);
      const dir = vy < zy ? -1 : 1;
      if (dir * (vy - zy) > 6.5) { gr.pins.moveTo(x, zy + dir * 0.5); gr.pins.lineTo(x, vy - dir * 6); }
      const r0 = kit.clamp(Math.min(inFig ? HEAD_IN : HEAD_OUT, (G.slotW * M.gap) / 2 - 0.4), 1, HEAD_IN);
      const r = r0 * kit.clamp(p * 4, 0, 1) + (hv === i ? 0.6 : 0);
      disc(gr.heads, x, vy, r);
      if (hv === i) rings.push([x, vy, r0, inFig]);
    }
    ctx.save();
    ctx.lineWidth = PIN_R * 2; ctx.lineCap = 'round'; ctx.setLineDash([0, PIN_STEP]);
    for (const [key, gr] of groups) {
      const inFig = key.startsWith('in');
      const a = key.endsWith('-dim') ? dimA : 1;
      const col = inFig ? T.rgb.ink : T.rgb.subtle;
      ctx.strokeStyle = kit.rgba(col, col[3] * (inFig ? 0.62 : 0.5) * a);
      ctx.stroke(gr.pins);
    }
    ctx.restore();
    for (const [key, gr] of groups) {
      const inFig = key.startsWith('in');
      const a = key.endsWith('-dim') ? dimA : 1;
      const col = inFig ? T.rgb.ink : T.rgb.subtle;
      ctx.fillStyle = kit.rgba(col, col[3] * (inFig ? 1 : 0.8) * a);
      ctx.fill(gr.heads);
    }
    if (rings.length) {
      ctx.save();
      ctx.lineWidth = 1;
      for (const [x, y, r, inFig] of rings) {
        const col = inFig ? T.rgb.ink : T.rgb.subtle;
        ctx.strokeStyle = kit.rgba(col, col[3] * 0.9);
        const rr = r + 1.5 + 3.5 * ring.x;
        ctx.beginPath(); ctx.arc(x, y, rr, 0, TAU); ctx.stroke();
      }
      ctx.restore();
    }

    /* the rule runs out of the label */
    const fr = kit.seg(u, RULE0, RULE1);
    if (fr > 0) {
      const rule = new Path2D();
      for (const f of G.flow) {
        const a = kit.seg(fr, f.at * 0.92, f.at * 0.92 + 0.08);
        if (a > 0) disc(rule, f.x, f.y, RULE_R * a);
      }
      ctx.fillStyle = kit.rgba(T.rgb.primary, T.rgb.primary[3] * 0.9);
      ctx.fill(rule);
    }
    /* the label, crisp once the dots have landed */
    const crisp = kit.seg(u, CRISP0, CRISP1);
    if (crisp > 0 && G.label) {
      ctx.save();
      ctx.globalAlpha = crisp;
      ctx.font = G.font;
      ctx.fillStyle = T.primary;
      ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic';
      ctx.fillText(G.text, G.label.x, G.label.base);
      ctx.restore();
    }
  }

  /* Dates: the first and the last, and the first row after a stretch with no rows, where there is room.
     Categories: spread evenly, about one per 52px, as series.js labels them. */
  function xLabels(ctx, M, G) {
    if (!M.marks.length) return [];
    const at = [];
    const first = M.marks[0], last = M.marks[M.marks.length - 1];
    if (!M.dates) {
      const b = G.b, room = Math.max(2, Math.floor((b.right - b.left) / 52));
      for (const k of kit.labelIndices(M.marks.length, room)) at.push([M.marks[k], 'center']);
    } else {
      at.push([first, 'left']);
      for (let k = 1; k < M.marks.length - 1; k++) {
        const i = M.marks[k], prev = M.marks[k - 1];
        if (M.slot[i] - M.slot[prev] > 3 * M.gap) at.push([i, 'center']);
      }
      if (last !== first) at.push([last, 'right']);
    }
    const out = [];
    let edge = -Infinity;
    at.forEach(([i, align], k) => {
      const text = kit.formatX(M.xs[i], M.dates);
      const w = ctx.measureText(text).width;
      let x = G.X(M.slot[i]);
      if (align === 'left') x -= Math.min(3, G.slotW / 2);
      if (align === 'right') x += Math.min(3, G.slotW / 2);
      const x0 = align === 'left' ? x : align === 'right' ? x - w : x - w / 2;
      const isLast = k === at.length - 1;
      if (x0 < edge + 8) {
        if (!isLast) return;
        while (out.length > 1 && x0 < out[out.length - 1][3] + 8) out.pop();
      }
      out.push([text, x, align, x0 + w]);
      edge = x0 + w;
    });
    return out.map(([text, x, align]) => [text, x, align]);
  }

  function drawPour() {
    const show = prep && prep.dots.length ? kit.seg(u, 0, 0.06) * (1 - kit.seg(u, CRISP0, CRISP1)) : 0;
    if (show <= 0.002) { if (pourCv) pourCv.style.visibility = 'hidden'; return; }
    if (!pourCv) {
      pourCv = kit.h('canvas', 'vk-number-pour', { 'aria-hidden': 'true' });
      pourCv.style.cssText = 'position:absolute;left:0;pointer-events:none;z-index:2';
      root.appendChild(pourCv);
    }
    pourCv.style.visibility = 'visible';
    pourCv.style.top = `${prep.top}px`;
    const ctx = kit.fitCanvas(pourCv, width, prep.height);
    const T = tokens;
    const lift = 2.5 * kit.ease.out(kit.seg(u, 0, 0.14));
    const inkPath = new Path2D(), accentPath = new Path2D();
    for (const P of prep.dots) {
      const pp = kit.seg(u, P.s0, P.s0 + POUR), e = kit.ease.inOut(pp);
      const sy = P.sy - lift;
      const x = bez3(P.sx, P.sx + P.bx, P.tx + P.bx * 0.3, P.tx, e);
      const y = bez3(sy, sy + P.drop, P.ty - 30, P.ty, e) - prep.top;
      disc(pp > 0.5 ? accentPath : inkPath, x, y, P.r * (1 + 1.1 * Math.sin(Math.PI * pp)));
    }
    ctx.globalAlpha = show;
    ctx.fillStyle = T.ink; ctx.fill(inkPath);
    ctx.fillStyle = T.primary; ctx.fill(accentPath);
  }

  /* The figure in the sentence keeps a pale trace while its ink is down in the chart. */
  function ghost() {
    for (const g of figs) {
      const k = g.column === cur ? kit.lerp(1, GHOST, kit.ease.out(kit.seg(u, 0.02, 0.14))) : 1;
      const c = k > 0.996 ? '' : kit.rgba(tokens.rgb.ink, tokens.rgb.ink[3] * k);
      if (g.el.style.color !== c) g.el.style.color = c;
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    const M = cur ? models[cur] : null;
    if (!M || !M.vKey) { kit.setReadout(parts.readout, []); return; }
    if (hover === null) { kit.setReadout(parts.readout, [{ label: kit.columnLabel(M.vKey) }]); return; }
    const v = M.values[hover];
    kit.setReadout(parts.readout, [{ when: kit.formatX(M.xs[hover], M.dates) }, { label: '', value: v === null ? '-' : kit.formatNumber(v) }]);
  }

  function ariaLabel(M, G) {
    if (!M || !M.vKey) return '';
    const fx = (i) => kit.formatX(M.xs[i], M.dates);
    let s = `${kit.columnLabel(M.vKey)} by ${kit.columnLabel(M.xKey || 'row').toLowerCase()}`;
    if (M.marks.length > 1) s += `, ${fx(M.marks[0])} to ${fx(M.marks[M.marks.length - 1])}`;
    if (M.runs.length && G && G.text) {
      const span = (r) => (r.rows.length > 1 ? `${fx(r.rows[0])} to ${fx(r.rows[r.rows.length - 1])}` : fx(r.rows[0]));
      if (M.runs.length === 1) s += `, with a rule at ${kit.formatNumber(M.runs[0].level)} for ${G.text}, across ${span(M.runs[0])}`;
      else s += `, with a rule for ${G.text} that steps from ${M.runs.map((r) => `${kit.formatNumber(r.level)} across ${span(r)}`).join(' to ')}`;
    }
    return `${s}.`;
  }

  function setHover(i, from) {
    if (i === hover) { hoverFrom = from; return; }
    hover = i; hoverFrom = from;
    if (i !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs while the space opens or closes, the dots pour, or a ring grows ---------- */
  let lastFade = -1, lastH = -1;
  function step(dt) {
    const still = reduced();
    if (cur && cur !== want) {
      u = still ? 0 : u - dt * (want ? SWITCH_SPEED : FOLD_SPEED);
      if (u <= 0) { u = 0; cur = null; prep = null; setHover(null, null); }
    }
    /* the space opens at its final height before anything moves into it, and closes once the series has folded */
    const target = want || cur ? openHeight(opts) : 0;
    if (still) { fh.x = target; fh.v = 0; } else kit.springTo(fh, target, dt, OPEN_S, 0);
    if (Math.abs(fh.x - target) < 0.25 && Math.abs(fh.v) < 2) { fh.x = target; fh.v = 0; }
    if (!cur && want && fh.x > target - 3) {
      cur = want; u = 0;
      prep = preparePour(cur);
      kit.setLabel(parts.plot, ariaLabel(prep.M, prep.G));
      kit.foot(parts.foot, seriesOf(cur), opts);
      readout();
    }
    if (cur && cur === want && prep && u < prep.uEnd) u = still ? prep.uEnd : Math.min(prep.uEnd, u + dt);
    if (still) { ring.x = 1; ring.v = 0; dim.x = hover !== null ? 1 : 0; dim.v = 0; }
    else { kit.springTo(ring, 1, dt, 0.28, 0.15); kit.springTo(dim, hover !== null ? 1 : 0, dt, 0.2, 0); }

    const h = fh.x < 0.05 ? 0 : fh.x;
    if (Math.abs(h - lastH) > 0.01) { root.style.height = `${h.toFixed(2)}px`; lastH = h; }
    const shut = h < 0.5 && !want && !cur;
    if (clip.inert !== shut) { clip.inert = shut; clip.style.visibility = shut ? 'hidden' : 'visible'; }
    const fade = cur ? kit.seg(u, ROWS0, ROWS1) : 0;
    if (Math.abs(fade - lastFade) > 0.002) {
      for (const node of [parts.readout, parts.foot]) if (node) node.style.opacity = fade.toFixed(3);
      lastFade = fade;
    }
    ghost();
    drawChart();
    drawPour();
    const settled = Math.abs(fh.x - target) < 0.01 && Math.abs(fh.v) < 0.01;
    const pouring = !!(cur && prep && (cur !== want || u < prep.uEnd));
    const waiting = !cur && !!want;
    const rings = !still && (ring.x !== 1 || dim.x !== (hover !== null ? 1 : 0));
    if (settled && !pouring && !waiting && pourCv && kit.seg(u, CRISP0, CRISP1) >= 1) pourCv.style.visibility = 'hidden';
    return !settled || pouring || waiting || rings;
  }

  /* ---------- pointer and keys on the open chart ---------- */
  const readable = () => !!(cur && cur === want && prep && u >= MARK0);
  function nearest(px) {
    const M = prep.M, G = prep.G;
    let best = null, dist = Infinity;
    for (const i of M.marks) { const d = Math.abs(G.X(M.slot[i]) - px); if (d < dist) { best = i; dist = d; } }
    return best;
  }
  const onMove = (e) => {
    if (!readable()) return;
    const r = canvas.getBoundingClientRect();
    setHover(nearest(e.clientX - r.left), 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onMove);
  parts.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(parts.plot, {
    count: () => (readable() ? prep.M.marks.length : 0),
    get: () => (hover === null || !prep ? null : prep.M.marks.indexOf(hover)),
    set: (k, why) => {
      if (why === 'blur') { if (hoverFrom === 'key') setHover(null, null); return; }
      setHover(k === null || !prep ? null : prep.M.marks[k], 'key');
    },
    page: 7,
  });

  /* ---------- setup and the handle ---------- */
  function apply() {
    models = {};
    const series = (out && out.series) || {};
    for (const column of Object.keys(series)) models[column] = seriesModel(series[column]);
    if (want && !models[want]) want = null;
    if (cur && !models[cur]) { cur = null; prep = null; u = 0; }
    linkFigures();
    syncExpanded();
    if (cur) {
      prep = preparePour(cur);
      kit.setLabel(parts.plot, ariaLabel(prep.M, prep.G));
      kit.foot(parts.foot, seriesOf(cur), opts);
    }
    hover = null; hoverFrom = null;
    readout();
    loop.wake();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    if (cur) prep = preparePour(cur);
    drawChart();
    loop.wake();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; loop.wake(); }
  });
  const stopDpr = kit.watchDpr(() => loop.wake());
  const stopMotion = kit.watchReducedMotion(() => loop.wake());
  apply();

  return {
    get plot() { return parts.plot; },
    get opened() { return want; },
    /* how far the open series is along its opening, 0 to 1; it runs back to 0 as it folds */
    get progress() { return cur && prep ? Math.min(1, u / prep.uEnd) : 0; },
    open(column) { if (models[column] && want !== column) press(column); },
    close() { if (want) { want = null; syncExpanded(); loop.wake(); } },
    update(next, patch) {
      if (patch) opts = { ...opts, ...patch };
      out = next || {};
      apply();
    },
    destroy() {
      loop.stop(); stopKeys(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerdown', onMove);
      parts.plot.removeEventListener('pointerleave', onLeave);
      unlinkFigures();
      root.remove();
    },
  };
}
