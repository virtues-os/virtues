// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/stack.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* stack.js: the stacked bar chart. One bar per period split into its parts, drawn in the dot grammar of the kept
   tiles.

   mark "stack". It keeps ShowChart's bar rules (apps/web/src/lib/components/chat/show/) and adds the parts:
   - The first column is the x axis and the next two to five columns are the parts, stacked in column order from the
     bottom. Rows arrive as show.rs shapes them: numbers, or null where the record has no value. 2 to 199 rows.
   - Bars start at zero and keep the order the query returned, one band each, as ShowChart lays out its bars.
   - A part is drawn where it and every part under it have a value, because its place in the bar depends on them.
     A part below zero is drawn as zero; show.rs refuses one before it gets here.
   - One accent, and ink told apart by weight and halftone: the first part is the primary, solid. The second is ink
     at 34%, solid. The third is a halftone of ink at 60%, the fourth a finer halftone of subtle ink, and the fifth a
     hairline outline. A 1px gap of the page separates two parts, and the grid never shows through a part.
   - The readout names the parts, or the bar you rest on: its period, its total and each part. In a narrow column
     the part names leave the line and stay for screen readers, and each value keeps its swatch.
   - Rest on a bar, or tab to the chart and use the arrow keys: the other bars dim and a hairline ring grows around
     the bar. A total of zero is a dot on the floor, so a quiet period still has its place.
   The rest of the grammar is series.js's: the y axis is a rule of fine dots with an ink dot at its top and its foot,
   and the grid is dotted. In the frame each bar grows from the floor as the frame's sweep passes.

   Contract: see the top of kit.js. Block: readout 26 + plot 200 + foot 26 = 252px. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'stack', kinds: ['chart'], marks: ['stack'] });

const HEIGHT = kit.PLOT.height;
const PAD = kit.PLOT.pad;
const LABEL_W = 52;          /* a short date label is about 44px of 9.5px mono; each one gets 52 */
const END_R = 1.75;          /* the ink dots at the top and the foot of the y axis */
const MAX_PARTS = 5;
const GAP = 1;               /* the page between two parts, in CSS px */
const GROW = 0.45, GROW_BOUNCE = 0.1;
const TAU = Math.PI * 2;

/* How each part is filled, from the bottom. `color` is a kit token and `alpha` scales it; a screen is a square dot
   grid. The second part is the lightest solid ink, because it is often the largest. */
const PARTS = Object.freeze([
  Object.freeze({ fill: 'solid', color: 's0' }),
  Object.freeze({ fill: 'solid', color: 'ink', alpha: 0.34 }),
  Object.freeze({ fill: 'screen', color: 's1', r: 0.85, pitch: 2.5 }),
  Object.freeze({ fill: 'screen', color: 's2', r: 0.7, pitch: 3 }),
  Object.freeze({ fill: 'outline', color: 's2' }),
]);

const CSS = '.vk-stack .vk-readout.vk-tight{gap:0 12px}';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const xKey = columns[0];
  const parts = columns.slice(1, 1 + MAX_PARTS);
  const xs = rows.map((r) => r[xKey]);
  const dates = kit.isDateColumn(xs) || kit.isTimeColumn(xs);
  /* each bar's running tops, part by part, up to the first part with no value */
  const stacks = rows.map((r) => {
    const tops = [];
    let acc = 0;
    for (const p of parts) {
      const v = r[p];
      if (!isNum(v)) break;
      acc += Math.max(0, v);
      tops.push(acc);
    }
    return tops;
  });
  const totals = stacks.map((t) => (t.length === parts.length && t.length ? t[t.length - 1] : null));
  let hi = 0;
  for (const t of stacks) if (t.length) hi = Math.max(hi, t[t.length - 1]);
  let ticks = kit.niceTicks(0, hi);
  if (ticks.length < 2) ticks = [0, hi || 1];
  return { columns, rows, xKey, parts, xs, dates, stacks, totals, ticks, yMin: ticks[0], yMax: ticks[ticks.length - 1] };
}

