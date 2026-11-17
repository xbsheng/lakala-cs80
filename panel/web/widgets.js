// 组件定义：一份 draw() 同时用于「编辑器里的每个格子」和「推给屏的整帧」。
// 视觉语言照 glance：卡片底色略高于页面、大写小标题、弱化分隔线、数字用等宽、accent 金色。

export const THEME = {
  bg: '#141519',
  card: '#1a1c22',
  cardHi: '#20232b',
  line: '#2a2c34',
  fg: '#d6d7dc',
  fg2: '#b4b5bd',
  fg3: '#8b8c96',
  fg4: '#52535c',
  accent: '#d8bb85',
  up: '#ef6f6f',
  down: '#5fc98d',
};

import { solarToLunar } from './lunar.js';

const FONT = '-apple-system, BlinkMacSystemFont, "Inter", "SF Pro Text", "PingFang SC", "Microsoft YaHei", sans-serif';
const MONO = '"SF Mono", "JetBrains Mono", ui-monospace, Menlo, monospace';

const font = (size, weight = 400, mono = false) => `${weight} ${size}px ${mono ? MONO : FONT}`;

function roundRect(ctx, x, y, w, h, r) {
  const rr = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  if (ctx.roundRect) ctx.roundRect(x, y, w, h, rr);
  else ctx.rect(x, y, w, h);
}

/** 逐字加字距（canvas 没有 letter-spacing，但 glance 的观感靠它） */
function tracked(ctx, text, x, y, spacing, align = 'left') {
  const chars = [...String(text)];
  let total = 0;
  for (const ch of chars) total += ctx.measureText(ch).width + spacing;
  total -= spacing;
  let cx = align === 'right' ? x - total : align === 'center' ? x - total / 2 : x;
  for (const ch of chars) {
    ctx.fillText(ch, cx, y);
    cx += ctx.measureText(ch).width + spacing;
  }
}

function ellipsis(ctx, text, maxW) {
  text = String(text ?? '');
  if (ctx.measureText(text).width <= maxW) return text;
  let s = text;
  while (s.length > 1 && ctx.measureText(s + '…').width > maxW) s = s.slice(0, -1);
  return s + '…';
}

function fitFont(ctx, text, maxW, size, weight = 400, mono = false) {
  let s = size;
  for (let i = 0; i < 26; i++) {
    ctx.font = font(s, weight, mono);
    if (ctx.measureText(text).width <= maxW || s <= 9) break;
    s *= 0.92;
  }
  return s;
}

/** 卡片外壳：底 + 细边 + 大写标题 + 右侧 meta，返回内容区 */
function card(ctx, r, t, { title = '', meta = '', metaColor = '', pad = 16 } = {}) {
  const box = { x: r.x + 3, y: r.y + 3, w: r.w - 6, h: r.h - 6 };
  roundRect(ctx, box.x, box.y, box.w, box.h, 7);
  ctx.fillStyle = t.card;
  ctx.fill();
  ctx.strokeStyle = 'rgba(255,255,255,0.06)';
  ctx.lineWidth = 1;
  ctx.stroke();

  const inner = { x: box.x + pad, y: box.y + pad, w: box.w - pad * 2, h: box.h - pad * 2 };
  if (!title) return inner;

  const size = Math.max(11, Math.min(17, box.h * 0.062));
  ctx.textBaseline = 'top';
  ctx.font = font(size, 600);
  ctx.fillStyle = t.fg4;
  ctx.textAlign = 'left';
  tracked(ctx, title, inner.x, box.y + pad * 0.72, size * 0.1);
  if (meta) {
    ctx.font = font(size * 0.92, 500, true);
    ctx.fillStyle = metaColor || t.fg4;
    ctx.textAlign = 'right';
    ctx.fillText(ellipsis(ctx, meta, inner.w * 0.55), inner.x + inner.w, box.y + pad * 0.72);
  }
  const headH = size + pad * 0.9;
  const out = { x: inner.x, y: inner.y + headH, w: inner.w, h: inner.h - headH };
  out.headY = box.y + pad * 0.72; // 标题基线（顶部对齐）
  out.headH = headH;
  out.headFont = size;
  return out;
}

function bar(ctx, x, y, w, h, frac, color, t) {
  roundRect(ctx, x, y, w, h, h / 2);
  ctx.fillStyle = 'rgba(255,255,255,0.07)';
  ctx.fill();
  const filled = Math.max(h, w * Math.max(0, Math.min(1, frac)));
  roundRect(ctx, x, y, filled, h, h / 2);
  ctx.fillStyle = color;
  ctx.fill();
}

/** 虚线（列表分隔用） */
function dashed(ctx, x1, y1, x2, y2, color = 'rgba(255,255,255,0.14)') {
  ctx.save();
  ctx.setLineDash([4, 4]);
  ctx.strokeStyle = color;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(x1, y1 + 0.5);
  ctx.lineTo(x2, y2 + 0.5);
  ctx.stroke();
  ctx.restore();
}

/** 大数字 + 小字后缀（假期倒计时用）；align 决定 x 是左边界/中心/右边界 */
function bigDays(ctx, x, y, maxW, size, n, t, align = 'right', suffix = '天') {
  const num = String(n);
  let s = size;
  const widthOf = () => {
    ctx.font = font(s, 700);
    const nw = ctx.measureText(num).width;
    if (!suffix) return nw;
    ctx.font = font(s * 0.42, 500);
    return nw + s * 0.1 + ctx.measureText(suffix).width;
  };
  while (widthOf() > maxW && s > 9) s *= 0.93;
  ctx.font = font(s, 700);
  const nw = ctx.measureText(num).width;
  const gap = suffix ? s * 0.1 : 0;
  ctx.font = font(s * 0.42, 500);
  const sw = suffix ? ctx.measureText(suffix).width : 0;
  const total = nw + gap + sw;
  const nx = align === 'center' ? x - total / 2 : align === 'left' ? x : x - total;
  ctx.textAlign = 'left';
  ctx.textBaseline = 'alphabetic';
  ctx.font = font(s, 700);
  ctx.fillStyle = t.fg;
  ctx.fillText(num, nx, y);
  if (suffix) {
    ctx.font = font(s * 0.42, 500);
    ctx.fillStyle = t.fg3;
    ctx.fillText(suffix, nx + nw + gap, y);
  }
}

const mdText = (iso) => {
  const [, m, d] = String(iso).split('-');
  return `${Number(m)}.${Number(d)}`;
};
const dateRange = (it) => (it.start === it.end ? mdText(it.start) : `${mdText(it.start)}–${mdText(it.end)}`);

/** 假期组件的一行：左名称/日期，右侧「还差几天」 */
function holidayRow(ctx, x, y, w, h, it, t) {
  const v = it.ongoing ? it.left : it.until;
  const cy = y + h / 2;
  const numSize = Math.min(h * 0.5, 48);
  const suffix = w >= 200 ? '天' : ''; // 特别窄的格子省掉「天」，把宽度留给名称/日期
  // 先算出右边数字块占多宽，左边「名称 + 日期」按剩下宽度自适应
  ctx.font = font(numSize * 0.42, 500);
  const numW = (suffix ? ctx.measureText(suffix).width + numSize * 0.1 : 0);
  ctx.font = font(numSize, 700);
  const avail = w - (numW + ctx.measureText(String(v)).width) - h * 0.16;

  const range = dateRange(it);
  const nameSize = Math.max(13, Math.min(h * 0.4, 34));
  const gap = () => nameSize * 0.45;
  const widthOf = (ns) => {
    ctx.font = font(ns, 500);
    const nw = ctx.measureText(it.name).width;
    ctx.font = font(ns * 0.72, 400);
    return nw + ns * 0.45 + ctx.measureText(range).width;
  };
  let ns = nameSize;
  while (widthOf(ns) > avail && ns > 13) ns *= 0.94;
  const withDate = widthOf(ns) <= avail; // 太窄就只留名称，不硬塞

  ctx.textAlign = 'left';
  ctx.textBaseline = 'middle';
  ctx.font = font(ns, 500);
  ctx.fillStyle = t.fg2;
  ctx.fillText(it.name, x, cy);
  if (withDate) {
    const nw = ctx.measureText(it.name).width;
    ctx.font = font(ns * 0.72, 400);
    ctx.fillStyle = t.fg3;
    ctx.fillText(range, x + nw + gap(), cy);
  }

  bigDays(ctx, x + w, cy + Math.min(h * 0.19, 18), w * 0.58, numSize, v, t, 'right', suffix);
}

const pctColor = (v, t) => (v >= 0 ? t.up : t.down);
const signed = (v, digits = 2) => `${v >= 0 ? '+' : ''}${Number(v).toFixed(digits)}`;

/**
 * 列表布局：行高按内容定（不随高度压缩），高度不够就少显示几条。
 * baseRowH 是「一行该多高」，只由宽度/字号决定；只有一行都放不下时才允许缩。
 */
function rowsFor(innerH, baseRowH, count) {
  const rows = Math.max(1, Math.min(count || 1, Math.floor(innerH / baseRowH)));
  return { rows, rowH: Math.min(baseRowH, innerH / rows) };
}

/** 某时区的「周几 + 当天第几分钟」 */
function tzClock(tz) {
  const p = new Intl.DateTimeFormat('en-US', {
    timeZone: tz, hour12: false, weekday: 'short', hour: '2-digit', minute: '2-digit',
  }).formatToParts(new Date());
  const get = (t) => (p.find((x) => x.type === t) || {}).value || '';
  const day = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'].indexOf(get('weekday'));
  return { day, minutes: (Number(get('hour')) % 24) * 60 + Number(get('minute')) };
}

const inWindow = (m, a, b) => m >= a && m <= b;

/**
 * 现在该看哪一组行情：
 *   A 股盘中（北京 9:15–15:00，周一–五；9:15 开始集合竞价）→ 自选股
 *   美股盘中（纽约 9:30–16:00，周一–五）→ 美股指数
 *   其余时间（含美股盘前、A 股收盘后）→ 美股期货
 */
export function stockSession() {
  const cn = tzClock('Asia/Shanghai');
  if (cn.day >= 1 && cn.day <= 5 && inWindow(cn.minutes, 555, 900)) return { id: 'cn', label: '自选股' };
  const us = tzClock('America/New_York');
  if (us.day >= 1 && us.day <= 5 && inWindow(us.minutes, 570, 960)) return { id: 'us', label: '美股指数' };
  return { id: 'fut', label: '美股期货' };
}

/** A 股午间休市（北京 11:30–13:00）：不轮询，屏上保留上午收盘数据 */
export function inCnLunch() {
  const cn = tzClock('Asia/Shanghai');
  return cn.day >= 1 && cn.day <= 5 && cn.minutes >= 690 && cn.minutes < 780;
}

/** 股票组件是否需要按 TTL 轮询；午休且有旧数据时由 core.js 跳过请求 */
export function stockPolling() {
  return stockSession().id !== 'cn' || !inCnLunch();
}

/** 当前这组代码：没配 us / usFut 就回落到 symbols（保持老配置也能用） */
export function stockSymbols(cfg = {}) {
  const id = stockSession().id;
  const pick = id === 'cn' ? cfg.symbols : id === 'us' ? cfg.us : cfg.usFut;
  return String(pick || cfg.symbols || 'sh000001').trim();
}

/** A 股交易时段（周一–周五 9:30–11:30 / 13:00–15:00）；休盘就不必再轮询（K 线用） */
export function tradingHours() {
  const d = new Date();
  const day = d.getDay();
  if (day === 0 || day === 6) return false;
  const m = d.getHours() * 60 + d.getMinutes();
  return (m >= 570 && m <= 690) || (m >= 780 && m <= 900);
}

/** 小字：数据新鲜度。旧数据 → 「更新失败·重取中」，正常 → 「12:31 更新」 */
export function freshStamp(d) {
  if (!d || !d.fetchedAt) return null;
  if (d.stale) return { text: '更新失败·重取中', color: '#fabe46' };
  const at = new Date(d.fetchedAt);
  const hm = `${String(at.getHours()).padStart(2, '0')}:${String(at.getMinutes()).padStart(2, '0')}`;
  const today = at.toDateString() === new Date().toDateString();
  return { text: today ? `${hm} 更新` : `${at.getMonth() + 1}/${at.getDate()} ${hm}`, color: '' };
}

