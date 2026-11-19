// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/list.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* list.js: the sorted bar list. Named categories as a short list, largest first, each with a bar and its value.

   A new chart mark, `mark: "list"`, in the dot grammar of series.js:
   - Two columns: the first is the label, the second the value, a number or null. 2 to 20 rows, drawn in the order
     the query returns them, which is largest first; the client never re-sorts what the box sent.
   - Each row is the label in the reply's type with its value at the right, in tabular figures, and the bar under
     them. The label has the column's whole width less the value, so a long label stays readable; one that still
     does not fit ends in an ellipsis and is read whole in the readout and the figure's label.
   - The bar is a run of dots in the primary over a track of pinpricks, ending in a larger dot at the value, so the
     track is the scale and the bar fills it. The longest bar reaches the end of the track. Bars start at zero, on a
     spine of fine dots with an ink dot at its top and its foot.
   - Rest on a row, or tab to the list and use the arrow keys: a ring grows on its bar's end, the other rows dim, and
     the readout names the row and its value. Home and End jump to the ends and Escape clears it.
   - The list is never a ranking of people: the tool's sentence says so, and the show.rs check in the entry's notes
     refuses the usual shape of one.
   Height: the rows set it. Before the result arrives the block is drawn for seven rows; a result with another
   count settles it once, as the rows land. Rows are 34px apart up to 8 rows, 31px up to 12 and 28px up to 20.
   Block: readout 26 + plot (248 for seven rows) + foot 26 = 300px.

   Contract: see the top of kit.js. While a query runs past 300 ms the frame's dot runs along the last bar's track
   (box.bottom). When the rows arrive the bars fill as the frame's sweep passes, through setReveal. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'list', kinds: ['chart'], marks: ['list'] });

const MAX_ROWS = 20;
const NOMINAL = 7;             /* the rows a list is drawn for before its result says how many */
const TOP = 6, BOTTOM = 12;
const SPINE_X = 2;             /* the spine: every bar starts here */
const INDENT = 12;             /* the labels start here, clear of the spine */
const RIGHT = 8;               /* room right of the longest bar for the ring */
const DOT = 4.5;               /* the track's pitch */
const PIN_R = 0.7, BAR_R = 1.45, HEAD_R = 2.6, END_R = 1.75;
const TAU = Math.PI * 2;
/* Row pitch by count: the label's line, then the bar's center under it. */
const TIERS = Object.freeze([
  Object.freeze({ upTo: 8, pitch: 34, line: 20, bar: 26 }),
  Object.freeze({ upTo: 12, pitch: 31, line: 20, bar: 25 }),
  Object.freeze({ upTo: MAX_ROWS, pitch: 28, line: 18, bar: 22 }),
]);
const READOUT_FONT = 13;

const tierFor = (n) => TIERS.find((t) => n <= t.upTo) || TIERS[TIERS.length - 1];
const rowsIn = (input) => (input && Array.isArray(input.rows) && input.rows.length ? kit.clamp(input.rows.length, 1, MAX_ROWS) : NOMINAL);
const plotHeight = (n) => { const t = tierFor(n); return TOP + (n - 1) * t.pitch + t.bar + BOTTOM; };

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + plotHeight(rowsIn(input)) + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width, input) {
  const n = rowsIn(input), t = tierFor(n);
  return { left: SPINE_X, right: Math.max(SPINE_X + 1, width - RIGHT), top: TOP + t.bar, bottom: TOP + (n - 1) * t.pitch + t.bar, width, height: plotHeight(n) };
}

const CSS = `
.vk-list .vk-list-rows { position: absolute; inset: 0; pointer-events: none; }
.vk-list .vk-list-row {
  position: absolute; left: ${INDENT}px; right: ${RIGHT}px;
  display: flex; align-items: baseline; gap: 12px;
  font-family: var(--vk-sans); font-size: 15px; color: var(--vk-ink); white-space: nowrap;
}
.vk-list .vk-list-lab { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.vk-list .vk-list-val { flex: none; font-variant-numeric: lining-nums tabular-nums; }
`;

/* ---------- the model ---------- */
function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const all = Array.isArray(out && out.rows) ? out.rows : [];
  const labelCol = columns[0], valueCol = columns[1];
  const rows = all.slice(0, MAX_ROWS).map((r) => {
    const raw = r ? r[valueCol] : null;
    const v = typeof raw === 'number' && Number.isFinite(raw) ? raw : null;
    const name = r ? kit.formatCell(r[labelCol]) : '';
    return { label: name || '-', value: v };
  });
  const max = Math.max(0, ...rows.map((r) => (r.value === null ? 0 : r.value)));
  return { columns, rows, labelCol, valueCol, max, tier: tierFor(Math.max(1, rows.length)) };
}

