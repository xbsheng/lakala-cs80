// 常量 + 状态 + API + 数据调度
import { THEME, WIDGETS } from './widgets.js';

export const COLS = 5;
export const ROWS = 8;
export const CELL = 160;
export const W = COLS * CELL;
export const H = ROWS * CELL;

export const state = {
  layout: null,
  data: {},          // widgetId -> 数据
  selected: null,    // widgetId
  zoom: 0.52,
  device: {},        // /api/status 的镜像
  session: null,
};

export async function api(path, opts) {
  const res = await fetch(path, opts);
  if (!res.ok) throw new Error(`${path} ${res.status}`);
  const type = res.headers.get('content-type') || '';
  return type.includes('json') ? res.json() : res;
}

export async function postBinary(path, body, timeout = 0) {
  const opts = { method: 'POST', headers: { 'Content-Type': 'application/octet-stream' }, body };
  // 没有超时的 fetch 会永远挂住 pusher.busy（host 被设备 ACK 卡住时就是这样），
  // 整页只能靠手刷。所有二进制推送都必须有上限。
  if (timeout) opts.signal = AbortSignal.timeout(timeout);
  return api(path, opts);
}

export async function postJson(path, obj) {
  return api(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(obj) });
}

export async function loadLayout() {
  const layout = await api('/api/layout');
  if (!layout.pages) layout.pages = [];
  // 设计 token 只来自 widgets.js（老布局里残留的 theme 字段直接忽略，避免旧配色回流）
  layout.theme = { ...THEME };
  state.layout = layout;
  // 先把上次落盘的数据铺进去：换面板/重开窗口立刻有内容，不用等网络
  state.data = { ...loadDataCache(), ...state.data };
  return state.layout;
}

// ---------------- 数据本地缓存 ----------------
// 目的：切面板、重开窗口时先用旧数据画出来（组件自带「HH:MM 更新」小字），
// 后台再按各组件 TTL 拉新的。只缓存能 JSON 化的数据，超大/异常的丢掉。
const DATA_CACHE = 'cs80:data:v1';
const DATA_CACHE_MAX = 200000; // 单个组件超过 200 KB 就不缓存

function loadDataCache() {
  try {
    const raw = JSON.parse(localStorage.getItem(DATA_CACHE) || '{}');
    return raw && typeof raw === 'object' ? raw : {};
  } catch (e) {
    return {};
  }
}

let cacheTimer = null;
function saveDataCache() {
  clearTimeout(cacheTimer);
  cacheTimer = setTimeout(() => {
    try {
      const live = new Set();
      for (const p of pages()) {
        if (p.type === 'custom') live.add(p.id);
        for (const w of p.widgets || []) live.add(w.id);
      }
      const out = {};
      for (const [id, d] of Object.entries(state.data)) {
        if (!live.has(id) || !d || typeof d !== 'object') continue;
        const text = JSON.stringify(d);
        if (text && text.length <= DATA_CACHE_MAX) out[id] = JSON.parse(text);
      }
      localStorage.setItem(DATA_CACHE, JSON.stringify(out));
    } catch (e) {
      /* 配额满了就当没缓存，不影响显示 */
    }
  }, 800);
}

export async function saveLayout() {
  await api('/api/layout', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(state.layout),
  });
}

export const pages = () => (state.layout ? state.layout.pages : []);
export const activePage = () => {
  if (!state.layout) return null;
  const find = (id) => state.layout.pages.find((p) => p.id === id);
  return find(state.layout.activePage) || state.layout.pages[0] || null;
};
export const gridPage = () => {
  const p = activePage();
  return p && p.type === 'grid' ? p : null;
};
export const widgetById = (id) => {
  const page = gridPage();
  return page ? page.widgets.find((w) => w.id === id) : null;
};

export async function setActivePage(id) {
  await postJson('/api/pages/active', { id });
  state.layout.activePage = id;
}

