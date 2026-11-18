// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/kit.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* kit.js: the shared pieces of the inline chat chart components.

   Every component in this folder takes the real output of the app's `show` tool and draws it inside a chat reply.
   They are plain ES modules with no dependencies, written so a developer can move them into
   apps/web/src/lib/components/chat/show/ without redesigning them. This file is the one thing they share.

   ============================================================================================================
   THE COMPONENT CONTRACT  (every charts/<name>.js)
   ============================================================================================================

     import * as kit from './kit.js';                 the only import a component makes

     export const meta = { name, kinds, marks }       what it draws, e.g. { name: 'series', kinds: ['chart'], marks: ['bar', 'line'] }
     export function blockHeight(input)  -> px        the height of its block before any data has arrived. `input` is
                                                       the call's input so far ({ kind, mark, title }) or the result.
     export function box(width, input)   -> rect      its plot rect inside the plot element, also before any data:
                                                       { left, right, top, bottom, width, height } in CSS px. The frame
                                                       runs its dot along `bottom` and sweeps the reveal from `left`.
     export function mount(el, output, options) -> handle

       el       the element to draw into. mount appends one root (class "vk ...") and never touches el's other children.
       output   exactly a `show` result, as virtues-core/src/tools/show.rs shapes it:
                  { kind, title, mark, columns, rows, row_count, note, ref }
                plus any field a brief proposes. Rows are objects keyed by column name. Numbers are numbers, nulls
                are gaps, times are strings (RFC 3339, `YYYY-MM-DD HH:MM:SS` or `YYYY-MM-DD`).
       options  shared by every component; each one ignores what it does not use:
                  onOpenRows(ref)   "See the rows" calls this with output.ref. No button without both.
                  reduced           true or false overrides prefers-reduced-motion.
                  foot              false leaves out the source line and "See the rows" (default true).
                  reveal            a number in plot px: start hidden right of it (the frame uses this).
                  sentence          the text of the reply's sentence before the figure, a string.
                  reply             the element that holds that sentence, for a component that works in the
                                    prose itself (the number component). Read it; never restyle the page through it.

       handle   { update(output, patch), destroy(), plot, setReveal?(x) }
                  update(output, patch)  draw a new result; `patch` is merged into options.
                  destroy()              removes the root and stops every listener, observer and frame.
                  plot                   the role="img" element, which is also the keyboard focus target.
                  setReveal(x)           optional. Show marks left of x (plot px) and grow them as x passes them.
                                         Infinity shows everything. The frame calls it on the chart's arrival;
                                         a component without it is revealed with a clip-path instead.

   The block every chart component builds (kit.block) is three rows, at fixed heights so nothing below ever moves:
     readout   ROWS.readout = 26px   (20px line + 6px gap): the legend, or the point you rest on. aria-live.
     plot      PLOT.height  = 200px  role="img", aria-label, tabindex 0, arrow keys move the readout.
     foot      ROWS.foot    = 26px   (8px gap + 18px line): "From your records" and "See the rows".
   The frame adds a title row, ROWS.title = 32px, when the call has a title.

   ============================================================================================================
   THE KIT  (everything exported below)
   ============================================================================================================

   Formatting, ported from apps/web/src/lib/components/chat/show/show.ts with the same behavior:
     isShowOutput(o)                     whether o is a show result this kit can draw
     niceTicks(min, max, count = 4)      up to `count` round ticks covering [min, max]
     formatNumber(n, { compactAbove })   7.25, 1,204, 3.4M (grouping from five digits, so 2026 reads 2026)
     formatCell(v)                       a cell as text: numbers formatted, times on the reader's clock
     parseTime(v)                        a Date or null; a bare date is local midnight, never UTC
     isTimeColumn(values)                every non-null value is a time
     isDateColumn(values)                every non-null value is a bare date
     formatTime(t, { withDate, withTime })
     formatX(v, dates)                   an x value as an axis label
     labelIndices(n, max)                at most `max` evenly spread label indices, first and last included
     assignLanes(spans, gap)             a lane per span so none overlaps another in its lane
     isNumericColumn(rows, column), hasValues(rows, column), isTimeValues(rows, column)
     columnLabel(column)                 an alias stands as written; `calendar_name` becomes "Calendar name"
     rowRef(row)                         a row's own record ref, or null

   Motion:
     clamp(x, a, b), lerp(a, b, t), seg(t, a, b)    seg is the 0..1 progress of t through [a, b]
     bezier(x1, y1, x2, y2) -> (t) => y              a CSS cubic-bezier as a function
     ease.out, ease.inOut                            cubic-bezier(.2,0,0,1) and (.45,0,.55,1), the labs' curves
     stepSpring(t, duration, bounce = 0)             closed-form spring step response: 0 at t = 0, settles at 1
     springTo(state, target, dt, duration, bounce)   integrates state = { x, v } toward target; returns state.x
     prefersReducedMotion()                          the media query, read now
     watchReducedMotion(cb) -> stop                  cb(reduced) when it changes
     loop(step) -> { wake(), stop(), running }       a frame loop that sleeps when idle: step(dt, now) runs once a
                                                     frame and returns true while something still moves. When it
                                                     returns false no frame is requested until wake(). dt is in
                                                     seconds and capped at 1/20 s, so a hidden tab never jumps.

   Tokens (the app's CSS variables, with the lab values as fallbacks):
     readTokens(el)        resolved colors and fonts for el's theme:
                             { primary, ink, muted, subtle, border, borderSubtle, borderStrong, background, page,
                               highlight, focus, s0, s1, s2,  sans, serif, mono,  rgb: { ...the colors as [r,g,b,a] },
                               signature }
                           Colors are 'rgba(...)' strings ready for a canvas. s0 is the first series (the primary),
                           s1 the second (ink at 60%), s2 the third (subtle ink). Compare `signature` to skip a
                           redraw when nothing changed.
     TOKENS                the table behind it: key -> [kit variable, app variable, lab fallback]
     parseColor(str)       '#1D4572' | 'rgb(..)' | 'rgba(..)' | 'color(srgb ..)' -> [r, g, b, a] or null
     rgba(c, alpha)        [r, g, b, a] -> 'rgba(...)', optionally with a new alpha
     mix(a, b, t)          two [r, g, b, a] colors mixed in sRGB
     watchTheme(el, cb) -> stop   cb() when a theme may have changed: class, style or data-theme on el's ancestors,
                                  or prefers-color-scheme. Re-read tokens there and redraw if the signature moved.

   DOM, canvas and size:
     h(tag, className, attrs), svgEl(tag, attrs, parent), esc(s), uid(prefix)
     dpr()                                    devicePixelRatio, at least 1 and at most 3
     fitCanvas(canvas, width, height) -> ctx  sizes the backing store to width x height x dpr, sets the transform so
                                              you draw in CSS px, clears it. Call at the top of every draw.
     watchDpr(cb) -> stop                     cb(dpr) when the window moves to a screen with another density
     observeWidth(el, cb) -> stop             cb(width) once el has a width, then on every change (whole px)
     ensureStyles(doc)                        injects kit.css when this module was copied as one file (the build
                                              fills KIT_CSS in for those copies); does nothing when kit.css is loaded
     ensureCss(id, css, doc)                  injects a component's own CSS once per document, as
                                              <style data-vk-css="id">. A component keeps its styles in its own module
                                              this way, scoped under its root class, so it needs no edit to kit.css
                                              and its one-file copy carries them too. Use the --vk-* variables.

   The block, its readout and its foot:
     PLOT = { height: 200, pad: { top: 8, right: 8, bottom: 22, left: 40 } }   the app's ShowChart geometry
     ROWS = { title: 32, readout: 26, foot: 26 }
     plotBox(width, height, pad) -> { left, right, top, bottom, width, height }
     block(el, { plotHeight, label, hint, readout, foot }) -> { root, readout, plot, foot, hint }
     setLabel(plot, text)                     sets the figure's aria-label
     setReadout(node, parts)                  parts: [{ when }, { swatch, label, value }, ...]; rewrites the readout
                                              only when its content changed, so aria-live speaks only real changes
     SERIES_STYLE                             the three series' weights and dashes: [{ line, dash, head, hollow, pin }]
     swatch(series, mark) -> svg string       the readout's mark for series 0, 1 or 2, in its own color
     foot(node, output, options)              writes "From your records" and, with output.ref and
                                              options.onOpenRows, a "See the rows" button that calls onOpenRows(ref)
     keyNav(target, { count, get, set, page }) -> stop
                                              makes target focusable and moves an index with Left and Right, Home,
                                              End, Page Up and Page Down; Escape and blur clear it.
                                              set(index | null, 'key' | 'blur')

   Stability: names and shapes above are the contract the components are written against. Add to it; do not rename
   or change what an existing name returns.

   Additions since the first version, all additive:
     2026-11-16  ensureCss(id, css, doc); the `reply` option; `sentence` written down as a string.

   ============================================================================================================
   ADDING A COMPONENT TO THE PAGE  (charts/build.sh writes ../dot-lab-charts.html)
   ============================================================================================================
   Write only your own files: charts/<slug>.js, charts/demo/<slug>.json and .sql, and charts/entries/<slug>.json.
   The entry is shaped like a built entry in components.json (frame and series are examples) and replaces the
   planned entry with the same slug. A demo is shown in a chat reply column. By default it is mounted the way
   Show.svelte would hold it, in the frame with your module as options.chart and the result already arrived. Optional
   demo fields: "replay": true adds "Run the call again" (running, then arrived after "query_ms"); "mount": "direct"
   mounts your module straight into the column with options.sentence and options.reply, for a component that works
   in the prose. An entry or module that fails its
   checks is left out with a warning, and the planned entry stays on the page. */

/* ---------- formatting: ports of show.ts ---------- */

const KINDS = ['chart', 'numbers', 'table', 'map', 'timeline', 'choices'];

export function isShowOutput(o) {
  const kind = o && o.kind;
  return typeof kind === 'string' && KINDS.includes(kind);
}

export function niceTicks(min, max, count = 4) {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return [];
  if (min === max) {
    if (min === 0) return [0, 1];
    [min, max] = min > 0 ? [0, min] : [min, 0];
  }
  const raw = (max - min) / count;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
  const start = Math.floor(min / step) * step;
  const ticks = [];
  for (let v = start; v <= max + step * 1e-9; v += step) ticks.push(roundTo(v, step));
  if (ticks[ticks.length - 1] < max) ticks.push(roundTo(start + ticks.length * step, step));
  return ticks;
}

function roundTo(v, step) {
  const digits = Math.max(0, -Math.floor(Math.log10(step)) + 1);
  return Number(v.toFixed(digits));
}

/* Grouping from five digits, so a year or a count like 2026 reads 2026, and 12,400 still groups. An engine that
   does not know "min2" reads it as true and groups from four digits. */
function numberFormat(opts) {
  try { return new Intl.NumberFormat(undefined, opts); } catch (err) { return new Intl.NumberFormat(undefined, { ...opts, useGrouping: true }); }
}
const NF_PLAIN = numberFormat({ maximumFractionDigits: 2, useGrouping: 'min2' });
const NF_WHOLE = numberFormat({ maximumFractionDigits: 0, useGrouping: 'min2' });
const NF_COMPACT = numberFormat({ notation: 'compact', maximumFractionDigits: 1 });

export function formatNumber(n, { compactAbove = 1e6 } = {}) {
  if (Math.abs(n) >= compactAbove) return NF_COMPACT.format(n);
  if (Math.abs(n) >= 100) return NF_WHOLE.format(n);
  return NF_PLAIN.format(n);
}

/* RFC 3339 (`timestamptz`) or the box's naive `YYYY-MM-DD HH:MM:SS`. */
const TIMESTAMP = /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/;
const DATE = /^\d{4}-\d{2}-\d{2}$/;

export function formatCell(v) {
  if (v === null || v === undefined) return '';
  if (typeof v === 'number') return formatNumber(v, { compactAbove: Infinity });
  if (typeof v === 'boolean') return v ? 'Yes' : 'No';
  if (typeof v === 'string') {
    if (DATE.test(v)) return formatTime(parseTime(v), { withDate: true, withTime: false });
    const t = TIMESTAMP.test(v) ? parseTime(v) : null;
    if (t) return formatTime(t, { withDate: true });
    return v;
  }
  return JSON.stringify(v);
}

/* A bare date is midnight in the reader's zone: `new Date("2026-09-08")` is UTC and lands on the 7th anywhere west
   of Greenwich. A naive timestamp is read the same way. */
export function parseTime(v) {
  if (typeof v !== 'string') return null;
  if (DATE.test(v)) {
    const [y, m, d] = v.split('-').map(Number);
    return new Date(y, m - 1, d);
  }
  if (!TIMESTAMP.test(v)) return null;
  const hasZone = /(Z|[+-]\d{2}:?\d{2})$/.test(v);
  const t = new Date(hasZone ? v : v.replace(' ', 'T'));
  return Number.isNaN(t.getTime()) ? null : t;
}

export function isTimeColumn(values) {
  const present = values.filter((v) => v !== null && v !== undefined);
  return present.length > 0 && present.every((v) => parseTime(v) !== null);
}

export function isDateColumn(values) {
  const present = values.filter((v) => v !== null && v !== undefined);
  return present.length > 0 && present.every((v) => typeof v === 'string' && DATE.test(v));
}

export function formatTime(t, { withDate = false, withTime = true } = {}) {
  const date = t.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  const midnight = t.getHours() === 0 && t.getMinutes() === 0;
  const time = t.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
  if (!withTime || (withDate && midnight)) return date;
  return withDate ? `${date}, ${time}` : time;
}

export function formatX(v, dates) {
  if (dates) {
    const t = parseTime(v);
    if (t) return formatTime(t, { withDate: true, withTime: false });
  }
  return formatCell(v);
}

export function labelIndices(n, max) {
  if (n <= max) return Array.from({ length: n }, (_, i) => i);
  const step = (n - 1) / (max - 1);
  return Array.from({ length: max }, (_, i) => Math.round(i * step));
}

export function assignLanes(spans, gap = 0) {
  const laneEnds = [];
  const order = spans.map((s, i) => i).sort((a, b) => spans[a].start - spans[b].start);
  const lanes = new Array(spans.length);
  for (const i of order) {
    let lane = laneEnds.findIndex((end) => end + gap <= spans[i].start);
    if (lane === -1) lane = laneEnds.push(spans[i].end) - 1;
    else laneEnds[lane] = spans[i].end;
    lanes[i] = lane;
  }
  return lanes;
}

export function isNumericColumn(rows, column) {
  const present = rows.map((r) => r[column]).filter((v) => v !== null && v !== undefined);
  return present.length > 0 && present.every((v) => typeof v === 'number');
}

export function columnLabel(column) {
  if (!/^[a-z0-9_]+$/.test(column)) return column;
  const words = column.replace(/_+/g, ' ').trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export function hasValues(rows, column) {
  return rows.some((r) => r[column] !== null && r[column] !== undefined && r[column] !== '');
}

export function isTimeValues(rows, column) {
  return isTimeColumn(rows.map((r) => r[column]));
}

export function rowRef(row) {
  const ref = row && row.ref;
  return typeof ref === 'string' && ref.startsWith('/') ? ref : null;
}

/* ---------- motion ---------- */

export const clamp = (x, a, b) => Math.max(a, Math.min(b, x));
export const lerp = (a, b, t) => a + (b - a) * t;
export const seg = (t, a, b) => clamp((t - a) / (b - a), 0, 1);

export function bezier(x1, y1, x2, y2) {
  const bx = (t) => 3 * x1 * t * (1 - t) * (1 - t) + 3 * x2 * t * t * (1 - t) + t * t * t;
  const by = (t) => 3 * y1 * t * (1 - t) * (1 - t) + 3 * y2 * t * t * (1 - t) + t * t * t;
  return (x) => {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    let lo = 0, hi = 1;
    for (let i = 0; i < 22; i++) { const m = (lo + hi) / 2; if (bx(m) < x) lo = m; else hi = m; }
    return by((lo + hi) / 2);
  };
}

export const ease = Object.freeze({ out: bezier(0.2, 0, 0, 1), inOut: bezier(0.45, 0, 0.55, 1) });

const springK = (d) => (2 * Math.PI / d) ** 2;
const springC = (d, b) => 4 * Math.PI * (1 - b) / d;

export function stepSpring(t, duration, bounce = 0) {
  if (t <= 0) return 0;
  const k = springK(duration), c = springC(duration, bounce);
  const w0 = Math.sqrt(k), z = c / (2 * w0);
  if (z < 1) {
    const wd = w0 * Math.sqrt(1 - z * z);
    return 1 - Math.exp(-z * w0 * t) * (Math.cos(wd * t) + (z * w0 / wd) * Math.sin(wd * t));
  }
  return 1 - Math.exp(-w0 * t) * (1 + w0 * t);
}

export function springTo(state, target, dt, duration, bounce = 0) {
  const k = springK(duration), c = springC(duration, bounce);
  let r = dt;
  while (r > 1e-6) {
    const step = Math.min(r, 1 / 240);
    state.v += (-k * (state.x - target) - c * state.v) * step;
    state.x += state.v * step;
    r -= step;
  }
  if (Math.abs(state.x - target) < 1e-4 && Math.abs(state.v) < 1e-3) { state.x = target; state.v = 0; }
  return state.x;
}

const reduceQuery = typeof matchMedia === 'function' ? matchMedia('(prefers-reduced-motion: reduce)') : null;

export function prefersReducedMotion() {
  return !!(reduceQuery && reduceQuery.matches);
}

export function watchReducedMotion(cb) {
  if (!reduceQuery) return () => {};
  const on = () => cb(reduceQuery.matches);
  reduceQuery.addEventListener('change', on);
  return () => reduceQuery.removeEventListener('change', on);
}

export function loop(step) {
  let raf = 0, last = 0, stopped = false;
  const frame = (now) => {
    raf = 0;
    const dt = last ? Math.min(0.05, Math.max(0, (now - last) / 1000)) : 1 / 60;
    last = now;
    let more = false;
    try { more = step(dt, now) === true; } catch (err) { console.error(err); more = false; }
    if (more && !stopped) raf = requestAnimationFrame(frame);
    else last = 0;
  };
  return {
    wake() { if (!raf && !stopped) raf = requestAnimationFrame(frame); },
    stop() { stopped = true; if (raf) cancelAnimationFrame(raf); raf = 0; last = 0; },
    get running() { return raf !== 0; },
  };
}

/* ---------- tokens ---------- */

/* key -> [the kit's variable (kit.css maps it to the app's, with the lab value), the app's variable, the lab value] */
export const TOKENS = Object.freeze({
  primary: ['--vk-primary', '--color-primary', '#1D4572'],
  ink: ['--vk-ink', '--color-foreground', '#1A2030'],
  muted: ['--vk-muted', '--color-foreground-muted', '#5E6370'],
  subtle: ['--vk-subtle', '--color-foreground-subtle', '#878B95'],
  border: ['--vk-border', '--color-border', '#E3E1DD'],
  borderSubtle: ['--vk-border-subtle', '--color-border-subtle', '#EFEEEB'],
  borderStrong: ['--vk-border-strong', '--color-border-strong', '#D4D1CC'],
  background: ['--vk-background', '--color-background', '#FFFFFF'],
  page: ['--vk-page', '--color-page', '#FFFFFF'],
  highlight: ['--vk-highlight', '--color-highlight', 'rgba(246, 222, 120, 0.55)'],
  focus: ['--vk-focus', '--color-border-focus', '#1D4572'],
  sans: ['--vk-sans', '--font-sans', "'Avenir Next', Avenir, 'Helvetica Neue', system-ui, sans-serif"],
  serif: ['--vk-serif', '--font-serif', "'EB Garamond', Georgia, serif"],
  mono: ['--vk-mono', '--font-mono', "ui-monospace, 'SF Mono', Menlo, monospace"],
});
const FONT_KEYS = ['sans', 'serif', 'mono'];

export function parseColor(str) {
  if (!str) return null;
  const s = String(str).trim();
  let m = s.match(/^#([0-9a-f]{3,8})$/i);
  if (m) {
    let x = m[1];
    if (x.length === 3 || x.length === 4) x = x.split('').map((c) => c + c).join('');
    if (x.length !== 6 && x.length !== 8) return null;
    const n = (i) => parseInt(x.slice(i, i + 2), 16);
    return [n(0), n(2), n(4), x.length === 8 ? n(6) / 255 : 1];
  }
  m = s.match(/^rgba?\(([^)]*)\)$/i);
  if (m) {
    const p = m[1].split(/[\s,/]+/).filter(Boolean);
    if (p.length < 3) return null;
    const ch = (v) => (v.endsWith('%') ? parseFloat(v) * 2.55 : parseFloat(v));
    const a = p[3] === undefined ? 1 : p[3].endsWith('%') ? parseFloat(p[3]) / 100 : parseFloat(p[3]);
    return [ch(p[0]), ch(p[1]), ch(p[2]), a];
  }
  m = s.match(/^color\(srgb\s+([^)]*)\)$/i);
  if (m) {
    const [rgb, alpha] = m[1].split('/');
    const p = rgb.trim().split(/\s+/).map(parseFloat);
    if (p.length < 3 || p.some((v) => !Number.isFinite(v))) return null;
    return [p[0] * 255, p[1] * 255, p[2] * 255, alpha === undefined ? 1 : parseFloat(alpha)];
  }
  return null;
}

export function rgba(c, alpha) {
  const a = alpha === undefined ? c[3] : alpha;
  return `rgba(${Math.round(c[0])},${Math.round(c[1])},${Math.round(c[2])},${Math.round(a * 1000) / 1000})`;
}

export function mix(a, b, t) {
  return [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t), lerp(a[3], b[3], t)];
}

