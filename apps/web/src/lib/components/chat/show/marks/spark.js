// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/spark.js in the design folder).
// Plain JavaScript drawn on a canvas; ShowSeries.svelte mounts it after the paragraph that quotes the figures.
// Type checking is off because the module is untyped JS, not because of an error.

/* spark.js: the sparkline. A word-sized trend set in the reply's sentence, right after the figure it belongs to.

   It takes number.js's contract: a numbers result whose `series` field maps each figure's column to the rows that
   figure summarizes (charts/entries/number.json). The call adds mark "spark", so the client draws sparklines where
   it would otherwise make the figures open. The figure stays plain text where the model quoted it. Right after it
   sits a sparkline 56 by 14 px on the text's baseline:
   - A dotted line of one dot for each row, spaced by day: pale for the rows shown around the figure's rows and ink
     for the rows the figure summarizes, ending in a dot in the accent: the last row, the one closest to now.
   - It is shorter than the line's ascent and sits on the baseline, so the line box keeps its height. Nothing about
     it moves after it is laid out: resting on it or focusing it lays a small label over the text above, out of the
     flow, and the words never move.
   - Resting on it or focusing it reads the trend in words, such as "Oct 18 to Nov 16: between 5 h 48 m and 8 h 6 m,
     with no steady rise or fall. Last, Nov 16: 6 h 45 m." The values take the figure's own unit: a figure quoted
     as 7 h 12 m reads its rows as hours and minutes, and one quoted as 41 minutes reads them in minutes.
   - It is role img with that sentence as its aria-label, and it takes focus. The left and right arrow keys step
     through the rows, a ring marks the row and an aria-live region reads it; Home and End jump to the ends and
     Escape clears. Enter, or a click, calls onOpenRows with the series' ref when both are given.
   - A series whose figure has more than one level compares two windows (last month's level against this week's), so
     it has no single trend, and that figure gets no sparkline.
   - It draws itself left to right once when it mounts, 0.55 s; under reduced motion it is there at once.

   mount(el, output, options) -> { update(output, patch), destroy(), plot, sparks }
     el        an empty element right after the paragraph. It holds the aria-live region and the hints, 0px tall.
     output    the numbers result with `series`, and mark "spark".
     options   reply, figures and the shared options, exactly as number.js reads them: the figures are the
               [data-vk-figure] elements in the reply, or the `figures` option, or found in the reply's text by value.
   Heights: nothing is drawn where the call sits, so blockHeight is 0. The sparkline is box(): 56 by 14 px. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'spark', kinds: ['numbers'], marks: ['spark'] });

const W = 56, H = 14;
const DOT_OUT = 0.95, DOT_IN = 1.05, END_R = 2.1;
const DRAW_S = 0.55, STAGGER = 0.12;
const TAU = Math.PI * 2;
const HINT_KEYS = 'Use the left and right arrow keys to read each row.';
const HINT_ROWS = ' Press Enter to see the rows.';

export function blockHeight() {
  return 0;
}

export function box() {
  return { left: 1.5, right: W - END_R - 1, top: END_R + 0.5, bottom: H - 1.5, width: W, height: H };
}

/* ---------- the series, as number.js reads it ---------- */

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
  const slot = dates ? (() => { const d = xs.map(dayNumber), f = Math.min(...d); return d.map((x) => x - f); })() : rows.map((_, i) => i);
  const num = (v) => (typeof v === 'number' && Number.isFinite(v) ? v : null);
  const values = rows.map((r) => num(r[vKey]));
  const levels = rows.map((r) => (fKey ? num(r[fKey]) : null));
  const marks = rows.map((_, i) => i).filter((i) => values[i] !== null).sort((a, b) => slot[a] - slot[b]);
  const lv = [...new Set(levels.filter((v) => v !== null).map((v) => v.toFixed(9)))];
  const lo = marks.length ? Math.min(...marks.map((i) => values[i])) : 0;
  const hi = marks.length ? Math.max(...marks.map((i) => values[i])) : 1;
  const span = marks.length ? slot[marks[marks.length - 1]] - slot[marks[0]] : 0;
  return { columns, rows, xKey, vKey, xs, dates, slot, values, levels, marks, lo, hi, span, levelCount: lv.length, ref: s && s.ref };
}