const moduleCache = new Map();
/** 自定义页面模块（pages/xxx.js），带缓存；加载失败返回 null */
export async function loadPageModule(page) {
  if (!page || page.type !== 'custom') return null;
  const key = page.module || 'pages/stock.js';
  if (moduleCache.has(key)) return moduleCache.get(key);
  try {
    const mod = await import('./' + key.replace(/^\.?\//, ''));
    moduleCache.set(key, mod.default || mod);
  } catch (e) {
    console.error('[page module]', key, e);
    moduleCache.set(key, null);
  }
  return moduleCache.get(key);
}

export function touchWidget(w) {
  if (!w) return;
  const def = WIDGETS[w.type] || {};
  w.config = w.config || {};
  for (const [k, v] of Object.entries(def.config || {})) if (w.config[k] === undefined) w.config[k] = v;
}

/** 给数据打上「何时取到的」/「这份是旧的」标记：拉失败时组件靠它显示小字，而不是把数据清空 */
function stamped(data) {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return data;
  return { ...data, fetchedAt: data.fetchedAt || Date.now(), stale: data.stale === true };
}

/** 拉取失败：保留上次的数据，只标 stale；一条都没拿到过才给 error 占位 */
function keepLast(key, err) {
  const prev = state.data[key];
  return prev && prev.fetchedAt ? { ...prev, stale: true } : { error: String(err), stale: true };
}

/** 数据调度：每个组件按自己的 needs.ttl 拉数据，只通知变化过的组件 */
export function startDataLoop(onChange) {
  const last = {};
  async function tick() {
    // 自定义页面：模块自己声明 needs，用页面 id 作为数据 key
    for (const page of pages()) {
      if (page.type !== 'custom') continue;
      const mod = await loadPageModule(page);
      const need = mod && mod.needs;
      if (!need) continue;
      const ttl = typeof need.ttl === 'function' ? need.ttl(page.config || {}, state.data[page.id]) : (need.ttl || 60000);
      const key = 'page:' + page.id;
      if (last[key] && Date.now() - last[key] < ttl) continue;
      // needs.when 说不刷（如休盘）且有旧数据 → 不拉，屏上继续显示上次的
      if (need.when && !need.when(page.config || {}) && (state.data[page.id] || {}).fetchedAt) continue;
      last[key] = Date.now();
      const q = need.query ? need.query(page.config || {}) : {};
      const qs = new URLSearchParams(q).toString();
      try {
        const data = stamped(await api(`/api/data/${need.kind}${qs ? '?' + qs : ''}`));
        if (JSON.stringify(data) !== JSON.stringify(state.data[page.id])) {
          state.data[page.id] = data;
          saveDataCache();
          onChange(page);
        }
      } catch (e) {
        state.data[page.id] = keepLast(page.id, e);
        saveDataCache();
        last[key] = Date.now() - ttl + 15000;
        onChange(page);
      }
    }
    const page = gridPage();
    const widgets = page ? page.widgets : [];
    for (const w of widgets) {
      const def = WIDGETS[w.type];
      if (!def || !def.needs) continue;
      const ttl = typeof def.needs.ttl === 'function' ? def.needs.ttl(w.config || {}, state.data[w.id]) : (def.needs.ttl || 60000);
      const due = ttl === 0 || !last[w.id] || Date.now() - last[w.id] > ttl;
      if (!due) continue;
      // 休盘/不需要刷新时保持旧数据：有数据就不再拉，等下次该拉的时候自然接上
      if (def.needs.when && !def.needs.when(w.config || {}) && (state.data[w.id] || {}).fetchedAt) continue;
      last[w.id] = Date.now();
      const q = def.needs.query ? def.needs.query(w.config || {}) : {};
      const qs = new URLSearchParams(q).toString();
      try {
        const data = stamped(await api(`/api/data/${def.needs.kind}${qs ? '?' + qs : ''}`));
        const changed = JSON.stringify(data) !== JSON.stringify(state.data[w.id]);
        state.data[w.id] = data;
        saveDataCache();
        if (changed) onChange(w);
      } catch (e) {
        state.data[w.id] = keepLast(w.id, e);
        saveDataCache();
        last[w.id] = Date.now() - ttl + 15000; // 失败别等满 TTL，15 秒后重试
        onChange(w);
      }
    }
  }
  tick();
  setInterval(tick, 500);
  return {
    invalidate: (id) => delete last[id],
    invalidatePage: (id) => delete last['page:' + id],
  };
}

/** 设备镜像状态轮询；session 变化（sidecar 重连）时回调，用于整帧重推 */
export function startStatusLoop(onStatus) {
  async function tick() {
    try {
      const st = await api('/api/status');
      state.device = st;
      onStatus(st);
    } catch (e) {
      state.device = { device: 'host-down', error: String(e) };
      onStatus(state.device);
    }
  }
  tick();
  setInterval(tick, 2000);
}