const validColor = (v) => !!v && (typeof CSS === 'undefined' || !CSS.supports || CSS.supports('color', v));

export function readTokens(el) {
  const cs = getComputedStyle(el);
  const raw = (names) => {
    for (const n of names) {
      if (!n) continue;
      const v = cs.getPropertyValue(n).trim();
      if (v) return v;
    }
    return '';
  };
  const probe = document.createElement('span');
  probe.style.cssText = 'position:absolute;width:0;height:0;overflow:hidden;visibility:hidden;pointer-events:none';
  el.appendChild(probe);
  const resolve = (value, fallback) => {
    probe.style.color = '';
    probe.style.color = validColor(value) ? value : fallback;
    return parseColor(getComputedStyle(probe).color) || parseColor(fallback) || [0, 0, 0, 1];
  };
  const out = { rgb: {} };
  for (const key of Object.keys(TOKENS)) {
    if (FONT_KEYS.includes(key)) continue;
    const [kit, app, fallback] = TOKENS[key];
    out.rgb[key] = resolve(raw([kit, app]), fallback);
  }
  /* the three series: kit.css defines them; without it they are derived the same way */
  const s0 = raw(['--vk-s0']), s1 = raw(['--vk-s1']), s2 = raw(['--vk-s2']);
  out.rgb.s0 = s0 ? resolve(s0, TOKENS.primary[2]) : out.rgb.primary;
  out.rgb.s1 = s1 ? resolve(s1, TOKENS.ink[2]) : [...out.rgb.ink.slice(0, 3), out.rgb.ink[3] * 0.6];
  out.rgb.s2 = s2 ? resolve(s2, TOKENS.subtle[2]) : out.rgb.subtle;
  probe.remove();
  for (const key of Object.keys(out.rgb)) out[key] = rgba(out.rgb[key]);
  for (const key of FONT_KEYS) out[key] = raw([TOKENS[key][0], TOKENS[key][1]]) || TOKENS[key][2];
  out.signature = Object.keys(out.rgb).map((k) => out[k]).concat(FONT_KEYS.map((k) => out[k])).join('|');
  return out;
}