const listText = (names) => (names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}` : names[0] || '');

function ariaLabel(M) {
  let label = `Stacked bar chart of ${listText(M.parts.map(kit.columnLabel))} by ${kit.columnLabel(M.xKey || 'row')}`;
  if (M.rows.length > 1) {
    const first = kit.formatX(M.xs[0], M.dates), last = kit.formatX(M.xs[M.xs.length - 1], M.dates);
    if (first && last) label += `, from ${first} to ${last}`;
  }
  return `${label}. Each bar stacks its parts from the bottom in that order.`;
}

/* The readout's mark for part p, drawn the way the bar draws it. `sr` adds the part's name for screen readers when
   the line has no room to show it. */
function partSwatch(p, sr) {
  const st = PARTS[p] || PARTS[PARTS.length - 1];
  const c = st.alpha ? `color-mix(in srgb, var(--vk-${st.color}) ${Math.round(st.alpha * 100)}%, transparent)` : `var(--vk-${st.color})`;
  let body;
  if (st.fill === 'solid') body = `<rect x="0" y="0" width="8" height="10" rx="1" style="fill:${c}"/>`;
  else if (st.fill === 'outline') body = `<rect x=".5" y=".5" width="7" height="9" rx="1" style="fill:none;stroke:${c};stroke-width:1"/>`;
  else {
    const step = st.pitch === 3 ? 3 : 2.5, off = st.pitch === 3 ? 1 : 1.25;
    const dots = [];
    for (let y = off; y < 10; y += step) for (let x = off; x < 8; x += step) dots.push(`<circle cx="${x}" cy="${y}" r="${st.r}"/>`);
    body = `<g style="fill:${c}">${dots.join('')}</g>`;
  }
  return `<svg class="vk-sw" width="8" height="10" viewBox="0 0 8 10" aria-hidden="true">${body}</svg>${sr ? `<span class="vk-sr">${kit.esc(sr)}</span>` : ''}`;
}

/* Writes the first of `levels` that fits the readout's line; the last one is written whatever its width. The line's
   figures shrink before it overflows, so a level that does not fit shows as a name cut short. */
const fits = (node) => node.scrollWidth <= node.clientWidth + 1
  && [...node.querySelectorAll('.vk-lab')].every((e) => e.scrollWidth <= e.clientWidth + 1);
function fitReadout(node, levels) {
  if (!node) return;
  for (let l = 0; l < levels.length; l++) {
    node.classList.toggle('vk-tight', l > 0);
    kit.setReadout(node, levels[l]);
    if (!node.clientWidth || l === levels.length - 1 || fits(node)) return;
  }
}

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('stack', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-stack');
  el.appendChild(root);
  const blk = kit.block(root, {
    plotHeight: HEIGHT, readout: true, foot: opts.foot !== false,
    hint: 'Use the left and right arrow keys to read each bar.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  blk.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  /* The reveal: 'none' draws everything at once; 'live' grows each bar as the frame's sweep passes it. */
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  let screens = new Map(), screensKey = '';
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry, as ShowChart lays out bars ---------- */
  const B = () => box(width);
  const Y = (v) => {
    const b = B(), plotH = b.bottom - b.top;
    return b.top + plotH - ((v - M.yMin) / (M.yMax - M.yMin || 1)) * plotH;
  };
  const band = () => (B().right - B().left) / Math.max(1, M.rows.length);
  const xAt = (i) => B().left + band() * (i + 0.5);
  const barWidth = () => kit.clamp(band() * 0.62, 2, 28);

  function growth(i) {
    if (mode === 'none') return 1;
    if (born[i] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[i], GROW, GROW_BOUNCE));
  }

  /* A halftone as a canvas pattern, one dot per tile in device pixels, so it stays crisp at any density. */
  function screen(ctx, st, k) {
    const key = `${k}|${tokens.signature}`;
    if (key !== screensKey) { screens = new Map(); screensKey = key; }
    const id = `${st.color}|${st.r}|${st.pitch}`;
    let s = screens.get(id);
    if (!s) {
      const pd = Math.max(2, Math.round(st.pitch * k));
      const c = document.createElement('canvas');
      c.width = pd; c.height = pd;
      const g = c.getContext('2d');
      g.fillStyle = kit.rgba(tokens.rgb[st.color]);
      g.beginPath(); g.arc(pd / 2, pd / 2, Math.max(0.5, st.r * k), 0, TAU); g.fill();
      s = { pattern: ctx.createPattern(c, 'repeat'), pd };
      screens.set(id, s);
    }
    return s;
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (ctx, x, y, r) => { ctx.moveTo(x + r, y); ctx.arc(x, y, r, 0, TAU); };

  function draw() {
    if (!width || !M.rows.length) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const k = canvas.width / width;
    const snap = (v) => Math.round(v * k) / k;
    const T = tokens, b = B();
    const L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const n = M.rows.length;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;

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

    /* x labels, thinned to one per 52px and centered under their bars, each fading in once the dot has passed */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`;
    ctx.fillStyle = T.muted;
    ctx.textBaseline = 'alphabetic';
    ctx.textAlign = 'center';
    const labelled = kit.labelIndices(n, Math.max(2, Math.floor((R - L) / LABEL_W)));
    const room = Math.max(4, Math.floor(Math.max(LABEL_W, band()) / 6) - 1);
    const fit = (text) => (text.length > room ? `${text.slice(0, room - 1)}…` : text);
    for (const i of labelled) {
      const x = xAt(i);
      const a = labelA * (clipX < Infinity ? kit.seg(clipX - x, 0, 24) : 1);
      if (a <= 0) continue;
      ctx.globalAlpha = a;
      const text = M.dates ? kit.formatX(M.xs[i], true) : fit(kit.formatX(M.xs[i], false));
      /* the last label is held inside the canvas when its bar sits close to the right edge */
      const half = ctx.measureText(text).width / 2;
      ctx.fillText(text, Math.min(x, width - half - 1), HEIGHT - 6);
    }
    ctx.restore();

    /* the y axis: fine dots from the foot to the top with an ink dot at each end */
    dotted(ctx, L, floor, L, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath(); disc(ctx, L, top, END_R); disc(ctx, L, floor, END_R); ctx.fill();
    ctx.save();
    ctx.globalAlpha = labelA;
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    for (const t of M.ticks) ctx.fillText(kit.formatNumber(t), L - 6, Y(t));
    ctx.restore();

    /* the bars */
    const hv = hover;
    const dimA = hv !== null ? kit.lerp(1, 0.45, dim.x) : 1;
    const bw = barWidth();
    let ringAt = null;
    for (let i = 0; i < n; i++) {
      const tops = M.stacks[i];
      const g = growth(i);
      if (g <= 0 || !tops.length) continue;
      const cx = xAt(i);
      const x0 = snap(cx - bw / 2), x1 = Math.max(x0 + 1 / k, snap(cx + bw / 2));
      ctx.globalAlpha = hv !== null && hv !== i ? dimA : 1;
      const total = tops[tops.length - 1];
      if (total <= 0) {
        ctx.fillStyle = T.muted;
        ctx.beginPath(); disc(ctx, cx, floor, 1.2); ctx.fill();
        if (hv === i) ringAt = { x0: cx - 1.2, x1: cx + 1.2, y: floor - 1.2 };
        continue;
      }
      let below = 0, drawnBelow = false, barTop = floor;
      for (let p = 0; p < tops.length; p++) {
        const v0 = below, v1 = tops[p];
        below = v1;
        if (v1 <= v0) continue;
        const yt = snap(Y(v1 * g));
        const yb = snap(Y(v0 * g)) - (drawnBelow ? GAP : 0);
        if (yb - yt < 0.5) continue;
        drawnBelow = true;
        barTop = yt;
        const st = PARTS[p], base = T.rgb[st.color];
        const col = st.alpha ? [base[0], base[1], base[2], base[3] * st.alpha] : base;
        const last = p === tops.length - 1;
        const r = last ? Math.min(1.5, (x1 - x0) / 2, yb - yt) : 0;
        const path = new Path2D();
        if (r > 0.2 && typeof path.roundRect === 'function') path.roundRect(x0, yt, x1 - x0, yb - yt, [r, r, 0, 0]);
        else path.rect(x0, yt, x1 - x0, yb - yt);
        if (st.fill === 'solid') {
          ctx.fillStyle = kit.rgba(col);
          ctx.fill(path);
        } else if (st.fill === 'screen') {
          ctx.fillStyle = T.page;
          ctx.fill(path);
          const s = screen(ctx, st, k);
          /* a dot column on the bar's center and a dot row on the floor, so every bar carries the same screen */
          const tx = Math.round(cx * k - s.pd / 2), ty = Math.round(floor * k);
          s.pattern.setTransform(new DOMMatrix([1 / k, 0, 0, 1 / k, tx / k, ty / k]));
          ctx.fillStyle = s.pattern;
          ctx.fill(path);
        } else {
          ctx.fillStyle = T.page;
          ctx.fill(path);
          ctx.strokeStyle = kit.rgba(col);
          ctx.lineWidth = 1;
          ctx.strokeRect(x0 + 0.5, yt + 0.5, Math.max(0, x1 - x0 - 1), Math.max(0, yb - yt - 1));
        }
      }
      if (hv === i) ringAt = { x0, x1, y: barTop };
    }
    ctx.globalAlpha = 1;

    /* the ring: the bar's outline lifted off it, open at the floor, growing as you rest */
    if (ringAt) {
      const o = 1 + 2.5 * ring.x;
      const xa = ringAt.x0 - o, xb = ringAt.x1 + o, ya = ringAt.y - o, rr = Math.min(3, (xb - xa) / 2);
      ctx.save();
      ctx.strokeStyle = kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.6);
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(xa, floor - 0.5);
      ctx.lineTo(xa, ya + rr);
      ctx.arcTo(xa, ya, xa + rr, ya, rr);
      ctx.lineTo(xb - rr, ya);
      ctx.arcTo(xb, ya, xb, ya + rr, rr);
      ctx.lineTo(xb, floor - 0.5);
      ctx.stroke();
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    const node = blk.readout;
    if (!node || !M.parts.length) return;
    const names = M.parts.map(kit.columnLabel);
    if (hover === null) {
      fitReadout(node, [names.map((nm, p) => ({ swatch: partSwatch(p), label: nm }))]);
      return;
    }
    const row = M.rows[hover];
    const when = { when: kit.formatX(M.xs[hover], M.dates) };
    const total = M.totals[hover];
    const tv = total === null ? '-' : kit.formatNumber(total);
    const vals = M.parts.map((p) => (isNum(row[p]) ? kit.formatNumber(row[p]) : '-'));
    fitReadout(node, [
      [when, { label: 'Total', value: tv }, ...vals.map((v, p) => ({ swatch: partSwatch(p), label: names[p], value: v }))],
      [when, { label: 'Total', value: tv }, ...vals.map((v, p) => ({ swatch: partSwatch(p, names[p]), value: v }))],
      [when, { swatch: '<span class="vk-sr">Total</span>', value: tv }, ...vals.map((v, p) => ({ swatch: partSwatch(p, names[p]), value: v }))],
    ]);
  }

  function setHover(i, from) {
    if (i === hover) { hoverFrom = from; return; }
    hover = i; hoverFrom = from;
    if (i !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows, the others dim, or bars grow ---------- */
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
      if (clock - revealAt < 0.35) moving = true;
      for (let i = 0; i < born.length; i++) if (born[i] !== undefined && clock - born[i] < 1.2) { moving = true; break; }
      if (reveal < Infinity) moving = true;
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    for (let i = 0; i < M.rows.length; i++) {
      if (born[i] === undefined && (reveal === Infinity || xAt(i) <= reveal)) born[i] = clock;
    }
  }
  function rowsWait(on) {
    for (const node of [blk.readout, blk.foot]) {
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
    const i = Math.floor((px - B().left) / band());
    return kit.clamp(i, 0, M.rows.length - 1);
  }
  /* A mouse reads the bar under it and lets go when it leaves. A finger lifts as soon as it taps, so a tap keeps its
     bar until the same bar is tapped again or the chart loses focus. A drag across the chart scrubs. */
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
  blk.plot.addEventListener('pointermove', onMove);
  blk.plot.addEventListener('pointerdown', onDown);
  blk.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(blk.plot, {
    count: () => M.rows.length,
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
    kit.setLabel(blk.plot, ariaLabel(M));
    readout();
    kit.foot(blk.foot, out, opts);
    if (mode === 'live') { if (width) markBorn(); rowsWait(reveal < Infinity); }
    draw();
  }
  const stopWidth = kit.observeWidth(blk.plot, (w) => {
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
    plot: blk.plot,
    update(next, patch) {
      if (patch) opts = { ...opts, ...patch };
      out = next || {};
      apply();
    },
    setReveal,
    destroy() {
      loop.stop(); stopKeys(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      blk.plot.removeEventListener('pointermove', onMove);
      blk.plot.removeEventListener('pointerdown', onDown);
      blk.plot.removeEventListener('pointerleave', onLeave);
      root.remove();
    },
  };
}