/** 相对新鲜度：5s前更新 / 2m前更新；拉取失败仍是黄色提示 */
export function freshStampAgo(d) {
  if (!d || !d.fetchedAt) return null;
  if (d.stale) return { text: '更新失败·重取中', color: '#fabe46' };
  const secs = Math.max(0, Math.floor((Date.now() - new Date(d.fetchedAt).getTime()) / 1000));
  let text;
  if (secs < 1) text = '刚刚更新';
  else if (secs < 60) text = `${secs}s前更新`;
  else if (secs < 3600) text = `${Math.floor(secs / 60)}m前更新`;
  else text = `${Math.floor(secs / 3600)}h前更新`;
  return { text, color: '' };
}

/** 股票组件专用：轮询中“5s前更新”，停盘/午休“10:28更新” */
export function stockStamp(d, polling) {
  if (polling) return freshStampAgo(d);
  if (!d || !d.fetchedAt) return null;
  if (d.stale) return { text: '更新失败·重取中', color: '#fabe46' };
  const at = new Date(d.fetchedAt);
  const hm = `${String(at.getHours()).padStart(2, '0')}:${String(at.getMinutes()).padStart(2, '0')}`;
  const today = at.toDateString() === new Date().toDateString();
  return { text: today ? `${hm}更新` : `${at.getMonth() + 1}/${at.getDate()} ${hm}更新`, color: '' };
}

function ago(iso) {
  const mins = Math.max(0, Math.round((Date.now() - new Date(iso).getTime()) / 60000));
  if (mins < 60) return `${mins}m ago`;
  if (mins < 1440) return `${Math.round(mins / 60)}h ago`;
  return `${Math.round(mins / 1440)}d ago`;
}

const AGENT_LOGOS = {
  pi: 'logos/pi.svg',
  claude: 'logos/claude.svg',
  codex: 'logos/openai.svg',
  deepseek: 'logos/deepseek.svg',
};
const logoCache = new Map();
// 模块加载时就把 logo 取下来，避免第一帧画成占位方块
for (const src of Object.values(AGENT_LOGOS)) {
  const img = new Image();
  img.src = src;
  logoCache.set(src, img);
}
function agentLogo(agent) {
  const src = AGENT_LOGOS[agent];
  if (!src) return null;
  const img = logoCache.get(src) || (() => {
    const i = new Image();
    i.src = src;
    logoCache.set(src, i);
    return i;
  })();
  return img.complete && img.naturalWidth ? img : null;
}

// ---------------- RunCat ----------------
// 猫素材：runcat-dev/RunCat365 `RunCat365/resources/game/cat_running_0..4.png`
// （Apache-2.0，120×100 黑色剪影 + alpha，5 帧一个奔跑步态，可任意染色）
const RUNCAT = { cpu: '#5fc98d', mem: '#5aa9ff' };
const CAT_N = 5;
const CAT_W = 120;
const CAT_H = 100;
const CAT_AR = CAT_H / CAT_W;
const CAT_CONTENT_CY = 0.635; // 内容（不含上下留白）的垂直中心占比，用于垂直居中
const CAT_SRC = Array.from({ length: CAT_N }, (_, i) => {
  const img = new Image();
  img.src = `assets/runcat/cat_running_${i}.png`;
  return img;
});
const catTinted = new Map();

function catSprite(frame, color) {
  const img = CAT_SRC[frame % CAT_N];
  if (!img.complete || !img.naturalWidth) return null;
  const key = (frame % CAT_N) + color;
  let c = catTinted.get(key);
  if (!c) {
    c = document.createElement('canvas');
    c.width = CAT_W;
    c.height = CAT_H;
    const g = c.getContext('2d');
    g.drawImage(img, 0, 0);
    g.globalCompositeOperation = 'source-in'; // 只给不透明像素上色，抗锯齿边缘保留
    g.fillStyle = color;
    g.fillRect(0, 0, CAT_W, CAT_H);
    catTinted.set(key, c);
  }
  return c;
}

// 速度照 RunCat 的思路（cpu 越高跑越快），但这条管线 200ms 一帧，
// 所以折算成「每 tick 前进 1~2 帧」，用累加器保证步态连续不跳帧。
// ponytail: 视觉上限 5 fps；要 RunCat 原版 40fps 得让 pusher 单独提速猫的 tile。
const cat = { frame: 0, at: 0, acc: 0 };
function catFrame(cpu) {
  const now = performance.now();
  if (now - cat.at >= 150) {
    cat.at = now;
    cat.acc += 1 + 2 * Math.min(1, Math.max(0, cpu));
    const step = Math.min(2, Math.floor(cat.acc)) || 1;
    cat.acc -= step;
    cat.frame = (cat.frame + step) % CAT_N;
  }
  return cat.frame;
}

/**
 * @param cx,cy 锚点：anchor='center' 时是内容中心，'feet' 时是脚底线
 * @param w 猫的显示宽度（高按 100:120 自动算）
 */
function drawCat(ctx, cx, cy, w, color, frame, anchor = 'center') {
  const img = catSprite(frame, color);
  if (!img) return;
  const h = w * CAT_AR;
  const top = anchor === 'feet' ? cy - h : cy - h * CAT_CONTENT_CY;
  ctx.drawImage(img, cx - w / 2, top, w, h);
}

/** 270° 缺口朝下的圆弧进度条 */
function gauge(ctx, cx, cy, r, lw, frac, color) {
  const a0 = Math.PI * 0.75;
  const sweep = Math.PI * 1.5;
  ctx.lineCap = 'round';
  ctx.lineWidth = lw;
  ctx.strokeStyle = 'rgba(255,255,255,0.07)';
  ctx.beginPath();
  ctx.arc(cx, cy, r, a0, a0 + sweep);
  ctx.stroke();
  const f = Math.min(1, Math.max(0, frac));
  if (f > 0.004) {
    ctx.strokeStyle = color;
    ctx.beginPath();
    ctx.arc(cx, cy, r, a0, a0 + sweep * f);
    ctx.stroke();
  }
  ctx.lineCap = 'butt';
}