const valueText = (v) => (v === null ? '-' : kit.formatNumber(v));

function ariaLabel(M) {
  if (!M.rows.length) return 'Sorted bar list with no rows';
  const what = kit.columnLabel(M.valueCol || 'value'), by = kit.columnLabel(M.labelCol || 'label').toLowerCase();
  return `Sorted bar list of ${what} by ${by}, largest first: ${M.rows.map((r) => `${r.label}, ${valueText(r.value)}`).join('; ')}.`;
}

/* The width of the readout's text, so a long label falls back to a form that keeps the value in view. */
let measureCtx = null;
function textWidth(text, font) {
  if (!measureCtx) measureCtx = document.createElement('canvas').getContext('2d');
  measureCtx.font = font;
  return measureCtx.measureText(text).width;
}

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('list', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-list');
  el.appendChild(root);
  let out = output || {};
  let M = model(out);
  const parts = kit.block(root, {
    plotHeight: plotHeight(rowsIn(out)), readout: true, foot: opts.foot !== false,
    hint: 'Use the arrow keys to read each row.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  const layer = kit.h('div', 'vk-list-rows', { 'aria-hidden': 'true' });
  parts.plot.append(canvas, layer);

  let width = 0;
  let tokens = kit.readTokens(root);
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
  const H = () => plotHeight(Math.max(1, M.rows.length));
  const rowTop = (i) => TOP + i * M.tier.pitch;
  const barY = (i) => rowTop(i) + M.tier.bar;
  const trackEnd = () => Math.max(SPINE_X + 1, width - RIGHT);
  const xOf = (v) => SPINE_X + (M.max > 0 ? kit.clamp(v, 0, M.max) / M.max : 0) * (trackEnd() - SPINE_X);

  function buildRows() {
    layer.textContent = '';
    rowEls = M.rows.map((r, i) => {
      const row = kit.h('div', 'vk-list-row');
      row.style.top = `${rowTop(i)}px`;
      row.style.height = row.style.lineHeight = `${M.tier.line}px`;
      const lab = kit.h('span', 'vk-list-lab');
      lab.textContent = r.label;
      const val = kit.h('span', 'vk-list-val');
      val.textContent = valueText(r.value);
      row.append(lab, val);
      layer.appendChild(row);
      return { row, lab, val, a: -1, va: -1 };
    });
  }

  function growth(i) {
    if (mode === 'none' || reduced()) return 1;
    if (born[i] === undefined) return 0;
    return Math.max(0, kit.stepSpring(clock - born[i], 0.45, 0.1));
  }

  /* ---------- drawing ---------- */
  function draw() {
    if (!width) return;
    const height = H();
    const ctx = kit.fitCanvas(canvas, width, height);
    const T = tokens, n = M.rows.length;
    if (!n) return;
    const live = mode === 'live';
    const rx = live ? reveal : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const R = trackEnd();
    const hv = hover;
    const dimA = hv !== null ? kit.lerp(1, 0.45, dim.x) : 1;
    const P = T.rgb.s0, pin = T.rgb.borderStrong;
    /* a dot near the sweep grows in as the frame's sweep passes it */
    const grown = (x) => (rx === Infinity ? 1 : kit.ease.out(kit.seg(rx - x, 0, 18)));

    /* the spine: fine dots from the first bar to the last, an ink dot at each end */
    const top = barY(0), foot = barY(n - 1);
    if (foot - top > 1) {
      ctx.save();
      ctx.strokeStyle = T.muted; ctx.lineWidth = 1.3; ctx.lineCap = 'round'; ctx.setLineDash([0, 5.5]);
      ctx.beginPath(); ctx.moveTo(SPINE_X, foot); ctx.lineTo(SPINE_X, top); ctx.stroke();
      ctx.restore();
    }
    ctx.fillStyle = T.ink;
    ctx.beginPath();
    ctx.moveTo(SPINE_X + END_R, top); ctx.arc(SPINE_X, top, END_R, 0, TAU);
    if (foot !== top) { ctx.moveTo(SPINE_X + END_R, foot); ctx.arc(SPINE_X, foot, END_R, 0, TAU); }
    ctx.fill();

    const rings = [];
    for (let i = 0; i < n; i++) {
      const r = M.rows[i], y = barY(i);
      const a = hv !== null && hv !== i ? dimA : 1;
      const hx = r.value === null ? null : xOf(r.value);
      const pins = new Path2D(), bar = new Path2D();
      for (let x = SPINE_X + DOT; x <= R + 0.01; x += DOT) {
        if (x > rx + 3) break;
        const g = grown(x);
        if (g <= 0) continue;
        if (hx !== null && x < hx - DOT * 0.6) { const rr = BAR_R * g; bar.moveTo(x + rr, y); bar.arc(x, y, rr, 0, TAU); }
        else if (hx === null || x > hx + DOT * 0.6) { const rr = PIN_R * g; pins.moveTo(x + rr, y); pins.arc(x, y, rr, 0, TAU); }
      }
      ctx.fillStyle = kit.rgba(pin, pin[3] * a);
      ctx.fill(pins);
      ctx.fillStyle = kit.rgba(P, P[3] * 0.92 * a);
      ctx.fill(bar);
      if (hx !== null) {
        if (live && born[i] === undefined && rx >= hx) born[i] = clock;
        const g = growth(i);
        if (g > 0) {
          const rr = HEAD_R * kit.clamp(g * 3, 0, 1.2) + (hv === i ? 0.4 : 0);
          ctx.fillStyle = kit.rgba(P, P[3] * a);
          ctx.beginPath(); ctx.arc(hx, y, Math.max(0, rr), 0, TAU); ctx.fill();
          if (hv === i) rings.push([hx, y]);
        }
      }
      /* the label arrives with the chart, the value as its bar reaches it */
      const e = rowEls[i];
      if (e) {
        const la = labelA * a;
        const va = (hx === null ? labelA : (rx === Infinity ? 1 : kit.seg(rx - hx, 0, 30))) * a;
        if (Math.abs(la - e.a) > 0.004) { e.a = la; e.lab.style.opacity = la >= 0.999 ? '' : la.toFixed(3); }
        if (Math.abs(va - e.va) > 0.004) { e.va = va; e.val.style.opacity = va >= 0.999 ? '' : va.toFixed(3); }
      }
    }
    if (rings.length) {
      ctx.save();
      ctx.lineWidth = 1;
      ctx.strokeStyle = kit.rgba(P, Math.min(1, P[3] * 1.1) * 0.9);
      ctx.beginPath();
      for (const [x, y] of rings) { const rr = HEAD_R + 1.5 + 3.5 * ring.x; ctx.moveTo(x + rr, y); ctx.arc(x, y, rr, 0, TAU); }
      ctx.stroke();
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    if (!M.rows.length) { kit.setReadout(parts.readout, []); return; }
    if (hover === null) { kit.setReadout(parts.readout, [{ label: kit.columnLabel(M.valueCol || 'value') }]); return; }
    const r = M.rows[hover], v = valueText(r.value);
    /* The label in ink, then the value; a label too long for the line goes muted and is cut, so the value stays. */
    const room = parts.readout.clientWidth || width;
    const font = `500 ${READOUT_FONT}px ${tokens.sans}`;
    const fits = textWidth(r.label, `${READOUT_FONT}px ${tokens.sans}`) + 16 + textWidth(v, font) <= room - 2;
    kit.setReadout(parts.readout, fits ? [{ when: r.label }, { value: v }] : [{ label: r.label, value: v }]);
  }

  function setHover(i, from) {
    if (i === hover) { hoverFrom = from; return; }
    hover = i; hoverFrom = from;
    if (i !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs while a ring grows, rows dim, or bars arrive ---------- */
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
  function rowAt(py) {
    const n = M.rows.length;
    if (!n) return null;
    return kit.clamp(Math.floor((py - TOP + 2) / M.tier.pitch), 0, n - 1);
  }
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
  /* Up and down move through the rows, as left and right do. */
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const n = M.rows.length;
    if (!n) return;
    const i = hover;
    let j;
    switch (e.key) {
      case 'ArrowDown': case 'ArrowRight': j = i === null ? 0 : Math.min(n - 1, i + 1); break;
      case 'ArrowUp': case 'ArrowLeft': j = i === null ? n - 1 : Math.max(0, i - 1); break;
      case 'Home': j = 0; break;
      case 'End': j = n - 1; break;
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
  const stopWidth = kit.observeWidth(parts.plot, (w) => { width = w; readout(); draw(); });
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