export function watchTheme(el, cb) {
  const targets = [];
  for (let n = el && el.parentElement; n && n.nodeType === 1; n = n.parentElement) targets.push(n);
  if (typeof document !== 'undefined' && !targets.includes(document.documentElement)) targets.push(document.documentElement);
  let raf = 0;
  const fire = () => { if (!raf) raf = requestAnimationFrame(() => { raf = 0; cb(); }); };
  const mo = typeof MutationObserver === 'function' ? new MutationObserver(fire) : null;
  if (mo) targets.forEach((t) => mo.observe(t, { attributes: true, attributeFilter: ['class', 'style', 'data-theme'] }));
  const dark = typeof matchMedia === 'function' ? matchMedia('(prefers-color-scheme: dark)') : null;
  if (dark) dark.addEventListener('change', fire);
  return () => {
    if (mo) mo.disconnect();
    if (dark) dark.removeEventListener('change', fire);
    if (raf) cancelAnimationFrame(raf);
  };
}

/* ---------- DOM, canvas and size ---------- */

export function h(tag, className, attrs) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (attrs) for (const k of Object.keys(attrs)) if (attrs[k] !== null && attrs[k] !== undefined) e.setAttribute(k, attrs[k]);
  return e;
}

const SVG_NS = 'http://www.w3.org/2000/svg';
export function svgEl(tag, attrs, parent) {
  const e = document.createElementNS(SVG_NS, tag);
  if (attrs) for (const k of Object.keys(attrs)) e.setAttribute(k, attrs[k]);
  if (parent) parent.appendChild(e);
  return e;
}