const fmtRate = (v) => {
  const n = Math.max(0, Number(v) || 0);
  if (n < 1024) return `${Math.round(n)}B`;
  if (n < 1024 ** 2) return `${Math.round(n / 1024)}K`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(n < 1024 ** 2 * 10 ? 1 : 0)}M`;
  return `${(n / 1024 ** 3).toFixed(1)}G`;
};
const netText = (d) => `↑${fmtRate(d && d.netUp)}  ↓${fmtRate(d && d.netDown)}`;

// 折线历史：200ms 循环下一次 / 秒，60 点 = 最近一分钟
const HIST_N = 60;
const hist = { cpu: [], mem: [], at: 0 };
function sampleHist(cpu, mem) {
  const now = performance.now();
  if (!hist.cpu.length) {
    // 第一帧铺平，避免来个斜线
    for (let i = 0; i < HIST_N; i++) {
      hist.cpu.push(cpu);
      hist.mem.push(mem);
    }
    hist.at = now;
    return;
  }
  if (now - hist.at < 900) return;
  hist.at = now;
  hist.cpu.push(cpu);
  hist.mem.push(mem);
  if (hist.cpu.length > HIST_N) {
    hist.cpu.shift();
    hist.mem.shift();
  }
}

/** Y 轴固定 0..100%，不 auto-scale（否则线会一直贴着上下边缘抖） */
function sparkline(ctx, x, y, w, h, data, color) {
  ctx.strokeStyle = 'rgba(255,255,255,0.06)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(x, y + h + 0.5);
  ctx.lineTo(x + w, y + h + 0.5);
  ctx.stroke();
  if (data.length < 2) return;
  const dx = w / (HIST_N - 1);
  const px = (i) => x + (i + HIST_N - data.length) * dx; // 新点贴右边
  const py = (v) => y + h - Math.min(1, Math.max(0, v)) * h;
  ctx.beginPath();
  data.forEach((v, i) => (i ? ctx.lineTo(px(i), py(v)) : ctx.moveTo(px(i), py(v))));
  ctx.lineTo(px(data.length - 1), y + h);
  ctx.lineTo(px(0), y + h);
  ctx.closePath();
  ctx.fillStyle = color;
  ctx.globalAlpha = 0.07;
  ctx.fill();
  ctx.globalAlpha = 1;
  ctx.beginPath();
  data.forEach((v, i) => (i ? ctx.lineTo(px(i), py(v)) : ctx.moveTo(px(i), py(v))));
  ctx.strokeStyle = color;
  ctx.lineWidth = 2;
  ctx.lineJoin = 'round';
  ctx.stroke();
}
/**
 * 分时整段时间轴的分钟数：
 *   A 股 241（9:30–15:00）/ 美股 391（9:30–16:00）/ CME 期货 1380（北京 07:00–次日 06:00）。
 * 有了它，盘中曲线只画到当前进度，右边留白，和券商 APP 一致。
 */
export function minuteSlots(symbol) {
  const s = String(symbol || '').trim();
  if (/^103\./.test(s)) return 1380;  // 外盘期货（ES/NQ…）
  if (/^100\./.test(s)) return 391;   // 美股指数
  return 241;                          // A 股
}

/**
 * 分时图：昨收虚线 + 涨跌面积/折线（box 用绝对坐标）。
 * slots = 整个交易时段的分钟数（A 股 241 / 美股 391 / 期货 1380）：给了就按时间轴画，
 * 盘中只画到当前进度、右边留白；不给就按现有数据铺满。
 */
export function minuteChart(ctx, box, points, preClose, color, slots) {
  if (!points || points.length < 2) {
    ctx.fillStyle = 'rgba(255,255,255,.18)';
    ctx.font = font(Math.max(10, box.h * 0.22), 400);
    ctx.textAlign = 'left';
    ctx.textBaseline = 'middle';
    ctx.fillText('无分时', box.x, box.y + box.h / 2);
    return;
  }
  const prices = points.map((p) => p[1]);
  let lo = Math.min(...prices, preClose || prices[0]);
  let hi = Math.max(...prices, preClose || prices[0]);
  const span0 = hi - lo || 1;
  lo -= span0 * 0.12;
  hi += span0 * 0.12;
  const span = Math.max(prices.length - 1, (slots || prices.length) - 1);
  const px = (i) => box.x + (i / span) * box.w;
  const py = (v) => box.y + box.h - ((v - lo) / (hi - lo)) * box.h;

  if (preClose) {
    ctx.strokeStyle = 'rgba(255,255,255,.14)';
    ctx.setLineDash([3, 4]);
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(box.x, py(preClose));
    ctx.lineTo(box.x + box.w, py(preClose));
    ctx.stroke();
    ctx.setLineDash([]);
  }
  const grad = ctx.createLinearGradient(0, box.y, 0, box.y + box.h);
  grad.addColorStop(0, color + '4d');
  grad.addColorStop(1, color + '00');
  ctx.beginPath();
  ctx.moveTo(px(0), py(prices[0]));
  prices.forEach((v, i) => ctx.lineTo(px(i), py(v)));
  ctx.lineTo(px(prices.length - 1), box.y + box.h);
  ctx.lineTo(px(0), box.y + box.h);
  ctx.closePath();
  ctx.fillStyle = grad;
  ctx.fill();

  ctx.beginPath();
  prices.forEach((v, i) => (i ? ctx.lineTo(px(i), py(v)) : ctx.moveTo(px(i), py(v))));
  ctx.strokeStyle = color;
  ctx.lineWidth = Math.max(1.2, box.h * 0.03);
  ctx.lineJoin = 'round';
  ctx.stroke();
}

/**
 * 一只股票一行（自定义股票页和股票组件共用）：
 * [名称/代码] [分时] [价格/涨跌幅]
 */
export function stockRow(ctx, r, item, t, opts = {}) {
  // 组件里没有卡片底色时，左右不再留内边距，保证和卡片标题对齐
  const pad = opts.card === false ? 0 : Math.max(8, r.h * 0.14);
  if (opts.card !== false) {
    roundRect(ctx, r.x, r.y, r.w, r.h, Math.min(8, r.h * 0.14));
    ctx.fillStyle = t.cardHi;
    ctx.fill();
  }
  const hasQuote = item.price != null && Number(item.price) > 0 && item.prev != null;
  const up = (item.pct ?? 0) >= 0;
  const color = hasQuote ? (up ? t.up : t.down) : t.fg4;

  // 名称 + 代码
  const nameW = Math.min(r.w * 0.36, Math.max(96, r.w * 0.3));
  ctx.textAlign = 'left';
  ctx.textBaseline = 'top';
  ctx.fillStyle = t.fg2;
  ctx.font = font(Math.max(12, Math.min(r.h * 0.22, 26)), 500);
  ctx.fillText(ellipsis(ctx, item.name || item.symbol || '', nameW), r.x + pad, r.y + r.h * 0.22);
  ctx.fillStyle = t.fg4;
  ctx.font = font(Math.max(9, Math.min(r.h * 0.15, 17)), 400, true);
  ctx.fillText(String(item.symbol || '').toUpperCase(), r.x + pad, r.y + r.h * 0.58);

  // 价格 + 涨跌幅（右侧）
  const priceW = Math.min(r.w * 0.26, Math.max(84, r.w * 0.22));
  // 没拿到快照时画占位符，别显示 0.00 让人以为跌停了
  const priceTxt = hasQuote ? Number(item.price).toFixed(2) : '—';
  const pctTxt = hasQuote ? `${up ? '+' : ''}${Number(item.pct ?? 0).toFixed(2)}%` : '';
  ctx.textAlign = 'right';
  ctx.fillStyle = color;
  ctx.font = font(fitFont(ctx, priceTxt, priceW, Math.min(r.h * 0.3, 34), 400, true), 400, true);
  ctx.fillText(priceTxt, r.x + r.w - pad, r.y + r.h * 0.2);
  ctx.font = font(Math.max(10, Math.min(r.h * 0.19, 22)), 500, true);
  ctx.fillText(pctTxt, r.x + r.w - pad, r.y + r.h * 0.58);

  // 中间：分时
  const sx = r.x + pad + nameW + pad * 0.6;
  const sw = r.x + r.w - pad - priceW - pad * 0.6 - sx;
  if (sw > 30) {
    const chart = { x: sx, y: r.y + r.h * 0.2, w: sw, h: r.h * 0.62 };
    minuteChart(ctx, chart, item.points, item.preClose ?? item.prev, color, opts.slots);
  }
}

/** open-meteo 的 WMO weather_code → 图标类别 */
function weatherKind(code) {
  const c = Number(code);
  if (c === 0) return 'clear';
  if (c <= 2) return 'partly';
  if (c === 3) return 'cloud';
  if (c === 45 || c === 48) return 'fog';
  if (c >= 51 && c <= 57) return 'drizzle';
  if ((c >= 61 && c <= 67) || (c >= 80 && c <= 82)) return 'rain';
  if ((c >= 71 && c <= 77) || c === 85 || c === 86) return 'snow';
  if (c >= 95) return 'thunder';
  return 'cloud';
}

const WX_COLOR = { sun: '#e8c169', cloud: '#9aa3b2', rain: '#5aa9ff', snow: '#d6d7dc', bolt: '#fabe46' };

/** 在 100×100 逻辑坐标里画天气图标，居中于 (cx,cy)，边长 size */
function weatherIcon(ctx, cx, cy, size, code) {
  const kind = weatherKind(code);
  const S = size / 100;
  ctx.save();
  ctx.translate(cx, cy);
  ctx.scale(S, S);
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';

  const sun = (x, y, r) => {
    ctx.fillStyle = WX_COLOR.sun;
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = WX_COLOR.sun;
    ctx.lineWidth = 6;
    for (let i = 0; i < 8; i++) {
      const a = (i / 8) * Math.PI * 2;
      ctx.beginPath();
      ctx.moveTo(x + Math.cos(a) * (r + 8), y + Math.sin(a) * (r + 8));
      ctx.lineTo(x + Math.cos(a) * (r + 18), y + Math.sin(a) * (r + 18));
      ctx.stroke();
    }
  };

  const cloud = (x, y, scale = 1) => {
    ctx.save();
    ctx.translate(x, y);
    ctx.scale(scale, scale);
    ctx.fillStyle = WX_COLOR.cloud;
    ctx.beginPath();
    ctx.arc(-16, 4, 17, 0, Math.PI * 2); // 左
    ctx.arc(4, -6, 22, 0, Math.PI * 2); // 中
    ctx.arc(24, 5, 16, 0, Math.PI * 2); // 右
    ctx.fill();
    ctx.beginPath();
    ctx.roundRect(-32, 4, 64, 22, 11);
    ctx.fill();
    ctx.restore();
  };

  const drops = (n, len) => {
    ctx.strokeStyle = WX_COLOR.rain;
    ctx.lineWidth = 6;
    for (let i = 0; i < n; i++) {
      const x = -20 + i * 18;
      ctx.beginPath();
      ctx.moveTo(x, 32);
      ctx.lineTo(x - 6, 32 + len);
      ctx.stroke();
    }
  };

  const flakes = (n) => {
    ctx.strokeStyle = WX_COLOR.snow;
    ctx.lineWidth = 4;
    for (let i = 0; i < n; i++) {
      const x = -20 + i * 18;
      const y = 40;
      ctx.beginPath();
      ctx.moveTo(x - 6, y - 6); ctx.lineTo(x + 6, y + 6);
      ctx.moveTo(x + 6, y - 6); ctx.lineTo(x - 6, y + 6);
      ctx.stroke();
    }
  };

  if (kind === 'clear') {
    sun(0, 0, 26);
  } else if (kind === 'partly') {
    sun(-18, -20, 20);
    cloud(4, 6, 0.92);
  } else if (kind === 'cloud') {
    cloud(0, -4, 1);
  } else if (kind === 'fog') {
    cloud(0, -12, 0.95);
    ctx.strokeStyle = WX_COLOR.cloud;
    ctx.lineWidth = 5;
    [-2, 12, 26].forEach((y, i) => {
      ctx.beginPath();
      ctx.moveTo(-26 + i * 4, y + 26);
      ctx.lineTo(26 - i * 4, y + 26);
      ctx.stroke();
    });
  } else if (kind === 'drizzle') {
    cloud(0, -8, 0.95);
    drops(3, 10);
  } else if (kind === 'rain') {
    cloud(0, -10, 0.95);
    drops(3, 18);
  } else if (kind === 'snow') {
    cloud(0, -10, 0.95);
    flakes(3);
  } else if (kind === 'thunder') {
    cloud(0, -10, 0.95);
    ctx.fillStyle = WX_COLOR.bolt;
    ctx.beginPath();
    ctx.moveTo(2, 26);
    ctx.lineTo(-10, 48);
    ctx.lineTo(2, 48);
    ctx.lineTo(-4, 66);
    ctx.lineTo(14, 40);
    ctx.lineTo(2, 40);
    ctx.lineTo(10, 26);
    ctx.closePath();
    ctx.fill();
  }
  ctx.restore();
}

// ---------------- 字体 / 七段数码管 ----------------
/** 七段数码管：不依赖系统字体，任意尺寸都清晰（液晶风格） */
export function drawSevenSeg(ctx, text, x, y, h, color, opts = {}) {
  const dim = opts.dim || 'rgba(255,255,255,0.045)';
  const t = h * (opts.thickness || 0.115);
  const w = h * 0.56;
  const gap = h * (opts.gap || 0.16);
  const SEG = {
    0: 'abcdef', 1: 'bc', 2: 'abged', 3: 'abgcd', 4: 'fgbc', 5: 'afgcd',
    6: 'afgedc', 7: 'abc', 8: 'abcdefg', 9: 'abcdfg', '-': 'g', ':': '', ' ': '',
  };
  let cx = x;
  ctx.save();
  ctx.lineCap = 'round';
  const seg = (sx, sy, sw, sh, on) => {
    ctx.fillStyle = on ? color : dim;
    roundRect(ctx, sx, sy, sw, sh, Math.min(sw, sh) / 2);
    ctx.fill();
  };
  for (const ch of String(text)) {
    if (ch === ':') {
      ctx.fillStyle = color;
      const r = t * 0.5;
      ctx.beginPath();
      ctx.arc(cx + t * 0.6, y + h * 0.32, r, 0, Math.PI * 2);
      ctx.arc(cx + t * 0.6, y + h * 0.68, r, 0, Math.PI * 2);
      ctx.fill();
      cx += t * 2.2 + gap * 0.6;
      continue;
    }
    const on = SEG[ch] || '';
    const l = t; // 段厚
    // a 上 / g 中 / d 下
    seg(cx + l, y, w - 2 * l, l, on.includes('a'));
    seg(cx + l, y + h / 2 - l / 2, w - 2 * l, l, on.includes('g'));
    seg(cx + l, y + h - l, w - 2 * l, l, on.includes('d'));
    // f 左上 / b 右上
    seg(cx, y + l, l, h / 2 - 1.5 * l, on.includes('f'));
    seg(cx + w - l, y + l, l, h / 2 - 1.5 * l, on.includes('b'));
    // e 左下 / c 右下
    seg(cx, y + h / 2 + l / 2, l, h / 2 - 1.5 * l, on.includes('e'));
    seg(cx + w - l, y + h / 2 + l / 2, l, h / 2 - 1.5 * l, on.includes('c'));
    cx += w + gap;
  }
  ctx.restore();
  return cx - gap - x;
}

export const sevenSegWidth = (text, h, opts = {}) => {
  const w = h * 0.56;
  const gap = h * (opts.gap || 0.16);
  const t = h * (opts.thickness || 0.115);
  let total = 0;
  let count = 0;
  for (const ch of String(text)) {
    if (ch === ':') total += t * 2.2 + gap * 0.6;
    else { total += w + gap; count++; }
  }
  return total - gap;
};

/**
 * 射线起点相对矩形中心偏移 (ox,oy)、方向 ang，取到圆角矩形边界的距离（二分）。
 * 表盘刻度用它落点：12 点正上方、3 点正右、6 点正下、9 点正左，而不是沿周长等距。
 */
function rrEdgeDistance(ox, oy, ang, w, h, r) {
  const hw = w / 2;
  const hh = h / 2;
  const cos = Math.cos(ang);
  const sin = Math.sin(ang);
  const inside = (d) => {
    const x = Math.abs(ox + cos * d);
    const y = Math.abs(oy + sin * d);
    if (x > hw || y > hh) return false;
    const dx = Math.max(x - (hw - r), 0);
    const dy = Math.max(y - (hh - r), 0);
    return dx * dx + dy * dy <= r * r;
  };
  let lo = 0;
  let hi = Math.hypot(w, h);
  for (let i = 0; i < 28; i++) {
    const mid = (lo + hi) / 2;
    if (inside(mid)) lo = mid;
    else hi = mid;
  }
  return lo;
}

/** WMO code → 中文天气词 */
export const WEATHER_TEXT = (code) => {
  const c = Number(code);
  if (c === 0) return '晴';
  if (c <= 2) return '多云';
  if (c === 3) return '阴';
  if (c === 45 || c === 48) return '有雾';
  if (c >= 51 && c <= 57) return '毛毛雨';
  if (c >= 61 && c <= 65) return '雨';
  if (c === 66 || c === 67) return '冻雨';
  if ((c >= 71 && c <= 77) || c === 85 || c === 86) return '雪';
  if (c >= 80 && c <= 82) return '阵雨';
  if (c >= 95) return '雷暴';
  return '多云';
};


// ---------------- Home Assistant ----------------
// 概览组件只做“看”：温度/湿度/第三项大数字 + 4 个状态 chip + 底部功率。
// token 只存在 host 的 config.json，这里只传 entity_id。
const HA_ROLES = ['temp', 'humidity', 'air', 'climate', 'lights', 'switches', 'doors', 'lock', 'power', 'energy', 'media'];
const HA_CLIMATE_LABEL = { off: '关', cool: '制冷', heat: '制热', dry: '除湿', fan_only: '送风', auto: '自动' };
const HA_MEDIA_LABEL = { playing: '播放中', paused: '暂停', idle: '待机', off: '关', on: '开', buffering: '缓冲' };
const HA_LOCK_LABEL = { locked: '已锁', unlocked: '未锁', locking: '上锁中', unlocking: '开锁中', jammed: '卡住' };
const HA_WEATHER_LABEL = { clear: '晴', sunny: '晴', partlycloudy: '多云', cloudy: '阴', fog: '雾', hail: '冰雹',
  lightning: '雷雨', rainy: '雨', pouring: '暴雨', snowy: '雪', windy: '大风', exceptional: '异常' };

function haEntityIds(cfg) {
  const out = [];
  for (const k of HA_ROLES) {
    for (const part of String((cfg && cfg[k]) || '').split(',')) {
      const id = part.trim();
      if (id && !out.includes(id)) out.push(id);
    }
  }
  return out;
}

function haText(it) {
  if (!it) return '—';
  const s = String(it.state ?? '');
  if (!s || s === 'unknown' || s === 'unavailable') return '—';
  const dom = it.domain;
  if (dom === 'climate') return HA_CLIMATE_LABEL[s] || s;
  if (dom === 'media_player') return HA_MEDIA_LABEL[s] || s;
  if (dom === 'lock') return HA_LOCK_LABEL[s] || s;
  if (dom === 'weather') return HA_WEATHER_LABEL[s] || s;
  if (dom === 'binary_sensor' || dom === 'input_boolean' || dom === 'light' || dom === 'switch')
    return s === 'on' ? '开' : s === 'off' ? '关' : s;
  return s;
}

function haMetric(ctx, x, y, w, h, label, value, unit, color, t) {
  ctx.textAlign = 'left';
  ctx.textBaseline = 'top';
  ctx.font = font(Math.max(11, Math.min(14, h * 0.13)), 600);
  ctx.fillStyle = t.fg4;
  tracked(ctx, label, x, y, 1.1);
  const size = fitFont(ctx, String(value), w - 4, Math.min(h * 0.58, 64), 300, true);
  ctx.font = font(size, 300, true);
  ctx.fillStyle = color;
  ctx.textBaseline = 'alphabetic';
  const base = y + h * 0.78;
  ctx.fillText(String(value), x, base);
  if (unit) {
    const vw = ctx.measureText(String(value)).width;
    ctx.font = font(Math.max(13, size * 0.4), 400, true);
    ctx.fillStyle = t.fg3;
    ctx.fillText(unit, x + vw + 5, base);
  }
}

function haChip(ctx, x, y, w, h, label, text, color, t) {
  roundRect(ctx, x, y, w, h, 7);
  ctx.fillStyle = 'rgba(255,255,255,0.035)';
  ctx.fill();
  ctx.strokeStyle = 'rgba(255,255,255,0.06)';
  ctx.lineWidth = 1;
  ctx.stroke();
  const dot = Math.max(3.5, Math.min(5, h * 0.11));
  ctx.beginPath();
  ctx.arc(x + 12, y + h / 2, dot, 0, Math.PI * 2);
  ctx.fillStyle = color;
  ctx.fill();
  ctx.textAlign = 'left';
  ctx.textBaseline = 'middle';
  const size = Math.max(12, Math.min(17, h * 0.33));
  const tx = x + 12 + dot + 7;
  ctx.font = font(size * 0.84, 600);
  ctx.fillStyle = t.fg4;
  ctx.fillText(label, tx, y + h / 2 - size * 0.52);
  ctx.font = font(size, 500);
  ctx.fillStyle = t.fg2;
  ctx.fillText(ellipsis(ctx, text, w - (tx - x) - 8), tx, y + h / 2 + size * 0.52);
}

export const WIDGETS = {
  clock: {
    label: '时钟',
    icon: '<circle cx="8" cy="8" r="6"/><path d="M8 4.5V8l2.5 1.5"/>',
    min: [2, 1],
    config: { city: '北京', showWeather: true, font: 'mono' },
    needs: { kind: 'weather', ttl: 600000, query: (cfg) => ({ city: cfg.city || '北京' }) },
    draw(ctx, r, cfg, d, t) {
      const now = new Date();
      const pad = Math.max(10, r.w * 0.034);
      const inner = card(ctx, r, t, { title: cfg.title || '时钟', pad });

      // 右上角：天气图标 + 当前温度（比原来的 meta 小字大得多）
      if (cfg.showWeather !== false && d && d.temp != null && inner.w > 300) {
        const tempSize = Math.max(14, Math.min(inner.h * 0.16, 34));
        const iconSize = tempSize * 2.1;
        const tempTxt = `${Math.round(d.temp)}°`;
        ctx.font = font(tempSize, 400, true);
        ctx.textAlign = 'right';
        ctx.textBaseline = 'top';
        const tw = ctx.measureText(tempTxt).width;
        const rightX = inner.x + inner.w;
        ctx.fillStyle = t.fg2;
        ctx.fillText(tempTxt, rightX, inner.headY - tempSize * 0.02);
        weatherIcon(ctx, rightX - tw - iconSize * 0.6, inner.headY + tempSize * 0.52, iconSize, d.code);
      }

      // 时分秒同一个字号，尽量大
      const hh = String(now.getHours()).padStart(2, '0');
      const mm = String(now.getMinutes()).padStart(2, '0');
      const ss = String(now.getSeconds()).padStart(2, '0');
      const time = `${hh}:${mm}:${ss}`;
      const style = cfg.font || 'mono';
      if (style === 'lcd') {
        // 七段数码管：液晶味，不依赖系统字体
        const maxW = inner.w * 0.99;
        const maxH = Math.min(inner.h * 0.62, inner.w * 0.22);
        let h = maxH;
        while (h > 12 && sevenSegWidth(time, h) > maxW) h *= 0.94;
        const wSeg = sevenSegWidth(time, h);
        drawSevenSeg(ctx, time, inner.x + (inner.w - wSeg) / 2, inner.y + inner.h * 0.5 - h * 0.5, h, t.fg, {
          dim: 'rgba(255,255,255,0.05)',
        });
      } else {
        const size = fitFont(ctx, time, inner.w * 0.99, Math.min(inner.h * 0.52, inner.w * 0.2), 300, style === 'mono');
        ctx.fillStyle = t.fg;
        ctx.font = font(size, 300, style === 'mono');
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
        ctx.fillText(time, inner.x, inner.y + inner.h * 0.5);
      }

      // 底部：2026年11月8日   星期日   农历九月三十
      const dateSize = Math.max(11, Math.min(inner.h * 0.115, 22));
      const dateTxt = `${now.getFullYear()}年${now.getMonth() + 1}月${now.getDate()}日`;
      const weekday = '星期' + '日一二三四五六'[now.getDay()];
      let lunar = '';
      try {
        lunar = '农历' + solarToLunar(now.getFullYear(), now.getMonth() + 1, now.getDate()).text;
      } catch (e) {
        lunar = '';
      }
      let size2 = dateSize;
      ctx.font = font(size2, 400);
      const gap = size2 * 1.1;
      let total = ctx.measureText(dateTxt).width + ctx.measureText(weekday).width + gap;
      if (lunar) total += ctx.measureText(lunar).width + gap;
      if (total > inner.w) size2 = fitFont(ctx, `${dateTxt}  ${weekday}  ${lunar}`, inner.w, size2, 400);

      ctx.textAlign = 'left';
      ctx.textBaseline = 'bottom';
      const baseY = inner.y + inner.h * 0.99;
      let x = inner.x;
      ctx.fillStyle = t.fg3; // 日期/星期/农历统一一个灰度
      ctx.fillText(dateTxt, x, baseY);
      x += ctx.measureText(dateTxt).width + gap;
      ctx.fillStyle = t.fg3;
      ctx.fillText(weekday, x, baseY);
      x += ctx.measureText(weekday).width + gap;
      if (lunar) {
        ctx.fillStyle = t.fg3;
        ctx.fillText(lunar, x, baseY);
      }
    },
  },

  agent: {
    label: 'Agent',
    icon: '<rect x="2.5" y="3.5" width="11" height="9" rx="2"/><path d="M6 6.5v3M10 6.5v3"/>',
    min: [2, 2],
    // 有 running 会话时 1s，全部结束/等待时保持 3s
    needs: { kind: 'agents', ttl: (_cfg, data) => (data && data.running > 0 ? 1000 : 3000) },
    // config.hide：整组隐藏某个 agent；config.hideTasks：只隐藏某一条会话
    config: { hide: [], hideTasks: [] },
    draw(ctx, r, cfg, d, t) {
      const hide = new Set((cfg && cfg.hide) || []);
      const hideTasks = new Set((cfg && cfg.hideTasks) || []);
      const items = ((d && d.items) || []).filter((i) => !hide.has(i.agent) && !hideTasks.has(i.id));
      const running = items.filter((i) => i.state === 'running').length;
      const inner = card(ctx, r, t, {
        title: cfg.title || 'Agent',
        meta: items.length ? (running ? `${running} 运行中` : `${items.length} 会话`) : '',
        pad: Math.max(10, r.w * 0.03),
      });
      if (!items.length) {
        const total = (d && d.items && d.items.length) || 0;
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(13, inner.h * 0.14), 400);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText(
          d && d.error ? '扫描失败：' + d.error : total ? '已在配置里隐藏全部 Agent' : '没有检测到 Agent 会话',
          inner.x,
          inner.y
        );
        return;
      }

      // 状态：绿=运行中（呼吸）· 琥珀=等待确认（呼吸）· 红=1 小时内异常退出 · 蓝=1 小时内结束 · 灰=更早（含异常退出）
      const pulse = 0.45 + 0.55 * Math.abs(Math.sin((Date.now() % 1600) / 1600 * Math.PI));
      const stateOf = (it) => {
        if (it.state === 'running') return { color: '#3ddc84', label: '运行中', blink: true };
        if (it.state === 'waiting') return { color: '#fabe46', label: '等待', blink: true };
        if (it.state === 'error') {
          return { color: (it.age || 0) < 3600 ? '#ef6f6f' : '#6b7280', label: '异常退出', blink: false };
        }
        if (it.state === 'done' && (it.age || 0) < 3600) return { color: '#5aa9ff', label: '', blink: false };
        return { color: '#6b7280', label: '', blink: false };
      };
      const ageTxt = (secs) => {
        if (secs < 60) return `${secs}s`;
        if (secs < 3600) return `${Math.round(secs / 60)}m`;
        if (secs < 86400) return `${Math.round(secs / 3600)}h`;
        return `${Math.round(secs / 86400)}d`;
      };

      const row0 = Math.max(40, Math.min(inner.w * 0.085, 96));
      const { rows, rowH } = rowsFor(inner.h, row0, items.length);
      const logo = Math.max(16, Math.min(rowH * 0.62, 34));
      const dot = Math.max(5, Math.min(10, rowH * 0.16));
      const chipW = Math.max(52, rowH * 1.5);

      items.slice(0, rows).forEach((it, i) => {
        const y = inner.y + i * rowH;
        const cy = y + rowH / 2;
        const st = stateOf(it);
        const age = Number(it.age || 0);
        const isDone = it.state === 'done';
        const isBlue = isDone && age < 3600;
        const isGray = it.state === 'idle' || (isDone && age >= 3600) || (it.state === 'error' && age >= 3600);
        const alpha = st.blink ? pulse : isGray ? 0.45 : 1;

        // 状态点（运行中/等待确认会呼吸）
        ctx.globalAlpha = alpha;
        ctx.fillStyle = st.color;
        ctx.beginPath();
        ctx.arc(inner.x + dot / 2, cy, dot / 2, 0, Math.PI * 2);
        ctx.fill();

        // Agent logo（识别用，替代文字）
        const img = agentLogo(it.agent);
        const lx = inner.x + dot + 8;
        ctx.globalAlpha = isGray ? 0.4 : 1;
        if (img) {
          ctx.drawImage(img, lx, cy - logo / 2, logo, logo);
        } else {
          // 没有对应 logo 的 agent：色块 + 首字母，别留空白
          roundRect(ctx, lx, cy - logo / 2, logo, logo, 4);
          ctx.fillStyle = st.color;
          ctx.fill();
          ctx.fillStyle = '#141519';
          ctx.font = font(logo * 0.62, 700);
          ctx.textAlign = 'center';
          ctx.textBaseline = 'middle';
          ctx.fillText((it.short || it.name || 'A').slice(0, 1).toUpperCase(), lx + logo / 2, cy + 1);
        }
        ctx.globalAlpha = 1;

        // 标题
        const titleX = lx + logo + Math.max(8, rowH * 0.2);
        const titleW = inner.w - (titleX - inner.x) - chipW - 8;
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = isGray ? t.fg4 : it.state === 'done' || it.state === 'error' ? t.fg2 : t.fg;
        ctx.font = font(Math.max(14, Math.min(rowH * 0.36, 24)), 400);
        ctx.fillText(ellipsis(ctx, it.title || '', titleW), titleX, cy);

        // 右侧：运行中显示已运行时长；等待/异常显示状态；结束显示距今年龄
        ctx.textAlign = 'right';
        const runFor = it.runningFor == null ? NaN : Number(it.runningFor);
        const runningTxt = it.state === 'running' && Number.isFinite(runFor)
          ? `运行 ${ageTxt(Math.max(0, runFor))}` : '';
        const agoTxt = isBlue && age < 60 ? '刚刚' : ageTxt(age);
        const rightTxt = runningTxt || st.label || agoTxt;
        const rightIsLabel = Boolean(runningTxt || st.label);
        ctx.font = font(Math.max(12, Math.min(rowH * 0.3, 19)), rightIsLabel ? 500 : 400, !rightIsLabel);
        ctx.globalAlpha = st.blink ? pulse : 1;
        ctx.fillStyle = st.color;
        ctx.fillText(rightTxt, inner.x + inner.w, cy);
        ctx.globalAlpha = 1;
      });
    },
  },

  system: {
    label: '电脑',
    icon: '<rect x="2" y="3" width="12" height="8" rx="1.5"/><path d="M5.5 13.5h5"/>',
    min: [2, 1],
    needs: { kind: 'system', ttl: 1000 },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.035);
      const cpu = d && typeof d.cpu === 'number' ? d.cpu : 0;
      const mem = d && typeof d.mem === 'number' ? d.mem : 0;
      const total = (d && d.memTotal) || 0;
      const inner = card(ctx, r, t, { title: cfg.title || '电脑', meta: (d && d.host) || '', pad });
      const rows = [
        ['CPU', `${Math.round(cpu * 100)}%`, cpu, cpu > 0.85 ? t.up : t.accent],
        ['MEM', total ? `${mem.toFixed(1)}G / ${total.toFixed(0)}G` : `${mem.toFixed(1)}G`, total ? mem / total : 0, t.fg3],
      ];
      const rowH = inner.h / rows.length;
      const labelSize = Math.max(11, Math.min(inner.h * 0.14, 18));
      rows.forEach(([name, val, frac, color], i) => {
        const y = inner.y + i * rowH;
        ctx.textBaseline = 'top';
        ctx.textAlign = 'left';
        ctx.font = font(labelSize, 500);
        ctx.fillStyle = t.fg4;
        ctx.fillText(name, inner.x, y);
        ctx.textAlign = 'right';
        ctx.fillStyle = t.fg2;
        ctx.font = font(labelSize, 400, true);
        ctx.fillText(val, inner.x + inner.w, y);
        const barH = Math.max(4, Math.min(8, rowH * 0.16));
        bar(ctx, inner.x, y + labelSize + rowH * 0.2, inner.w, barH, frac, color, t);
      });
    },
  },

  weather: {
    label: '天气',
    icon: '<path d="M5 11.5a2.5 2.5 0 0 1 .3-5 3.5 3.5 0 0 1 6.6 1A2 2 0 0 1 12 11.5"/><circle cx="5.5" cy="4.5" r="1.6"/>',
    min: [1, 1],
    needs: { kind: 'weather', ttl: 600000, query: (cfg) => ({ city: cfg.city || '北京' }) },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.06);
      const inner = card(ctx, r, t, {
        title: cfg.title || '天气',
        meta: d && d.humidity != null ? `湿度 ${Math.round(d.humidity)}%` : (d && d.city) || cfg.city || '',
        pad,
      });
      if (!d || d.temp == null) {
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(13, inner.h * 0.16), 400);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText(d && d.error ? '暂无数据' : '加载中…', inner.x, inner.y);
        return;
      }
      ctx.textBaseline = 'top';
      ctx.textAlign = 'left';
      ctx.fillStyle = t.fg;
      const size = Math.min(inner.h * 0.5, inner.w * 0.42);
      ctx.font = font(size, 300);
      ctx.fillText(`${Math.round(d.temp)}°`, inner.x, inner.y + inner.h * 0.12);
      ctx.fillStyle = t.fg3;
      ctx.font = font(Math.max(11, inner.h * 0.14), 400);
      const range = [d.low != null ? `↓${Math.round(d.low)}°` : '', d.high != null ? `↑${Math.round(d.high)}°` : '']
        .filter(Boolean)
        .join('   ');
      if (range) ctx.fillText(range, inner.x, inner.y + inner.h * 0.74);
    },
  },

  stock: {
    label: '股票',
    icon: '<path d="M2.5 11.5l3.5-3.5 2.5 2 4-4.5"/><path d="M10 5.5h3v3"/>',
    min: [2, 1],
    config: {
      symbols: 'sh000001',              // A 股盘中
      us: '100.SPX,100.NDX',            // 美股盘中（标普500 / 纳斯达克100）
      usFut: '103.ES00Y,103.NQ00Y',     // 其余时间看这两个期货连续合约
    },
    needs: {
      kind: 'portfolio',
      // 期货也是连续交易，价格每秒都在动；60 秒会让屏上看着像收盘了。
      // 统一 15 秒（host 侧 portfolio 缓存 10 秒，能真的拿到新价）。
      ttl: 15000,
      // A 股午休不轮询；有旧数据时 core.js 直接跳过，13:00 后恢复
      when: stockPolling,
      query: (cfg) => ({ symbols: stockSymbols(cfg) }),
    },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(8, r.w * 0.025);
      const stamp = stockStamp(d, stockPolling());
      const session = stockSession();
      const inner = card(ctx, r, t, {
        title: cfg.title || (cfg.us || cfg.usFut ? session.label : '股票'),
        meta: stamp ? stamp.text : '',
        metaColor: stamp ? stamp.color : '',
        pad,
      });
      const items = (d && d.items) || [];
      if (!items.length) {
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(12, inner.h * 0.14), 400);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText(d && d.error ? '暂无数据' : '加载中…', inner.x, inner.y);
        return;
      }
      // 一行一只股票：一格高 = 一只（2 行就 2 只），高度不够时才减条不压内容
      const CELLH = 160; // 与 core.js 的 CELL 一致：一格 160 设备像素
      const gap = Math.max(4, inner.h * 0.02);
      const fit = Math.floor((inner.h + gap) / (96 + gap)); // 96 = 一行股票的最小高度
      const perCell = Math.max(1, Math.round(r.h / CELLH));
      const rows = Math.max(1, Math.min(items.length, fit, perCell));
      const rowH = Math.min(200, (inner.h - gap * (rows - 1)) / rows);
      // 盘中：分时图按整段时间轴画，只走到当前时刻，右边留白（一眼看出还在走）
      // 期货是 23 小时连续交易（07:00–次日 06:00），少了 fut 就会把 14 小时拉满整宽，
      // 看起来像「已经收盘/顶到头」。
      const slots = { cn: 241, us: 391, fut: 1380 }[session.id];
      for (let i = 0; i < rows; i++) {
        stockRow(ctx, { x: inner.x, y: inner.y + i * (rowH + gap), w: inner.w, h: rowH }, items[i], t, {
          card: false,
          slots,
        });
      }
    },
  },

  kline: {
    label: 'K 线',
    icon: '<path d="M4 3v10M4 6h3M4 9h3M10 4v9M10 6h3M10 10h3"/>',
    min: [2, 1],
    needs: { kind: 'kline', ttl: 120000, when: tradingHours, query: (cfg) => ({ symbol: cfg.symbol || 'sh000001' }) },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.03);
      const inner = card(ctx, r, t, {
        title: (d && d.name) || cfg.title || cfg.symbol || 'K 线',
        meta: d && typeof d.pct === 'number' ? `${signed(d.pct)}%` : '',
        metaColor: d && typeof d.pct === 'number' ? pctColor(d.pct, t) : '',
        pad,
      });
      if (!d || !d.candles || !d.candles.length) {
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(13, inner.h * 0.16), 400);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText(d && d.error ? '暂无数据' : '加载中…', inner.x, inner.y);
        return;
      }
      const candles = d.candles.slice(-48);
      let hi = -Infinity;
      let lo = Infinity;
      for (const c of candles) {
        hi = Math.max(hi, c.h);
        lo = Math.min(lo, c.l);
      }
      const span = hi - lo || 1;
      const w = inner.w / candles.length;
      const y = (v) => inner.y + inner.h - ((v - lo) / span) * inner.h;
      candles.forEach((c, i) => {
        const x = inner.x + i * w + w / 2;
        const col = c.c >= c.o ? t.up : t.down;
        ctx.strokeStyle = col;
        ctx.fillStyle = col;
        ctx.lineWidth = Math.max(1, w * 0.16);
        ctx.beginPath();
        ctx.moveTo(x, y(c.h));
        ctx.lineTo(x, y(c.l));
        ctx.stroke();
        const bw = Math.max(1.5, w * 0.55);
        const y1 = y(Math.max(c.o, c.c));
        const y2 = y(Math.min(c.o, c.c));
        ctx.fillRect(x - bw / 2, y1, bw, Math.max(1.2, y2 - y1));
      });
    },
  },

  crypto: {
    label: '加密',
    icon: '<circle cx="8" cy="8" r="6"/><path d="M6.2 5.5h2.6a1.7 1.7 0 0 1 0 3.4H6.2m0 0h2.9a1.7 1.7 0 0 1 0 3.4H6.2M7.4 4v8.6"/>',
    min: [1, 1],
    needs: { kind: 'crypto', ttl: 60000, query: (cfg) => ({ ids: cfg.ids || 'bitcoin,ethereum' }) },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.06);
      const inner = card(ctx, r, t, { title: cfg.title || '加密', meta: '', pad });
      if (!d || !d.coins) {
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(13, inner.h * 0.16), 400);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText(d && d.error ? '暂无数据' : '加载中…', inner.x, inner.y);
        return;
      }
      const { rows, rowH } = rowsFor(inner.h, Math.max(34, Math.min(inner.w * 0.055, 56)), d.coins.length);
      d.coins.slice(0, rows).forEach((c, i) => {
        const yy = inner.y + i * rowH + rowH * 0.12;
        const price = Number(c.price);
        ctx.textBaseline = 'top';
        ctx.textAlign = 'left';
        ctx.font = font(Math.max(11, Math.min(rowH * 0.32, 17)), 500, true);
        ctx.fillStyle = t.fg3;
        ctx.fillText(c.symbol, inner.x, yy);
        ctx.textAlign = 'right';
        ctx.fillStyle = t.fg2;
        ctx.font = font(Math.max(11, Math.min(rowH * 0.32, 17)), 400, true);
        ctx.fillText(`$${price >= 1000 ? price.toFixed(0) : price.toFixed(price < 10 ? 3 : 2)}`, inner.x + inner.w, yy);
        ctx.fillStyle = pctColor(c.pct, t);
        ctx.font = font(Math.max(10, Math.min(rowH * 0.26, 14)), 500, true);
        ctx.fillText(signed(c.pct), inner.x + inner.w, yy + rowH * 0.44);
      });
    },
  },

  github: {
    label: 'GitHub',
    icon: '<path d="M8 1.8a6.2 6.2 0 0 0-2 12.1c.3.1.4-.1.4-.3v-1.1c-1.7.4-2.1-.8-2.1-.8-.3-.7-.7-.9-.7-.9-.6-.4 0-.4 0-.4.6 0 1 .7 1 .7.6 1 1.5.7 1.9.5 0-.4.2-.7.4-.9-1.4-.2-2.8-.7-2.8-3.1 0-.7.2-1.2.6-1.7-.1-.2-.3-.8.1-1.7 0 0 .5-.2 1.7.6a5.9 5.9 0 0 1 3.1 0c1.2-.8 1.7-.6 1.7-.6.4.9.1 1.5.1 1.7.4.5.6 1 .6 1.7 0 2.4-1.4 2.9-2.8 3.1.2.2.4.6.4 1.1v1.6c0 .2.1.4.4.3A6.2 6.2 0 0 0 8 1.8z"/>',
    min: [1, 1],
    needs: { kind: 'github', ttl: 300000, query: (cfg) => ({ repo: cfg.repo || 'tauri-apps/tauri' }) },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.06);
      const inner = card(ctx, r, t, { title: cfg.title || 'GitHub', meta: d && d.pushed ? ago(d.pushed) : '', pad });
      if (!d || d.stars == null) {
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(13, inner.h * 0.16), 400);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText(d && d.error ? '暂无数据' : '加载中…', inner.x, inner.y);
        return;
      }
      ctx.textBaseline = 'top';
      ctx.textAlign = 'left';
      ctx.fillStyle = t.fg;
      ctx.font = font(Math.max(12, Math.min(inner.h * 0.2, 22)), 500);
      ctx.fillText(ellipsis(ctx, d.repo || cfg.repo, inner.w), inner.x, inner.y);
      ctx.fillStyle = t.accent;
      ctx.font = font(Math.max(13, Math.min(inner.h * 0.3, 34)), 300, true);
      ctx.fillText(`★ ${d.stars}`, inner.x, inner.y + inner.h * 0.3);
      ctx.fillStyle = t.fg4;
      ctx.font = font(Math.max(10, inner.h * 0.13), 400, true);
      ctx.textAlign = 'left';
      ctx.fillText(`${d.lang || ''} · issue ${d.issues}`, inner.x, inner.y + inner.h * 0.78);
    },
  },

  music: {
    label: '音乐',
    icon: '<path d="M6 11.5V4l6-1.5v7.5"/><circle cx="4.5" cy="12" r="1.8"/><circle cx="10.5" cy="10.5" r="1.8"/>',
    min: [1, 1],
    needs: { kind: 'music', ttl: 5000 },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.06);
      const playing = !!(d && d.playing);
      const inner = card(ctx, r, t, { title: cfg.title || '正在播放', meta: playing ? d.app : '', pad });
      ctx.textBaseline = 'top';
      ctx.textAlign = 'left';
      ctx.fillStyle = playing ? t.fg : t.fg4;
      ctx.font = font(Math.max(12, Math.min(inner.h * 0.22, 24)), 500);
      const title = playing ? d.title : d && d.error ? '需要授权' : '未在播放';
      ctx.fillText(ellipsis(ctx, title, inner.w), inner.x, inner.y + inner.h * 0.14);
      ctx.fillStyle = t.fg4;
      ctx.font = font(Math.max(10, inner.h * 0.14), 400);
      ctx.fillText(ellipsis(ctx, (playing && d.artist) || '', inner.w), inner.x, inner.y + inner.h * 0.62);
      if (playing) {
        ctx.fillStyle = t.accent;
        ctx.font = font(Math.max(12, inner.h * 0.18));
        ctx.textAlign = 'right';
        ctx.fillText('♫', inner.x + inner.w, inner.y + inner.h * 0.6);
      }
    },
  },

  timer: {
    label: '倒计时',
    icon: '<circle cx="8" cy="9" r="5"/><path d="M8 6.5V9l1.8 1.2M6.5 2h3"/>',
    min: [1, 1],
    config: { label: '下班', time: '18:00' },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.06);
      const inner = card(ctx, r, t, { title: cfg.label || '倒计时', meta: cfg.time || '', pad });
      const m = /^(\d{1,2}):(\d{2})$/.exec(String(cfg.time || '18:00'));
      const now = new Date();
      const target = new Date(now);
      if (m) {
        target.setHours(Number(m[1]), Number(m[2]), 0, 0);
        if (target < now) target.setDate(target.getDate() + 1);
      }
      const left = Math.max(0, Math.floor((target - now) / 1000));
      const txt = [left / 3600, (left / 60) % 60, left % 60]
        .map((v) => String(Math.floor(v)).padStart(2, '0'))
        .join(':');
      ctx.textBaseline = 'top';
      ctx.textAlign = 'left';
      ctx.fillStyle = t.fg;
      const size = fitFont(ctx, txt, inner.w, Math.min(inner.h * 0.5, inner.w * 0.34), 300, true);
      ctx.font = font(size, 300, true);
      ctx.fillText(txt, inner.x, inner.y + inner.h * 0.2);
    },
  },

  note: {
    label: '便签',
    icon: '<path d="M3 2.5h10v8l-3.5 3H3z"/><path d="M9.5 13.5v-3h3"/>',
    min: [1, 1],
    config: { text: '写点什么' },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.05);
      const inner = card(ctx, r, t, { title: cfg.title || '便签', meta: '', pad });
      const size = Math.max(12, Math.min(inner.h * 0.2, 34));
      ctx.font = font(size, 400);
      ctx.textAlign = 'left';
      ctx.textBaseline = 'top';
      ctx.fillStyle = t.fg2;
      let line = '';
      let y = inner.y;
      for (const ch of String(cfg.text || '')) {
        if (ctx.measureText(line + ch).width > inner.w) {
          ctx.fillText(line, inner.x, y);
          line = ch;
          y += size * 1.35;
        } else line += ch;
        if (y > inner.y + inner.h - size) return;
      }
      if (line) ctx.fillText(line, inner.x, y);
    },
  },
  // 表盘时钟：刻度圈 + 左右大数字 + 中央指针 + 底部天气（参考用户给的截图）
  clock_dial: {
    label: '时钟 · 表盘',
    icon: '<circle cx="8" cy="8" r="6"/><path d="M8 4.5V8l2.5 1.5"/><path d="M8 1.6v1M8 13.4v1M1.6 8h1M13.4 8h1"/>',
    min: [3, 2],
    config: { city: '北京', showWeather: true, customTitle: true },
    needs: { kind: 'weather', ttl: 600000, query: (cfg) => ({ city: cfg.city || '北京' }) },
    draw(ctx, r, cfg, d, t) {
      const now = new Date();
      const pad = Math.max(8, r.w * 0.02);
      const showTitle = cfg.title;
      const inner = card(ctx, r, t, { title: showTitle || '', pad });

      const box = { x: r.x + 3, y: r.y + 3, w: r.w - 6, h: r.h - 6 };
      const cx = box.x + box.w / 2;
      const cy = box.y + box.h * (showTitle ? 0.56 : 0.52);

      // 刻度：每 6° 一根，沿「圆心 → 边界」的射线落点，所以 12/3/6/9 正好在上/右/下/左正中
      const inset = Math.min(box.w, box.h) * 0.028;
      const tickBase = Math.min(box.w, box.h);
      const gw = box.w - inset * 2;
      const gh = box.h - inset * 2;
      const cornerR = Math.min(box.h * 0.1, gw / 2, gh / 2);
      // 刻度圈的矩形是 box 居中，表盘中心 cy 可能因为标题下移，射线起点按偏移算
      const ox = cx - (box.x + box.w / 2);
      const oy = cy - (box.y + box.h / 2);
      ctx.save();
      ctx.lineCap = 'round';
      for (let i = 0; i < 60; i++) {
        const major = i % 5 === 0;
        const ang = (i / 60) * Math.PI * 2 - Math.PI / 2; // 0° = 12 点
        const edge = rrEdgeDistance(ox, oy, ang, gw, gh, cornerR);
        const ux = Math.cos(ang);
        const uy = Math.sin(ang);
        const px = cx + ux * edge;
        const py = cy + uy * edge;
        const len = tickBase * (major ? 0.075 : 0.04);
        ctx.strokeStyle = major ? 'rgba(255,255,255,0.8)' : 'rgba(255,255,255,0.26)';
        ctx.lineWidth = Math.max(1.2, tickBase * (major ? 0.01 : 0.0045));
        ctx.beginPath();
        ctx.moveTo(px, py);
        ctx.lineTo(px - ux * len, py - uy * len); // 朝圆心
        ctx.stroke();
      }
      ctx.restore();

      // HH : MM（中间留指针位）
      const hh = String(now.getHours()).padStart(2, '0');
      const mm = String(now.getMinutes()).padStart(2, '0');
      const numSize = Math.min(box.h * 0.36, box.w * 0.19);
      const handGap = numSize * 0.62;

      // 日期 + 星期（星期用 accent）
      const dateSize = Math.max(11, Math.min(box.h * 0.11, 26));
      const dateTxt = `${now.getMonth() + 1}月 ${now.getDate()}日`;
      const weekday = '周' + '日一二三四五六'[now.getDay()];
      ctx.font = font(dateSize, 500);
      ctx.textBaseline = 'middle';
      const dGap = dateSize * 0.9;
      const dW = ctx.measureText(dateTxt).width + ctx.measureText(weekday).width + dGap;
      const dY = box.y + box.h * (showTitle ? 0.24 : 0.2);
      ctx.textAlign = 'left';
      ctx.fillStyle = t.fg2;
      ctx.fillText(dateTxt, cx - dW / 2, dY);
      ctx.fillStyle = t.accent;
      ctx.fillText(weekday, cx - dW / 2 + ctx.measureText(dateTxt).width + dGap, dY);


      // 指针（先画，数字盖在上面）：分针比时针长出一截，秒针最长，长度按卡片取固定比例
      const minLen = Math.min(box.w * 0.26, box.h * 0.33);
      const hourLen = minLen * 0.58;
      const toRad = (deg) => ((deg - 90) * Math.PI) / 180;
      const hourAng = toRad(((now.getHours() % 12) + now.getMinutes() / 60) * 30);
      const minAng = toRad(now.getMinutes() * 6);
      // 秒针整秒跳；10 Hz 刷新只用于 RunCat/Agent，秒针一秒走一格。
      const secAng = toRad(now.getSeconds() * 6);
      const thick = Math.max(2.5, numSize * 0.075);
      ctx.strokeStyle = t.fg;
      ctx.lineCap = 'round';
      ctx.lineWidth = thick;
      [
        [hourAng, hourLen],
        [minAng, minLen],
      ].forEach(([a, len]) => {
        ctx.beginPath();
        ctx.moveTo(cx - Math.cos(a) * len * 0.18, cy - Math.sin(a) * len * 0.18);
        ctx.lineTo(cx + Math.cos(a) * len, cy + Math.sin(a) * len);
        ctx.stroke();
      });
      // 秒针：一根细长的 accent 线，横贯中心
      ctx.strokeStyle = t.accent;
      ctx.lineWidth = Math.max(1.4, thick * 0.32);
      const secLen = Math.min(box.w * 0.42, box.h * 0.42);
      ctx.beginPath();
      ctx.moveTo(cx - Math.cos(secAng) * secLen * 0.18, cy - Math.sin(secAng) * secLen * 0.18);
      ctx.lineTo(cx + Math.cos(secAng) * secLen, cy + Math.sin(secAng) * secLen);
      ctx.stroke();

      // 数字（盖在指针之上，保证可读）
      ctx.font = font(numSize, 700);
      ctx.fillStyle = t.fg;
      ctx.textBaseline = 'middle';
      ctx.textAlign = 'right';
      ctx.fillText(hh, cx - handGap, cy);
      ctx.textAlign = 'left';
      ctx.fillText(mm, cx + handGap, cy);

      // 中心轴
      ctx.fillStyle = box.bg || t.card;
      ctx.beginPath();
      ctx.arc(cx, cy, thick * 1.35, 0, Math.PI * 2);
      ctx.fill();
      ctx.strokeStyle = t.accent;
      ctx.lineWidth = Math.max(1.6, thick * 0.45);
      ctx.beginPath();
      ctx.arc(cx, cy, thick * 1.05, 0, Math.PI * 2);
      ctx.stroke();
      ctx.fillStyle = t.accent;
      ctx.beginPath();
      ctx.arc(cx, cy, thick * 0.45, 0, Math.PI * 2);
      ctx.fill();


      // 底部：天气图标 + 晴 4°
      if (cfg.showWeather !== false && d && d.temp != null) {
        const iconSize = Math.min(box.h * 0.2, 46);
        const txtSize = Math.max(11, Math.min(box.h * 0.14, 26));
        const label = `${WEATHER_TEXT(d.code)} ${Math.round(d.temp)}°`;
        ctx.font = font(txtSize, 500);
        const tw = ctx.measureText(label).width;
        const gap = iconSize * 0.35;
        const totalW = iconSize + gap + tw;
        const bx = cx - totalW / 2;
        const by = box.y + box.h * (showTitle ? 0.86 : 0.86);
        weatherIcon(ctx, bx + iconSize / 2, by, iconSize, d.code);
        ctx.fillStyle = t.fg2;
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
        ctx.fillText(label, bx + iconSize + gap, by);
      }
    },
  },

  // 2×2 方形：CPU 外环 / MEM 内环，猫在内环里跑，缺口放读数 + 网速
  runcat: {
    label: 'RunCat',
    icon: '<path d="M2.5 9.5h2l1.5-3 1.5 5 2-6 1.5 4h2.5"/>',
    min: [2, 2],
    needs: { kind: 'system', ttl: 1000 },
    draw(ctx, r, cfg, d, t) {
      const cpu = d && typeof d.cpu === 'number' ? Math.min(1, Math.max(0, d.cpu)) : 0;
      const mem = d && d.memTotal ? Math.min(1, Math.max(0, d.mem / d.memTotal)) : 0;
      const pad = Math.max(10, r.w * 0.04);
      // 右上角显示局域网 IP（拿不到就退到主机名）；cfg.meta = 'host' 可以强制显示主机名
      const meta = cfg.meta === 'host' ? (d && d.host) : ((d && (d.ip || d.host)) || '');
      const inner = card(ctx, r, t, { title: cfg.title || 'RunCat', meta, pad });

      // 底部固定留一条读数带（数字 + 网速），圆环只在上面那块里画
      const textBand = Math.max(44, inner.h * 0.26);
      const rOut = Math.min(inner.w * 0.42, (inner.h - textBand) * 0.47);
      const lw = Math.max(9, rOut * 0.14); // 两环同粗细
      const rIn = rOut - lw * 2;
      const cx = inner.x + inner.w / 2;
      const cy = inner.y + rOut + lw / 2 + 2;
      gauge(ctx, cx, cy, rOut, lw, cpu, cpu > 0.85 ? t.up : RUNCAT.cpu);
      gauge(ctx, cx, cy, rIn, lw, mem, RUNCAT.mem);

      drawCat(ctx, cx, cy, rIn * 1.72, t.fg, catFrame(cpu), 'center');

      // 读数：左列 CPU + 上传，右列 MEM + 下载（和环一一对齐）
      const big = Math.max(12, Math.min(inner.w * 0.058, 21));
      const small = Math.max(10, big * 0.78);
      const colGap = big * 2.2;
      const yTop = inner.y + inner.h - textBand * 0.66;
      const yBottom = inner.y + inner.h - textBand * 0.16;
      ctx.textBaseline = 'middle';

      ctx.textAlign = 'right';
      ctx.font = font(big, 500, true);
      ctx.fillStyle = cpu > 0.85 ? t.up : RUNCAT.cpu;
      ctx.fillText(`CPU ${Math.round(cpu * 100)}%`, cx - colGap / 2, yTop);
      ctx.font = font(small, 400, true);
      ctx.fillStyle = t.fg3; // 和其它次要文字同灰度，别太暗
      ctx.fillText(`↑ ${fmtRate(d && d.netUp)}`, cx - colGap / 2, yBottom);

      ctx.textAlign = 'left';
      ctx.font = font(big, 500, true);
      ctx.fillStyle = RUNCAT.mem;
      ctx.fillText(`MEM ${Math.round(mem * 100)}%`, cx + colGap / 2, yTop);
      ctx.font = font(small, 400, true);
      ctx.fillStyle = t.fg3;
      ctx.fillText(`↓ ${fmtRate(d && d.netDown)}`, cx + colGap / 2, yBottom);
    },
  },

  // 5×2 长条：左輺道跑猫，右 CPU/MEM 两行大数字 + 折线，左下网速
  runcat_bar: {
    label: 'RunCat 条',
    icon: '<path d="M2.5 10.5h11"/><path d="M4 10.5V8l2 1 2-4 2 5 2-3v3.5"/>',
    min: [5, 2],
    needs: { kind: 'system', ttl: 1000 },
    draw(ctx, r, cfg, d, t) {
      const cpu = d && typeof d.cpu === 'number' ? Math.min(1, Math.max(0, d.cpu)) : 0;
      const mem = d && d.memTotal ? Math.min(1, Math.max(0, d.mem / d.memTotal)) : 0;
      const pad = Math.max(12, r.w * 0.018);
      const inner = card(ctx, r, t, { title: cfg.title || 'RunCat', meta: (d && d.host) || '', pad });
      sampleHist(cpu, mem);

      // 左：輺道 + 猫
      const laneW = Math.min(inner.w * 0.38, inner.h * 1.1);
      // 跑道线放在左栏垂直中线偏下；猫按"可用的上方空间"定尺寸，别压到跑道线下的网速文字
      const baseY = inner.y + inner.h * 0.6;
      const catSize = Math.min(laneW * 0.98, (baseY - inner.y) / (CAT_AR * 0.78));
      ctx.strokeStyle = 'rgba(255,255,255,0.08)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(inner.x, Math.round(baseY) + 0.5);
      ctx.lineTo(inner.x + laneW, Math.round(baseY) + 0.5);
      ctx.stroke();
      drawCat(ctx, inner.x + laneW / 2, baseY, catSize, t.fg, catFrame(cpu), 'feet');
      ctx.textAlign = 'center';
      ctx.textBaseline = 'top';
      ctx.font = font(Math.max(11, Math.min(laneW * 0.07, 18)), 400, true);
      ctx.fillStyle = t.fg3;
      ctx.fillText(netText(d), inner.x + laneW / 2, baseY + Math.max(10, inner.h * 0.055));

      // 右：两行大数字 + 折线
      const x0 = inner.x + laneW + Math.max(18, inner.w * 0.028);
      const colW = inner.x + inner.w - x0;
      const numW = colW * 0.42;
      const rows = [
        ['CPU', `${Math.round(cpu * 100)}%`, hist.cpu, cpu > 0.85 ? t.up : RUNCAT.cpu],
        [`MEM${d && d.memTotal ? ' ' + Math.round(d.memTotal) + 'G' : ''}`, d && d.mem != null ? `${Number(d.mem).toFixed(1)}G` : '--', hist.mem, RUNCAT.mem],
      ];
      const rowH = inner.h / rows.length;
      rows.forEach(([name, val, series, color], i) => {
        const y = inner.y + i * rowH;
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.font = font(Math.max(11, Math.min(rowH * 0.12, 20)), 500);
        ctx.fillStyle = t.fg4;
        ctx.fillText(name, x0, y + rowH * 0.18);

        ctx.fillStyle = t.fg;
        ctx.font = font(fitFont(ctx, val, numW, Math.min(rowH * 0.42, 84), 300, true), 300, true);
        ctx.fillText(val, x0, y + rowH * 0.4);

        const sx = x0 + numW + Math.max(12, colW * 0.03);
        const sh = Math.min(rowH * 0.44, 60);
        sparkline(ctx, sx, y + rowH * 0.3, inner.x + inner.w - sx, sh, series, color);
      });
    },
  },

  // 热榜资讯：顶部 Tab = 数据源（用户勾选哪几个就轮播哪几个），行数由组件高度决定
  news: {
    label: '资讯',
    icon: '<path d="M3 3.5h10v9H3z"/><path d="M5 6h6M5 8.5h5M5 11h3"/>',
    min: [3, 2],
    config: { sources: ['toutiao', 'baidu', 'zhihu', 'weibo'], rotate: 60, labels: {} },
    needs: { kind: 'news', ttl: 180000, query: (cfg) => ({ sources: newsSources(cfg).join(',') }) },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.028);
      const inner = card(ctx, r, t, { pad });
      const ids = newsSources(cfg);
      const hint = (txt) => {
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(13, Math.min(inner.h * 0.09, 20)), 400);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText(txt, inner.x, inner.y + inner.h * 0.44);
      };
      if (!ids.length) return hint('未选择数据源');

      // Tab 条：太宽就缩字体，再宽就把放不下的 Tab 截掉（不换行、不滚动）
      let tabFont = Math.max(13, Math.min(inner.h * 0.1, inner.w * 0.034, 24));
      const stripW = () => {
        ctx.font = font(tabFont, 500);
        return ids.reduce((s, id) => s + ctx.measureText(newsLabel(cfg, id)).width + tabFont * 1.5, 0);
      };
      while (stripW() > inner.w && tabFont > 11) tabFont *= 0.94;
      const tabH = tabFont * 1.9;
      const active = Math.floor(Date.now() / (Math.max(5, Number(cfg.rotate) || 60) * 1000)) % ids.length;
      ctx.textBaseline = 'middle';
      ctx.font = font(tabFont, 500);
      const gap = tabFont * 0.3;
      const tabs = ids.map((id) => ({ id, w: ctx.measureText(newsLabel(cfg, id)).width + tabFont * 1.2 }));
      const total = tabs.reduce((sum, t) => sum + t.w + gap, 0) - gap;
      // Tab 太多放不下时，只画「以当前 Tab 为中心」能放下的那一段，保证高亮的那个永远可见
      let a = 0;
      let b = tabs.length - 1;
      let used = total;
      if (total > inner.w) {
        a = b = active;
        used = tabs[active].w;
        let l = active - 1;
        let r = active + 1;
        for (;;) {
          if (r < tabs.length && used + tabs[r].w + gap <= inner.w) {
            used += tabs[r].w + gap;
            b = r;
            r++;
          } else if (l >= 0 && used + tabs[l].w + gap <= inner.w) {
            used += tabs[l].w + gap;
            a = l;
            l--;
          } else break;
        }
      }
      let tx = inner.x;
      for (let i = a; i <= b; i++) {
        const { id, w } = tabs[i];
        if (i === active) {
          roundRect(ctx, tx, inner.y, w, tabH, tabH * 0.28);
          ctx.fillStyle = 'rgba(255,255,255,0.10)';
          ctx.fill();
          ctx.fillStyle = t.fg;
        } else {
          ctx.fillStyle = t.fg3;
        }
        ctx.textAlign = 'center';
        ctx.fillText(newsLabel(cfg, id), tx + w / 2, inner.y + tabH / 2 + 0.5);
        tx += w + gap;
      }

      const items = (d && d[ids[active]]) || [];
      // 右上角小字：多少秒/分钟前更新（拉取失败时变成提示，数据仍然是旧的）
      const stamp = freshStampAgo(d);
      if (stamp) {
        ctx.font = font(tabFont * 0.78, 500);
        if (tx + ctx.measureText(stamp.text).width + tabFont * 0.6 < inner.x + inner.w) {
          ctx.textAlign = 'right';
          ctx.textBaseline = 'middle';
          ctx.fillStyle = stamp.color || t.fg4;
          ctx.fillText(stamp.text, inner.x + inner.w, inner.y + tabH / 2);
        }
      }
      const listY = inner.y + tabH + tabFont * 0.5;
      const listH = inner.y + inner.h - listY;
      if (!items.length) {
        ctx.textBaseline = 'middle';
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(13, Math.min(inner.h * 0.08, 20)), 400);
        ctx.textAlign = 'left';
        ctx.fillText(!d ? '加载中…' : '暂无数据', inner.x, listY + Math.min(listH, 24) / 2);
        return;
      }

      // 一行一条：字号跟着宽度走，条数跟着高度走，行高再把剩余空间摊平
      const rowFont = Math.max(13, Math.min(inner.w * 0.028, 24));
      const rows = Math.max(1, Math.floor(listH / (rowFont * 1.9)));
      const rowH = Math.min(listH / rows, rowFont * 2.4);
      const rankW = rowFont * 1.6;
      ctx.font = font(rowFont * 0.88, 400, true);
      const hotW = Math.max(
        ...items.slice(0, rows).map((it) => (it.hot == null ? 0 : ctx.measureText(String(it.hot)).width))
      );
      const titleW = inner.w - rankW - hotW - rowFont * (hotW ? 1.4 : 0);

      items.slice(0, rows).forEach((it, i) => {
        const cy = listY + i * rowH + rowH / 2;
        ctx.textBaseline = 'middle';
        ctx.font = font(rowFont * 0.88, 600, true);
        ctx.fillStyle = t.fg4;
        ctx.textAlign = 'left';
        ctx.fillText(String(i + 1), inner.x, cy);

        ctx.font = font(rowFont, 400);
        ctx.fillStyle = t.fg2;
        ctx.fillText(ellipsis(ctx, it.title, Math.max(20, titleW)), inner.x + rankW, cy);

        if (it.hot != null && hotW) {
          ctx.font = font(rowFont * 0.88, 400, true);
          ctx.fillStyle = t.fg4;
          ctx.textAlign = 'right';
          ctx.fillText(String(it.hot), inner.x + inner.w, cy);
        }
      });
    },
  },

  // 假期倒计时：按宽高切四种版式（横版大数字+列表 / 竖版列表 / 横版单条 / 方形大数字）
  holiday: {
    label: '假期',
    icon: '<path d="M3 6h10v7.5H3z"/><path d="M3 9h10M6.2 3.5v3M9.8 3.5v3"/>',
    min: [1, 1],
    needs: { kind: 'holiday', ttl: 1800000 },
    draw(ctx, r, cfg, d, t) {
      const pad = Math.max(10, r.w * 0.05);
      const inner = card(ctx, r, t, { title: cfg.title || '', pad });
      const items = (d && d.upcoming) || [];
      if (!items.length) {
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(12, Math.min(inner.h * 0.12, 20)), 400);
        ctx.textAlign = 'left';
        ctx.textBaseline = 'top';
        ctx.fillText(!d ? '加载中…' : '暂无数据', inner.x, inner.y + inner.h * 0.05);
        return;
      }
      const next = items[0];
      const label = next.ongoing ? `${next.name}·假期中` : `${next.name}还有`;

      // 竖版：一假期一行，条数由高度决定
      if (inner.h > inner.w * 1.15) {
        const rowH = Math.max(66, Math.min(inner.w * 0.36, 120));
        const rows = Math.max(1, Math.min(items.length, Math.floor(inner.h / rowH)));
        const h = Math.min(rowH, inner.h / rows);
        items.slice(0, rows).forEach((it, i) => {
          dashed(ctx, inner.x, inner.y + i * h, inner.x + inner.w, inner.y + i * h);
          holidayRow(ctx, inner.x, inner.y + i * h, inner.w, h, it, t);
        });
        dashed(ctx, inner.x, inner.y + rows * h, inner.x + inner.w, inner.y + rows * h);
        return;
      }

      // 宽且够高：左边一个大的，右边列后面几个（与 4×2 / 5×2 一致）
      if (inner.w >= 520 && inner.h >= 210) {
        const leftW = inner.w * 0.44;
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = t.fg3;
        ctx.font = font(Math.max(12, Math.min(inner.h * 0.14, inner.w * 0.05)), 500);
        ctx.fillText(label, inner.x, inner.y + inner.h * 0.17);
        bigDays(ctx, inner.x, inner.y + inner.h * 0.62, leftW * 0.96, Math.min(inner.h * 0.34, leftW * 0.62),
          next.ongoing ? next.left : next.until, t, 'left');
        ctx.fillStyle = t.fg3;
        ctx.font = font(Math.max(11, Math.min(inner.h * 0.13, inner.w * 0.04)), 400);
        ctx.fillText(dateRange(next), inner.x, inner.y + inner.h * 0.86);

        const rest = items.slice(1);
        const x0 = inner.x + leftW + inner.w * 0.03;
        const wR = inner.x + inner.w - x0;
        const rowH = Math.max(56, Math.min(inner.h * 0.24, 96));
        const rows = Math.max(1, Math.min(rest.length, Math.floor(inner.h / rowH)));
        const h = Math.min(rowH, inner.h / rows);
        rest.slice(0, rows).forEach((it, i) => {
          dashed(ctx, x0, inner.y + i * h, x0 + wR, inner.y + i * h);
          holidayRow(ctx, x0, inner.y + i * h, wR, h, it, t);
        });
        dashed(ctx, x0, inner.y + rows * h, x0 + wR, inner.y + rows * h);
        return;
      }

      // 横版：名称/日期在左，大数字在右
      if (inner.w > inner.h * 1.25) {
        const nameSize = Math.max(13, Math.min(inner.h * 0.24, inner.w * 0.07));
        ctx.textAlign = 'left';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = t.fg2;
        ctx.font = font(nameSize, 500);
        ctx.fillText(next.name, inner.x, inner.y + inner.h * 0.34);
        ctx.fillStyle = t.fg3;
        ctx.font = font(Math.max(11, nameSize * 0.82), 400);
        ctx.fillText(dateRange(next), inner.x, inner.y + inner.h * 0.7);
        bigDays(ctx, inner.x + inner.w, inner.y + inner.h * 0.68, inner.w * 0.62,
          Math.min(inner.h * 0.5, inner.w * 0.3), next.ongoing ? next.left : next.until, t, 'right');
        return;
      }

      // 方形：名称在上，大数字在下（小尺寸不带「天」，跟参考图一致）
      const nameSize = Math.max(11, Math.min(inner.w * 0.17, inner.h * 0.16));
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillStyle = t.fg3;
      ctx.font = font(nameSize, 500);
      ctx.fillText(next.ongoing ? label : next.name, inner.x + inner.w / 2, inner.y + inner.h * 0.3);
      bigDays(ctx, inner.x + inner.w / 2, inner.y + inner.h * 0.72, inner.w * 0.9,
        Math.min(inner.w * 0.5, inner.h * 0.42), next.ongoing ? next.left : next.until, t, 'center', '');
    },
  },

  ha: {
    label: 'Home Assistant',
    icon: '<path d="M2.5 7.5 8 3l5.5 4.5V13a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1z"/><path d="M6.5 14V9.5h3V14"/>',
    min: [2, 1],
    config: {
      title: '家里',
      temp: '', humidity: '', air: '',
      climate: '', lights: '', switches: '', doors: '', lock: '',
      power: '', energy: '', media: '',
      chips: 'climate,lights,switches,media',
    },
    needs: { kind: 'ha', ttl: 2500, query: (cfg) => ({ entities: haEntityIds(cfg).join(',') }) },
    draw(ctx, r, cfg, d, t) {
      const items = (d && d.items) || {};
      const online = !!(d && d.online) && !d.stale;
      const pad = Math.max(12, r.w * 0.024);
      const inner = card(ctx, r, t, {
        title: cfg.title || '家里',
        meta: !d ? 'HA …' : online ? 'HA 在线' : 'HA 离线',
        metaColor: online ? t.fg4 : t.up,
        pad,
      });
      if (!Object.keys(items).length) {
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = t.fg4;
        ctx.font = font(Math.max(13, Math.min(20, inner.h * 0.12)), 400);
        const msg = d && d.error ? 'HA：' + d.error : 'Home Assistant 未配置';
        ctx.fillText(ellipsis(ctx, msg, inner.w * 0.92), inner.x + inner.w / 2, inner.y + inner.h / 2);
        return;
      }
      const get = (id) => (id && items[id]) || null;
      const idsOf = (v) => String(v || '').split(',').map((s) => s.trim()).filter(Boolean);

      const wide = inner.w > 460;
      const metricKeys = wide ? ['temp', 'humidity', 'air'] : ['temp', 'humidity'];
      const metricH = inner.h * (wide ? 0.5 : 0.54);
      const colW = inner.w / metricKeys.length;
      const labels = { temp: '室内温度', humidity: '室内湿度', air: '空气质量' };
      metricKeys.forEach((key, i) => {
        const x = inner.x + i * colW;
        const it = get(cfg[key]);
        let value = '—', unit = '', label = labels[key];
        if (it) {
          if (it.domain === 'weather') {
            const a = it.attributes || {};
            value = a.temperature != null ? String(a.temperature) : '—';
            unit = '°C';
            label = '室外 · ' + haText(it);
          } else {
            value = haText(it);
            unit = it.unit || '';
          }
        }
        haMetric(ctx, x, inner.y + 2, colW - 14, metricH, label, value, unit,
          it && it.state !== 'unavailable' && it.state !== 'unknown' ? t.fg : t.fg4, t);
        if (i) {
          ctx.strokeStyle = 'rgba(255,255,255,0.06)';
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(x - 7.5, inner.y + metricH * 0.12);
          ctx.lineTo(x - 7.5, inner.y + metricH * 0.9);
          ctx.stroke();
        }
      });

      const wanted = String(cfg.chips || 'climate,lights,switches,media').split(',').map((s) => s.trim()).filter(Boolean);
      const chips = [];
      for (const role of wanted) {
        if (role === 'climate') {
          const it = get(cfg.climate);
          if (!it) continue;
          const target = it.attributes && it.attributes.temperature != null ? ' · ' + it.attributes.temperature + '°' : '';
          chips.push({ label: '空调', text: haText(it) + target, color: it.state === 'off' ? t.fg4 : t.accent });
        } else if (role === 'lights' || role === 'switches') {
          const list = idsOf(cfg[role]).map(get).filter(Boolean);
          if (!list.length) continue;
          const on = list.filter((it) => it.state === 'on').length;
          const text = list.length === 1 ? haText(list[0])
            : on === 0 ? '全关' : on === list.length ? '全开' : on + '/' + list.length + ' 开';
          chips.push({ label: role === 'lights' ? '灯' : '电器', text, color: on ? t.accent : t.fg4 });
        } else if (role === 'doors') {
          const list = idsOf(cfg.doors).map(get).filter(Boolean);
          if (!list.length) continue;
          const open = list.filter((it) => it.state === 'on').length;
          chips.push({ label: '门窗', text: open ? open + ' 开' : '全关', color: open ? t.up : t.down });
        } else if (role === 'lock') {
          const it = get(cfg.lock);
          if (!it) continue;
          const bad = it.state !== 'locked';
          chips.push({ label: '门锁', text: haText(it), color: bad ? t.up : t.down });
        } else if (role === 'media') {
          const it = get(cfg.media);
          if (!it) continue;
          const title = it.attributes && it.attributes.media_title;
          const text = it.state === 'playing' && title ? title : haText(it);
          chips.push({ label: '音箱', text, color: it.state === 'playing' ? t.accent : t.fg4 });
        }
      }
      const chipY = inner.y + metricH + 8;
      const chipH = Math.max(36, Math.min(54, inner.h - metricH - 34));
      if (inner.h >= 170 && chips.length) {
        const gap = 8;
        const chipW = (inner.w - gap * (chips.length - 1)) / chips.length;
        chips.forEach((c, i) => haChip(ctx, inner.x + i * (chipW + gap), chipY, chipW, chipH, c.label, c.text, c.color, t));
      }

      const power = get(cfg.power), energy = get(cfg.energy);
      const foot = [];
      if (power) foot.push('⚡ 功率 ' + haText(power) + (power.unit ? ' ' + power.unit : ''));
      if (energy) foot.push('今日 ' + haText(energy) + (energy.unit ? ' ' + energy.unit : ''));
      const footY = inner.y + inner.h;
      ctx.textBaseline = 'bottom';
      ctx.textAlign = 'left';
      ctx.font = font(Math.max(11, Math.min(14, inner.h * 0.06)), 400, true);
      ctx.fillStyle = t.fg3;
      ctx.fillText(ellipsis(ctx, foot.join('   ') || 'Home Assistant', inner.w * 0.9), inner.x, footY);
    },
  },
};

/** 热榜数据源：id 与 host.py 的 NEWS 一一对应，顺序即 Tab 顺序 */
export const NEWS_TABS = [
  ['toutiao', '今日头条'],
  ['baidu', '百度'],
  ['zhihu', '知乎'],
  ['weibo', '微博'],
  ['tieba', '贴吧'],
  ['v2ex', 'V2EX'],
  ['juejin', '掘金'],
  ['sspai', '少数派'],
  ['ithome', 'IT之家'],
  ['36kr', '36氪'],
  ['wscn', '华尔街见闻'],
  ['hackernews', 'HN'],
];
export const NEWS_LABEL = Object.fromEntries(NEWS_TABS);

/** 勾选的数据源（按配置里的顺序；默认全开；未知 id 丢掉） */
export function newsSources(cfg) {
  const picked = cfg && Array.isArray(cfg.sources) ? cfg.sources : NEWS_TABS.map(([id]) => id);
  return picked.filter((id) => NEWS_LABEL[id]);
}

/** Tab 显示名：用户改过就用改的（cfg.labels），否则用默认名 */
export function newsLabel(cfg, id) {
  const custom = cfg && cfg.labels && cfg.labels[id];
  return (custom && String(custom).trim()) || NEWS_LABEL[id];
}
export { card, tracked, font, roundRect, fitFont, ellipsis, weatherIcon, weatherKind };
