// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/share.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* share.js: the 100% bar. One bar split into the shares of a whole, with each part named under it.

   A new chart mark, `mark: "share"`, that does the job of a pie or a donut and reads more exactly, because shares
   are compared as lengths along one line rather than as angles.
   - Two columns: the first is the label, the second the value, a number. 2 to 8 rows, drawn left to right in the
     order the query returns them. Each value is drawn as its share of the rows' total; a null counts as 0.
   - The bar is a 45 degree dot screen. The first part is the primary, its dots nearly touching; every other part is
     ink in a screen whose dots shrink part by part, so the parts are told apart by how much ink they carry and never by a
     second hue. 2px of page separate them.
   - Under the bar each part is named in its own row: a piece of its screen, its share in whole percent and its
     label. The percents are rounded so they add up to 100; a part under half a percent reads "<1%".
   - A short spine with an ink dot at its top and its foot stands at the bar's left end. While the query runs the
     frame's dot runs the bar's lower edge; the bar fills as the frame's sweep passes and each part's name arrives
     with it.
   - Rest on a part or its row, or tab to the bar and use the arrow keys: the other parts dim, two ink dots joined by
     fine dots mark the part's span under the bar, and the readout names it with its value and its share in words,
     such as "a little over half". Home and End jump to the ends and Escape clears it.
   Height: the rows set it. Before the result arrives the block is drawn for five parts; a result with another count
   settles it once, as the rows land. Block: readout 26 + plot (42 + 26 a part + 4, 176 for five) + foot 26 = 228px.

   Contract: see the top of kit.js. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'share', kinds: ['chart'], marks: ['share'] });

const MAX_PARTS = 8;
const NOMINAL = 5;             /* the parts a share bar is drawn for before its result says how many */
const BAR_TOP = 8, BAR_H = 20;
const SPINE_X = 2;             /* the spine at the bar's left end */
const BAR_L = 8;               /* the bar starts here */
const RIGHT = 2;
const GAP = 2;                 /* page between two parts */
const LEG_TOP = BAR_TOP + BAR_H + 14;
const ROW = 26, ROW_LINE = 20;
const SW_W = 16, SW_H = 10;    /* a row's piece of its part's screen */
const PCT_W = 40;              /* the share column, right-aligned */
const BOTTOM = 4;
const END_R = 1.75;
const S = 2.4;                 /* the screen's grid: dots on a checkerboard of 2.4px, a 45 degree screen of 3.4px */
/* Each part's dots: the first is the primary, nearly touching. The rest are ink whose dots shrink part by part
   while their ink grows stronger, so the screen thins but even the eighth part still shows on a dark page. */
const RADII = Object.freeze([1.56, 1.22, 1.0, 0.84, 0.72, 0.62, 0.54, 0.47]);
const INK = Object.freeze([1, 0.62, 0.7, 0.78, 0.86, 0.92, 1, 1]);
const TAU = Math.PI * 2;
const READOUT_FONT = 13;

const partsIn = (input) => (input && Array.isArray(input.rows) && input.rows.length ? kit.clamp(input.rows.length, 1, MAX_PARTS) : NOMINAL);
const plotHeight = (n) => LEG_TOP + n * ROW + BOTTOM;

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + plotHeight(partsIn(input)) + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width, input) {
  return { left: SPINE_X, right: Math.max(SPINE_X + 1, width - RIGHT), top: BAR_TOP, bottom: BAR_TOP + BAR_H, width, height: plotHeight(partsIn(input)) };
}

const CSS = `
.vk-share .vk-share-rows { position: absolute; inset: 0; pointer-events: none; }
.vk-share .vk-share-row {
  position: absolute; left: ${BAR_L + SW_W + 8}px; right: ${RIGHT}px; height: ${ROW_LINE}px; line-height: ${ROW_LINE}px;
  display: flex; align-items: baseline; gap: 12px;
  font-family: var(--vk-sans); font-size: 15px; color: var(--vk-ink); white-space: nowrap;
}
.vk-share .vk-share-pct { flex: none; width: ${PCT_W}px; text-align: right; font-variant-numeric: lining-nums tabular-nums; }
.vk-share .vk-share-lab { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
`;

/* ---------- the model ---------- */