/* ---------- figures in plain prose, found by their value (number.js's matcher, with the unit kept) ---------- */

const DURATION = /(\d+)\s*(?:h|hr|hrs|hours?)\s+(?:and\s+)?(\d+)\s*(?:m|min|mins|minutes?)(?![a-z])/gi;
const QUANTITY = /(\d{1,3}(?:,\d{3})+|\d+(?:\.\d+)?)(?:\s*(minutes?|mins?|hours?|hrs?|h|m|nights?|days?|weeks?|steps|km|miles?|kg|lb|%))?(?![\w.])/gi;

function candidates(text) {
  const out = [];
  for (const m of text.matchAll(DURATION)) {
    const h = Number(m[1]), mm = Number(m[2]);
    out.push({ at: m.index, end: m.index + m[0].length, values: [h + mm / 60, h * 60 + mm], units: ['hm-hours', 'hm-minutes'] });
  }
  for (const m of text.matchAll(QUANTITY)) {
    const at = m.index, end = at + m[0].length;
    if (out.some((c) => at < c.end && end > c.at)) continue;
    out.push({ at, end, values: [Number(m[1].replace(/,/g, ''))], units: [(m[2] || '').toLowerCase()] });
  }
  return out.sort((a, b) => a.at - b.at);
}
const sameValue = (a, b) => Math.abs(a - b) <= Math.max(0.005, Math.abs(b) * 1e-6);

/* The unit a figure was quoted in, from its text in the sentence. */
function unitOf(text, value) {
  for (const c of candidates(text)) {
    const k = c.values.findIndex((v) => typeof value !== 'number' || sameValue(v, value));
    if (k >= 0) return c.units[k];
  }
  return '';
}

function fmtHM(minutes) {
  const total = Math.round(minutes), h = Math.floor(total / 60), m = total % 60;
  return h ? `${h} h ${m} m` : `${m} m`;
}
/* A figure quoted as "7 h 12 m" reads its rows in hours and minutes, whichever of the two its rows count in. A
   figure with a word after it ("41 minutes") reads its rows in that word, singular for 1. */
const WORDS = Object.freeze({ minute: 'minutes', hour: 'hours', night: 'nights', day: 'days', week: 'weeks', step: 'steps', mile: 'miles' });
function formatterFor(unit) {
  if (unit === 'hm-hours') return (v) => fmtHM(v * 60);
  if (unit === 'hm-minutes') return (v) => fmtHM(v);
  if (unit === '%') return (v) => `${kit.formatNumber(v)}%`;
  const one = unit.replace(/s$/, '');
  if (WORDS[one]) return (v) => `${kit.formatNumber(v)} ${Math.abs(v) === 1 ? one : WORDS[one]}`;
  if (unit) return (v) => `${kit.formatNumber(v)} ${unit}`;
  return (v) => kit.formatNumber(v);
}

/* ---------- the trend in words ---------- */