export function esc(s) {
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

let uidCount = 0;
export function uid(prefix = 'vk') {
  uidCount += 1;
  return `${prefix}-${uidCount}`;
}

export function dpr() {
  const r = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1;
  return clamp(r, 1, 3);
}

export function fitCanvas(canvas, width, height) {
  const r = dpr();
  const bw = Math.max(1, Math.round(width * r)), bh = Math.max(1, Math.round(height * r));
  if (canvas.width !== bw || canvas.height !== bh) { canvas.width = bw; canvas.height = bh; }
  const w = `${width}px`, hh = `${height}px`;
  if (canvas.style.width !== w) canvas.style.width = w;
  if (canvas.style.height !== hh) canvas.style.height = hh;
  const ctx = canvas.getContext('2d');
  ctx.setTransform(bw / width, 0, 0, bh / height, 0, 0);
  ctx.clearRect(0, 0, width, height);
  return ctx;
}

export function watchDpr(cb) {
  if (typeof matchMedia !== 'function') return () => {};
  let mq = null, stopped = false;
  const on = () => { if (stopped) return; cb(dpr()); arm(); };
  const arm = () => {
    mq = matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
    mq.addEventListener('change', on, { once: true });
  };
  arm();
  return () => { stopped = true; if (mq) mq.removeEventListener('change', on); };
}

export function observeWidth(el, cb) {
  let last = -1;
  const ro = new ResizeObserver((entries) => {
    const w = Math.round(entries[entries.length - 1].contentRect.width);
    if (w > 0 && w !== last) { last = w; cb(w); }
  });
  ro.observe(el);
  return () => ro.disconnect();
}

/* Empty in the source; charts/build.sh writes kit.css in here for the one-file copies. */
const KIT_CSS = '';

export function ensureStyles(doc = typeof document !== 'undefined' ? document : null) {
  if (!KIT_CSS || !doc || !doc.head) return;
  if (doc.querySelector('style[data-vk-kit]')) return;
  if (getComputedStyle(doc.documentElement).getPropertyValue('--vk-kit').trim()) return;
  const s = doc.createElement('style');
  s.setAttribute('data-vk-kit', '');
  s.textContent = KIT_CSS;
  doc.head.appendChild(s);
}

export function ensureCss(id, css, doc = typeof document !== 'undefined' ? document : null) {
  if (!id || !css || !doc || !doc.head) return;
  for (const s of doc.querySelectorAll('style[data-vk-css]')) if (s.getAttribute('data-vk-css') === id) return;
  const s = doc.createElement('style');
  s.setAttribute('data-vk-css', id);
  s.textContent = css;
  doc.head.appendChild(s);
}

/* ---------- the block, its readout and its foot ---------- */

export const PLOT = Object.freeze({ height: 200, pad: Object.freeze({ top: 8, right: 8, bottom: 22, left: 40 }) });
export const ROWS = Object.freeze({ title: 32, readout: 26, foot: 26 });

export function plotBox(width, height = PLOT.height, pad = PLOT.pad) {
  return { left: pad.left, right: Math.max(pad.left, width - pad.right), top: pad.top, bottom: height - pad.bottom, width, height };
}

export function block(el, { plotHeight = PLOT.height, label = '', hint = 'Use the left and right arrow keys to read each point.', readout = true, foot = true } = {}) {
  const root = h('div', 'vk-block');
  const ro = readout ? h('div', 'vk-readout', { 'aria-live': 'polite', 'aria-atomic': 'true' }) : null;
  const plot = h('div', 'vk-plot', { role: 'img', tabindex: '0' });
  plot.style.height = `${plotHeight}px`;
  if (label) plot.setAttribute('aria-label', label);
  const hintEl = h('span', 'vk-sr', { id: uid('vk-hint') });
  hintEl.textContent = hint;
  plot.setAttribute('aria-describedby', hintEl.id);
  const ft = foot ? h('div', 'vk-foot') : null;
  for (const part of [ro, plot, hintEl, ft]) if (part) root.appendChild(part);
  el.appendChild(root);
  return { root, readout: ro, plot, foot: ft, hint: hintEl };
}

export function setLabel(plot, text) {
  if (plot.getAttribute('aria-label') !== text) plot.setAttribute('aria-label', text);
}

export function setReadout(node, parts) {
  if (!node) return;
  const html = parts.map((p) => {
    if (p.when !== undefined) return `<span class="vk-when">${esc(p.when)}</span>`;
    const sw = p.swatch || '';
    const label = p.label ? `<span class="vk-lab">${esc(p.label)}</span>` : '';
    const value = p.value !== undefined ? `<b>${esc(p.value)}</b>` : '';
    return `<span class="vk-fig">${sw}${label}${value}</span>`;
  }).join('');
  if (node.__vkHtml !== html) { node.innerHTML = html; node.__vkHtml = html; }
}

/* The second series is ink at 60% and the third subtle ink drawn hollow and dotted, so three series never claim a
   second hue (design.md, "Color"). `pin` is the opacity of a stem's pinpricks. */
export const SERIES_STYLE = Object.freeze([
  Object.freeze({ line: 1.75, dash: null, head: 3, hollow: false, pin: 0.62 }),
  Object.freeze({ line: 1.5, dash: null, head: 2.6, hollow: false, pin: 0.62 }),
  Object.freeze({ line: 1.6, dash: Object.freeze([0, 3.5]), head: 2.4, hollow: true, pin: 0.7 }),
]);

export function swatch(series, mark) {
  const st = SERIES_STYLE[series] || SERIES_STYLE[0];
  const c = `var(--vk-s${Math.min(2, Math.max(0, series))})`;
  const head = st.hollow ? `fill:var(--vk-page);stroke:${c};stroke-width:1` : `fill:${c}`;
  if (mark === 'line') {
    const dash = st.dash ? `;stroke-dasharray:${st.dash.join(' ')}` : '';
    return `<svg class="vk-sw" width="16" height="8" viewBox="0 0 16 8" aria-hidden="true"><line x1="1.5" y1="4" x2="14.5" y2="4" style="stroke:${c};stroke-width:${st.line};stroke-linecap:round${dash}"/><circle cx="8" cy="4" r="2.2" style="${head}"/></svg>`;
  }
  return `<svg class="vk-sw" width="8" height="12" viewBox="0 0 8 12" aria-hidden="true"><circle cx="4" cy="11" r=".75" style="fill:${c};opacity:${st.pin}"/><circle cx="4" cy="7.5" r=".75" style="fill:${c};opacity:${st.pin}"/><circle cx="4" cy="3.2" r="2.4" style="${head}"/></svg>`;
}

export function foot(node, output, options = {}) {
  if (!node) return;
  node.textContent = '';
  const src = h('span', 'vk-src');
  src.textContent = 'From your records';
  node.appendChild(src);
  if (output && typeof output.ref === 'string' && output.ref && typeof options.onOpenRows === 'function') {
    const b = h('button', 'vk-rows', { type: 'button' });
    b.textContent = 'See the rows';
    b.addEventListener('click', () => options.onOpenRows(output.ref));
    node.appendChild(b);
  }
}

export function keyNav(target, { count, get, set, page = 10 }) {
  if (!target.hasAttribute('tabindex')) target.tabIndex = 0;
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey) return;
    const n = count();
    if (!n) return;
    const i = get();
    let j;
    switch (e.key) {
      case 'ArrowRight': j = i === null ? 0 : Math.min(n - 1, i + 1); break;
      case 'ArrowLeft': j = i === null ? n - 1 : Math.max(0, i - 1); break;
      case 'Home': j = 0; break;
      case 'End': j = n - 1; break;
      case 'PageDown': j = Math.min(n - 1, (i === null ? -1 : i) + page); break;
      case 'PageUp': j = Math.max(0, (i === null ? n : i) - page); break;
      case 'Escape':
        if (i === null) return;
        e.preventDefault();
        set(null, 'key');
        return;
      default: return;
    }
    e.preventDefault();
    set(j, 'key');
  };
  const onBlur = () => set(null, 'blur');
  target.addEventListener('keydown', onKey);
  target.addEventListener('blur', onBlur);
  return () => { target.removeEventListener('keydown', onKey); target.removeEventListener('blur', onBlur); };
}