/* Whole percents that add up to 100: each share's floor, then the largest remainders get the leftover points. */
function percents(shares) {
  const raw = shares.map((s) => s * 100);
  const out = raw.map(Math.floor);
  let left = 100 - out.reduce((a, b) => a + b, 0);
  const order = raw.map((v, i) => i).sort((a, b) => (raw[b] - out[b]) - (raw[a] - out[a]));
  for (let k = 0; k < order.length && left > 0; k++, left--) out[order[k]] += 1;
  return out;
}

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const all = Array.isArray(out && out.rows) ? out.rows : [];
  const labelCol = columns[0], valueCol = columns[1];
  const parts = all.slice(0, MAX_PARTS).map((r) => {
    const raw = r ? r[valueCol] : null;
    const v = typeof raw === 'number' && Number.isFinite(raw) && raw > 0 ? raw : 0;
    const name = r ? kit.formatCell(r[labelCol]) : '';
    return { label: name || '-', value: v, raw: typeof raw === 'number' && Number.isFinite(raw) ? raw : null };
  });
  const total = parts.reduce((a, p) => a + p.value, 0);
  const shares = parts.map((p) => (total > 0 ? p.value / total : 0));
  const pct = total > 0 ? percents(shares) : parts.map(() => 0);
  parts.forEach((p, i) => { p.share = shares[i]; p.pct = pct[i]; p.pctText = p.value > 0 && pct[i] === 0 ? '<1%' : `${pct[i]}%`; });
  return { columns, parts, labelCol, valueCol, total };
}

/* A share in words, for the readout: the nearest plain fraction a person would say, never a judgment. */
const FRACTIONS = Object.freeze([
  [0.95, 'nearly all of it'], [0.8, 'about four fifths'], [0.75, 'about three quarters'], [2 / 3, 'about two thirds'],
  [0.6, 'a little over half'], [0.5, 'about half'], [0.45, 'a little under half'], [0.4, 'about two fifths'],
  [1 / 3, 'about a third'], [0.25, 'about a quarter'], [0.2, 'about a fifth'], [1 / 6, 'about a sixth'],
  [0.125, 'about an eighth'], [0.1, 'about a tenth'], [0.05, 'about one in twenty'],
]);
function shareWords(s) {
  if (!(s > 0)) return 'none of it';
  if (s >= 0.995) return 'all of it';
  if (s < 0.035) return 'under one in twenty';
  let best = FRACTIONS[0], err = Infinity;
  for (const f of FRACTIONS) { const e = Math.abs(s - f[0]) / f[0]; if (e < err) { err = e; best = f; } }
  return best[1];
}

function ariaLabel(M) {
  if (!M.parts.length) return '100% bar with no rows';
  if (!(M.total > 0)) return `100% bar of ${kit.columnLabel(M.valueCol || 'value')}, with nothing to divide`;
  const what = kit.columnLabel(M.valueCol || 'value'), by = kit.columnLabel(M.labelCol || 'label').toLowerCase();
  return `100% bar of ${what} by ${by}: ${M.parts.map((p) => `${p.label}, ${p.pctText}`).join('; ')}.`;
}