function trend(M, fmt) {
  const P = M.marks;
  if (P.length < 2) return '';
  const fx = (i) => kit.formatX(M.xs[i], M.dates);
  const first = P[0], last = P[P.length - 1];
  const xs = P.map((i) => M.slot[i]), ys = P.map((i) => M.values[i]);
  const n = P.length, mx = xs.reduce((a, b) => a + b, 0) / n, my = ys.reduce((a, b) => a + b, 0) / n;
  let sxx = 0, sxy = 0, syy = 0;
  for (let k = 0; k < n; k++) { sxx += (xs[k] - mx) ** 2; sxy += (xs[k] - mx) * (ys[k] - my); syy += (ys[k] - my) ** 2; }
  const slope = sxx ? sxy / sxx : 0;
  const sd = Math.sqrt(syy / n);
  const change = slope * (xs[n - 1] - xs[0]);
  const lowest = P.reduce((a, i) => (M.values[i] < M.values[a] ? i : a), P[0]);
  const highest = P.reduce((a, i) => (M.values[i] > M.values[a] ? i : a), P[0]);
  const range = `${fx(first)} to ${fx(last)}`;
  const tail = `Last, ${fx(last)}: ${fmt(M.values[last])}.`;
  if (sd === 0) return `${range}: ${fmt(M.values[first])} on every row. ${tail}`;
  if (Math.abs(change) < 0.6 * sd) return `${range}: between ${fmt(M.values[lowest])} and ${fmt(M.values[highest])}, with no steady rise or fall. ${tail}`;
  const a = my + slope * (xs[0] - mx), b = my + slope * (xs[n - 1] - mx);
  return `${range}: ${change > 0 ? 'rising' : 'falling'}, from about ${fmt(a)} to about ${fmt(b)}. ${tail}`;
}