let measureCtx = null;
function textWidth(text, font) {
  if (!measureCtx) measureCtx = document.createElement('canvas').getContext('2d');
  measureCtx.font = font;
  return measureCtx.measureText(text).width;
}

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('share', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-share');
  el.appendChild(root);
  let out = output || {};
  let M = model(out);
  const parts = kit.block(root, {
    plotHeight: plotHeight(partsIn(out)), readout: true, foot: opts.foot !== false,
    hint: 'Use the arrow keys to read each part.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  const layer = kit.h('div', 'vk-share-rows', { 'aria-hidden': 'true' });
  parts.plot.append(canvas, layer);

  let width = 0;
  let tokens = kit.readTokens(root);
  let rowEls = [];
  let hover = null, hoverFrom = null;
  const mark = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let clock = 0, revealAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry: each part's span on the bar ---------- */
  const H = () => plotHeight(Math.max(1, M.parts.length));
  const rowTop = (i) => LEG_TOP + i * ROW;
  function spans() {
    const R = Math.max(BAR_L + 1, width - RIGHT);
    const live = M.parts.filter((p) => p.value > 0).length;
    const room = Math.max(1, R - BAR_L - GAP * Math.max(0, live - 1));
    const out = [];
    let x = BAR_L;
    for (const p of M.parts) {
      if (!(p.value > 0) || !(M.total > 0)) { out.push(null); continue; }
      /* a part is at least 2px wide, so the smallest still shows */
      const w = Math.max(2, room * p.share);
      out.push([x, Math.min(R, x + w)]);
      x += w + GAP;
    }
    return out;
  }

  function buildRows() {
    layer.textContent = '';
    rowEls = M.parts.map((p, i) => {
      const row = kit.h('div', 'vk-share-row');
      row.style.top = `${rowTop(i)}px`;
      const pct = kit.h('span', 'vk-share-pct');
      pct.textContent = p.pctText;
      const lab = kit.h('span', 'vk-share-lab');
      lab.textContent = p.label;
      row.append(pct, lab);
      layer.appendChild(row);
      return { row, a: -1 };
    });
  }

  /* ---------- drawing ---------- */
  const partColor = (i) => {
    const c = i === 0 ? tokens.rgb.s0 : tokens.rgb.ink;
    return [c[0], c[1], c[2], c[3] * INK[Math.min(i, INK.length - 1)]];
  };
  /* The screen over [x0, x1] x [y0, y1], anchored to the plot so neighbors share one grid. g scales the dots as
     they arrive. */
  function screen(ctx, x0, x1, y0, y1, r, g) {
    const path = new Path2D();
    const k0 = Math.floor(y0 / S), k1 = Math.ceil(y1 / S);
    for (let k = k0; k <= k1; k++) {
      const y = (k + 0.5) * S;
      if (y < y0 - r || y > y1 + r) continue;
      const off = (k & 1) ? S : 0;
      const j0 = Math.floor((x0 - off) / (2 * S)) - 1, j1 = Math.ceil((x1 - off) / (2 * S)) + 1;
      for (let j = j0; j <= j1; j++) {
        const x = off + j * 2 * S;
        if (x < x0 - r || x > x1 + r) continue;
        const rr = r * (typeof g === 'function' ? g(x) : 1);
        if (rr <= 0.05) continue;
        path.moveTo(x + rr, y); path.arc(x, y, rr, 0, TAU);
      }
    }
    return path;
  }

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, H());
    const T = tokens, n = M.parts.length;
    if (!n) return;
    const live = mode === 'live';
    const rx = live ? reveal : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const sp = spans();
    const hv = hover;
    const dimA = hv !== null ? kit.lerp(1, 0.32, dim.x) : 1;
    const grown = rx === Infinity ? null : (x) => kit.ease.out(kit.seg(rx - x, 0, 22));
    const top = BAR_TOP, bot = BAR_TOP + BAR_H;

    /* the spine at the bar's left end, an ink dot at each end */
    ctx.save();
    ctx.strokeStyle = T.muted; ctx.lineWidth = 1.3; ctx.lineCap = 'round'; ctx.setLineDash([0, 5]);
    ctx.beginPath(); ctx.moveTo(SPINE_X, bot); ctx.lineTo(SPINE_X, top); ctx.stroke();
    ctx.restore();
    ctx.fillStyle = T.ink;
    ctx.beginPath();
    ctx.moveTo(SPINE_X + END_R, top); ctx.arc(SPINE_X, top, END_R, 0, TAU);
    ctx.moveTo(SPINE_X + END_R, bot); ctx.arc(SPINE_X, bot, END_R, 0, TAU);
    ctx.fill();

    for (let i = 0; i < n; i++) {
      const s = sp[i];
      const col = partColor(i);
      const a = (hv !== null && hv !== i ? dimA : 1) * col[3];
      const r = RADII[Math.min(i, RADII.length - 1)];
      if (s && s[0] <= rx + 3) {
        const x1 = Math.min(s[1], rx + 3);
        ctx.save();
        ctx.beginPath(); ctx.rect(s[0], top, x1 - s[0], BAR_H); ctx.clip();
        ctx.fillStyle = kit.rgba(col, a);
        ctx.fill(screen(ctx, s[0], x1, top, bot, r, grown));
        ctx.restore();
      }
      /* the row under the bar: its piece of the screen, then its share and label, arriving with its part */
      const rowA = (s ? (rx === Infinity ? 1 : kit.seg(rx - s[0], 0, 24)) : labelA) * labelA;
      const ry = rowTop(i) + (ROW_LINE - SW_H) / 2 + 1;
      if (rowA > 0) {
        ctx.save();
        ctx.beginPath(); ctx.rect(BAR_L, ry, SW_W, SW_H); ctx.clip();
        ctx.fillStyle = kit.rgba(col, a * rowA);
        ctx.fill(screen(ctx, BAR_L, BAR_L + SW_W, ry, ry + SW_H, r, null));
        ctx.restore();
      }
      const e = rowEls[i];
      if (e) {
        const ta = rowA * (hv !== null && hv !== i ? dimA : 1);
        if (Math.abs(ta - e.a) > 0.004) { e.a = ta; e.row.style.opacity = ta >= 0.999 ? '' : ta.toFixed(3); }
      }
    }

    /* the part you rest on: its span marked under the bar by two ink dots joined by fine dots */
    if (hv !== null && sp[hv]) {
      const [x0, x1] = sp[hv];
      const y = bot + 6, k = mark.x;
      const cx = (x0 + x1) / 2, h0 = cx + (x0 - cx) * k, h1 = cx + (x1 - cx) * k;
      ctx.save();
      ctx.globalAlpha = kit.clamp(k * 1.4, 0, 1);
      if (h1 - h0 > 6) {
        ctx.strokeStyle = T.muted; ctx.lineWidth = 1.2; ctx.lineCap = 'round'; ctx.setLineDash([0, 4]);
        ctx.beginPath(); ctx.moveTo(h0, y); ctx.lineTo(h1, y); ctx.stroke();
        ctx.setLineDash([]);
      }
      ctx.fillStyle = T.ink;
      ctx.beginPath();
      ctx.moveTo(h0 + 1.6, y); ctx.arc(h0, y, 1.6, 0, TAU);
      if (h1 - h0 > 3.2) { ctx.moveTo(h1 + 1.6, y); ctx.arc(h1, y, 1.6, 0, TAU); }
      ctx.fill();
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    if (!M.parts.length) { kit.setReadout(parts.readout, []); return; }
    const what = kit.columnLabel(M.valueCol || 'value');
    if (hover === null) { kit.setReadout(parts.readout, [{ label: what }]); return; }
    const p = M.parts[hover];
    const v = p.raw === null ? '-' : kit.formatNumber(p.raw);
    const words = shareWords(p.share);
    const room = (parts.readout.clientWidth || width) - 2;
    const f = `${READOUT_FONT}px ${tokens.sans}`, fb = `500 ${READOUT_FONT}px ${tokens.sans}`;
    const w = (xs) => xs.reduce((a, [t, font]) => a + textWidth(t, font), 0);
    const forms = [
      { parts: [{ when: p.label }, { label: what, value: v }, { label: words }], w: w([[p.label, f], [what, f], [v, fb], [words, f]]) + 16 * 2 + 6 },
      { parts: [{ when: p.label }, { value: v }, { label: words }], w: w([[p.label, f], [v, fb], [words, f]]) + 16 * 2 },
      { parts: [{ when: p.label }, { label: words }], w: w([[p.label, f], [words, f]]) + 16 },
    ];
    const pick = forms.find((x) => x.w <= room) || { parts: [{ label: p.label, value: p.pctText }] };
    kit.setReadout(parts.readout, pick.parts);
  }

  function setHover(i, from) {
    if (i === hover) { hoverFrom = from; return; }
    hover = i; hoverFrom = from;
    if (i !== null) { mark.x = reduced() ? 1 : 0; mark.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs while a part's mark opens, parts dim, or the bar arrives ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    const target = hover !== null ? 1 : 0;
    if (reduced()) { mark.x = 1; mark.v = 0; dim.x = target; dim.v = 0; }
    else {
      kit.springTo(mark, 1, dt, 0.3, 0.1);
      kit.springTo(dim, target, dt, 0.2, 0);
      moving = mark.x !== 1 || dim.x !== target;
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
    reveal = reduced() ? Infinity : x;
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  function pickAt(px, py) {
    const n = M.parts.length;
    if (!n) return null;
    if (py >= LEG_TOP - 6) return kit.clamp(Math.floor((py - LEG_TOP + 3) / ROW), 0, n - 1);
    const sp = spans();
    let best = null, dist = Infinity;
    sp.forEach((s, i) => {
      if (!s) return;
      const d = px < s[0] ? s[0] - px : px > s[1] ? px - s[1] : 0;
      if (d < dist) { dist = d; best = i; }
    });
    return best;
  }
  const pick = (e) => { const r = canvas.getBoundingClientRect(); return pickAt(e.clientX - r.left, e.clientY - r.top); };
  const onMove = (e) => {
    if (!width || !M.parts.length) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!width || !M.parts.length) return;
    const i = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === i) { setHover(null, null); return; }
    setHover(i, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const n = M.parts.length;
    if (!n) return;
    const i = hover;
    let j;
    switch (e.key) {
      case 'ArrowRight': case 'ArrowDown': j = i === null ? 0 : Math.min(n - 1, i + 1); break;
      case 'ArrowLeft': case 'ArrowUp': j = i === null ? n - 1 : Math.max(0, i - 1); break;
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