function sparkCss() {
  return `
.vk-spark-fig{white-space:nowrap}
.vk.vk-spark{display:inline-block;position:relative;width:${W}px;height:${H}px;margin:0 0 0 4px;padding:0;vertical-align:baseline;line-height:0;border-radius:2px;outline:none;-webkit-tap-highlight-color:transparent;user-select:none;-webkit-user-select:none}
.vk.vk-spark.vk-spark-rows{cursor:pointer}
.vk.vk-spark:focus-visible{outline:2px solid var(--vk-focus);outline-offset:2px}
.vk.vk-spark>canvas{display:block}
.vk-spark-tip{position:absolute;left:0;bottom:calc(100% + 8px);z-index:3;width:max-content;max-width:300px;margin:0;padding:5px 9px;border:1px solid var(--vk-border);border-radius:var(--vk-radius-sm);background:var(--vk-background);color:var(--vk-ink);font-family:var(--vk-sans);font-style:normal;font-weight:400;font-size:13px;line-height:18px;letter-spacing:normal;white-space:normal;text-align:left;font-variant-numeric:tabular-nums;pointer-events:none;visibility:hidden;opacity:0;transform:translateY(2px);transition:opacity .16s cubic-bezier(.2,0,0,1),transform .16s cubic-bezier(.2,0,0,1),visibility 0s linear .16s}
.vk-spark-tip.vk-on{visibility:visible;opacity:1;transform:none;transition:opacity .16s cubic-bezier(.2,0,0,1),transform .16s cubic-bezier(.2,0,0,1),visibility 0s}
@media (prefers-reduced-motion: reduce){.vk-spark-tip,.vk-spark-tip.vk-on{transition:none;transform:none}}`;
}

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('spark', sparkCss());
  let opts = { ...options };
  let out = output || {};
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());

  /* The root holds what the prose cannot: the live region and the hints the sparklines point at. */
  const root = kit.h('div', 'vk vk-spark-root');
  root.style.cssText = 'position:relative;height:0;overflow:visible';
  const live = kit.h('span', 'vk-sr', { 'aria-live': 'polite', 'aria-atomic': 'true' });
  const hintKeys = kit.h('span', 'vk-sr', { id: kit.uid('vk-spark-hint') });
  hintKeys.textContent = HINT_KEYS;
  const hintRows = kit.h('span', 'vk-sr', { id: kit.uid('vk-spark-hint') });
  hintRows.textContent = HINT_KEYS + HINT_ROWS;
  root.append(live, hintKeys, hintRows);
  el.appendChild(root);

  let tokens = kit.readTokens(root);
  let sparks = [];     /* { column, M, fmt, words, fig, made, el, canvas, tip, born, focus, show, from, stop } */
  let clock = 0;
  const loop = kit.loop(step);

  /* ---------- finding the figures ---------- */
  const seriesOf = (column) => (out && out.series && out.series[column]) || null;
  const prose = () => (opts.reply && opts.reply.nodeType === 1 ? opts.reply : null);
  function resolveColumn(v) {
    if (typeof v !== 'string' || !v) return null;
    if (seriesOf(v)) return v;
    if (/^\d+$/.test(v)) { const c = (out.columns || [])[Number(v) - 1]; return c && seriesOf(c) ? c : null; }
    return null;
  }
  function wrapRange(n, at, len) {
    const hit = n.splitText(at);
    hit.splitText(len);
    const span = kit.h('span');
    hit.parentNode.insertBefore(span, hit);
    span.appendChild(hit);
    return span;
  }
  const claimed = (n) => n.parentElement && n.parentElement.closest('.vk-spark-fig, .vk-spark');
  function wrapText(container, text) {
    if (!container || !text) return null;
    const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (claimed(n)) continue;
      const at = n.data.indexOf(text);
      if (at >= 0) return wrapRange(n, at, text.length);
    }
    return null;
  }
  function wrapValue(container, value) {
    if (!container || typeof value !== 'number') return null;
    const walker = document.createTreeWalker(container, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (claimed(n)) continue;
      const hit = candidates(n.data).find((c) => c.values.some((v) => sameValue(v, value)));
      if (hit) return wrapRange(n, hit.at, hit.end - hit.at);
    }
    return null;
  }
  function findFigures() {
    const list = [];
    const f = opts.figures, reply = prose();
    const marked = reply ? [...reply.querySelectorAll('[data-vk-figure]')] : [];
    const row = (out.rows || [])[0] || {};
    if (Array.isArray(f)) {
      for (const e of f) if (e && e.nodeType === 1) list.push({ el: e, column: resolveColumn(e.getAttribute('data-vk-figure')), made: false });
    } else if (f && typeof f === 'object') {
      for (const column of Object.keys(f)) { if (!seriesOf(column)) continue; const s = wrapText(reply, String(f[column])); if (s) list.push({ el: s, column, made: true }); }
    } else if (marked.length) {
      for (const e of marked) list.push({ el: e, column: resolveColumn(e.getAttribute('data-vk-figure')), made: false });
    } else if (reply) {
      for (const column of out.columns || []) {
        if (!seriesOf(column) || seriesModel(seriesOf(column)).levelCount > 1) continue;
        const s = wrapValue(reply, row[column]);
        if (s) list.push({ el: s, column, made: true });
      }
    }
    return list.filter((g) => g.column);
  }

  /* ---------- building one sparkline ---------- */
  function build() {
    clear();
    const row = (out.rows || [])[0] || {};
    for (const g of findFigures()) {
      const M = seriesModel(seriesOf(g.column));
      if (M.levelCount > 1 || M.marks.length < 2 || !M.vKey) {
        if (g.made) unwrap(g.el);
        continue;
      }
      const fmt = formatterFor(unitOf(g.el.textContent, row[g.column]));
      const words = trend(M, fmt);
      const rows = !!(M.ref && typeof opts.onOpenRows === 'function');
      const sp = kit.h('span', `vk vk-spark${rows ? ' vk-spark-rows' : ''}`, {
        role: 'img', tabindex: '0', 'aria-label': ariaLabel(M, fmt, words, g.el.textContent.trim()),
        'aria-describedby': (rows ? hintRows : hintKeys).id,
      });
      const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
      const tip = kit.h('span', 'vk-spark-tip', { 'aria-hidden': 'true' });
      sp.append(canvas, tip);
      g.el.classList.add('vk-spark-fig');
      g.el.appendChild(sp);
      const S = { column: g.column, M, fmt, words, fig: g.el, made: g.made, el: sp, canvas, tip, rows,
        born: clock + sparks.length * STAGGER, focus: null, show: false, from: null };
      wire(S);
      sparks.push(S);
    }
    loop.wake();
  }

  function ariaLabel(M, fmt, words, figText) {
    const what = kit.columnLabel(M.vKey || 'value');
    const inFig = M.marks.filter((i) => M.levels[i] !== null);
    const fx = (i) => kit.formatX(M.xs[i], M.dates);
    const dark = inFig.length && inFig.length < M.marks.length
      ? ` The rows that ${figText} sums up, ${fx(inFig[0])} to ${fx(inFig[inFig.length - 1])}, are drawn darker.`
      : '';
    return `Sparkline of ${what.charAt(0).toLowerCase() + what.slice(1)}. ${words}${dark}`;
  }

  function wire(S) {
    const show = (on, from) => {
      S.show = on; S.from = on ? from : null;
      if (on) { tipText(S); placeTip(S); }
      S.tip.classList.toggle('vk-on', on);
    };
    const onEnter = (e) => { if (e.pointerType !== 'touch') show(true, 'pointer'); };
    const onLeave = (e) => { if (e.pointerType !== 'touch' && S.from === 'pointer' && document.activeElement !== S.el) show(false); };
    const onDown = (e) => {
      if (e.pointerType !== 'touch') return;
      if (S.show && S.from === 'touch') show(false); else show(true, 'touch');
    };
    const onClick = () => { if (S.rows) opts.onOpenRows(S.M.ref); };
    const onFocus = () => show(true, 'focus');
    const onKey = (e) => { if (e.key === 'Enter' && S.rows) { e.preventDefault(); opts.onOpenRows(S.M.ref); } };
    S.el.addEventListener('pointerenter', onEnter);
    S.el.addEventListener('pointerleave', onLeave);
    S.el.addEventListener('pointerdown', onDown);
    S.el.addEventListener('click', onClick);
    S.el.addEventListener('focus', onFocus);
    S.el.addEventListener('keydown', onKey);
    const stopKeys = kit.keyNav(S.el, {
      count: () => S.M.marks.length,
      get: () => S.focus,
      set: (k, why) => {
        if (why === 'blur') { S.focus = null; show(false); draw(S); return; }
        S.focus = k;
        if (k === null) { live.textContent = ''; show(true, 'focus'); draw(S); return; }
        const i = S.M.marks[k];
        const text = `${kit.formatX(S.M.xs[i], S.M.dates)}: ${S.fmt(S.M.values[i])}`;
        live.textContent = text;
        show(true, 'key');
        draw(S);
      },
      page: 7,
    });
    S.stop = () => {
      stopKeys();
      S.el.removeEventListener('pointerenter', onEnter);
      S.el.removeEventListener('pointerleave', onLeave);
      S.el.removeEventListener('pointerdown', onDown);
      S.el.removeEventListener('click', onClick);
      S.el.removeEventListener('focus', onFocus);
      S.el.removeEventListener('keydown', onKey);
    };
  }

  function tipText(S) {
    let text = S.words;
    if (S.focus !== null) { const i = S.M.marks[S.focus]; text = `${kit.formatX(S.M.xs[i], S.M.dates)}: ${S.fmt(S.M.values[i])}`; }
    if (S.tip.textContent !== text) S.tip.textContent = text;
  }

  /* The label sits over the text above the sparkline and stays inside the reply's column. */
  function placeTip(S) {
    S.tip.style.left = '0px';
    const reply = prose();
    const bound = (reply || document.documentElement).getBoundingClientRect();
    const sr = S.el.getBoundingClientRect(), tr = S.tip.getBoundingClientRect();
    let shift = 0;
    if (tr.right > bound.right) shift = bound.right - tr.right;
    if (sr.left + shift < bound.left) shift = bound.left - sr.left;
    S.tip.style.left = `${Math.round(shift)}px`;
  }

  /* ---------- drawing ---------- */
  function progress(S) {
    if (reduced()) return 1;
    return kit.ease.out(kit.seg(clock - S.born, 0, DRAW_S));
  }

  function draw(S) {
    const ctx = kit.fitCanvas(S.canvas, W, H);
    const T = tokens, M = S.M, b = box();
    const P = M.marks;
    const span = M.span || 1;
    const X = (i) => b.left + (M.slot[i] - M.slot[P[0]]) / span * (b.right - b.left);
    const Y = (v) => (M.hi === M.lo ? (b.top + b.bottom) / 2 : b.bottom - ((v - M.lo) / (M.hi - M.lo)) * (b.bottom - b.top));
    const p = progress(S);
    const clipX = b.left + p * (b.right - b.left + END_R + 1) + 0.5;
    /* One dot for each row, so the dotted line is the rows themselves: pale for the rows shown around the figure's,
       ink for the rows the figure summarizes. The last row is the accent dot below. */
    const lastI = P[P.length - 1];
    const pale = new Path2D(), dark = new Path2D();
    for (const i of P) {
      if (i === lastI) continue;
      const x = X(i);
      if (x > clipX) break;
      const inFig = M.levels[i] !== null;
      const r = inFig ? DOT_IN : DOT_OUT;
      (inFig ? dark : pale).moveTo(x + r, Y(M.values[i]));
      (inFig ? dark : pale).arc(x, Y(M.values[i]), r, 0, TAU);
    }
    ctx.fillStyle = kit.rgba(T.rgb.muted, T.rgb.muted[3] * 0.72);
    ctx.fill(pale);
    ctx.fillStyle = kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.9);
    ctx.fill(dark);
    const endG = reduced() ? 1 : kit.clamp(kit.stepSpring(clock - S.born - DRAW_S * 0.85, 0.3, 0.25), 0, 1.25);
    if (endG > 0) {
      ctx.fillStyle = T.primary;
      ctx.beginPath(); ctx.arc(X(lastI), Y(M.values[lastI]), END_R * endG, 0, TAU); ctx.fill();
    }
    if (S.focus !== null) {
      const i = P[S.focus];
      ctx.lineWidth = 1;
      ctx.strokeStyle = kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.9);
      ctx.beginPath(); ctx.arc(X(i), Y(M.values[i]), 2.9, 0, TAU); ctx.stroke();
      if (i !== lastI) { ctx.fillStyle = T.ink; ctx.beginPath(); ctx.arc(X(i), Y(M.values[i]), 1.3, 0, TAU); ctx.fill(); }
    }
  }

  /* ---------- the loop: runs only while a sparkline draws itself ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    for (const S of sparks) {
      draw(S);
      if (!reduced() && clock - S.born < DRAW_S + 0.6) moving = true;
    }
    return moving;
  }

  /* ---------- teardown ---------- */
  function unwrap(e) {
    const parent = e.parentNode;
    if (!parent) return;
    while (e.firstChild) parent.insertBefore(e.firstChild, e);
    parent.removeChild(e);
    parent.normalize();
  }
  function clear() {
    for (const S of sparks) {
      S.stop();
      S.el.remove();
      if (S.made) unwrap(S.fig);
      else S.fig.classList.remove('vk-spark-fig');
    }
    sparks = [];
    live.textContent = '';
  }

  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; for (const S of sparks) draw(S); }
  });
  const stopDpr = kit.watchDpr(() => { for (const S of sparks) draw(S); });
  const stopMotion = kit.watchReducedMotion(() => loop.wake());
  build();

  return {
    get plot() { return sparks.length ? sparks[0].el : null; },
    get sparks() { return sparks.map((S) => S.el); },
    update(next, patch) {
      if (patch) opts = { ...opts, ...patch };
      out = next || {};
      build();
    },
    destroy() {
      loop.stop(); stopTheme(); stopDpr(); stopMotion();
      clear();
      root.remove();
    },
  };
}
