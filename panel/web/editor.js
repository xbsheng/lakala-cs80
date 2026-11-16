// 编辑器：多画布（页面 tab）+ gridstack 网格 + 组件属性 + 自定义页面编辑
import {
  CELL, COLS, ROWS, W, H, state, activePage, gridPage, loadLayout, loadPageModule, saveLayout,
  setActivePage, touchWidget, widgetById,
} from './core.js';
import { THEME, WIDGETS, NEWS_TABS, NEWS_LABEL, newsSources, newsLabel } from './widgets.js';

const { GridStack } = window;

/** 每种组件在属性面板里暴露哪些业务字段 */
/** Agent 组件里可以逐个隐藏的 agent（数据里出现的会自动补进列表） */
const AGENT_LABELS = { pi: 'Pi', claude: 'Claude Code', codex: 'Codex', deepseek: 'DeepSeek Harness' };

const FIELDS = {
  clock: [['city', '天气城市'], ['font', '数字字体', ['mono', 'sans', 'lcd']]],
  clock_dial: [['city', '天气城市']],
  weather: [['city', '城市（中文）']],
  stock: [
    ['symbols', 'A 股盘中（代码，逗号分隔）'],
    ['us', '美股盘中（如 100.SPX,100.NDX）'],
    ['usFut', '其余时间/期货（如 103.ES00Y,103.NQ00Y）'],
  ],
  kline: [['symbol', '代码（sh000001）']],
  crypto: [['ids', 'coingecko id（逗号分隔）']],
  github: [['repo', 'owner/repo']],
  timer: [['label', '名称'], ['time', '到点时间 HH:MM']],
  note: [['text', '文字']],
  news: [['rotate', '自动切换秒数']],
  ha: [
    ['temp', '温度实体（sensor）'],
    ['humidity', '湿度实体（sensor）'],
    ['air', '第三项：空气质量 sensor 或 weather'],
    ['climate', '空调/温控（climate）'],
    ['lights', '灯（light，逗号分隔）'],
    ['switches', '开关/插座（switch，逗号分隔）'],
    ['doors', '门窗（binary_sensor，逗号分隔）'],
    ['lock', '门锁（lock）'],
    ['power', '功率（sensor，W）'],
    ['energy', '今日用电（sensor，kWh）'],
    ['media', '媒体播放器（media_player）'],
    ['chips', '概览 chip（climate,lights,switches,media,doors,lock）'],
  ],
};

export const tiles = new Map();
let grid = null;
let hooks = {};
let readOnly = false;   // kiosk：只渲染，不写布局、不触发拖拽
let saveTimer = null;
let customCanvas = null;
let customPreviewTimer = null;
let customPreviewBusy = false;
let dragging = false;
const interacting = () => dragging;

const el = (sel) => document.querySelector(sel);

function toast(msg, kind = '') {
  const box = el('.toasts');
  if (!box) return;
  const node = document.createElement('div');
  node.className = 'toast ' + kind;
  node.textContent = msg;
  box.appendChild(node);
  setTimeout(() => node.remove(), 2200);
}

function drawWidget(ctx, w, rect) {
  const def = WIDGETS[w.type];
  if (!def) return;
  try {
    def.draw(ctx, rect, w.config || {}, state.data[w.id], state.layout.theme || THEME);
  } catch (e) {
    ctx.fillStyle = '#3a1d1d';
    ctx.fillRect(rect.x, rect.y, rect.w, rect.h);
    ctx.fillStyle = '#ffd7d7';
    ctx.font = '400 16px -apple-system, sans-serif';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    ctx.fillText(`组件 ${w.type} 出错`, rect.x + 12, rect.y + 12);
    ctx.fillText(String(e).slice(0, 48), rect.x + 12, rect.y + 34);
    console.error('[widget]', w.type, e);
  }
}

// ---------------- 渲染 ----------------
export function renderTile(w) {
  const canvas = tiles.get(w.id);
  if (!canvas) return;
  const [, , cw, ch] = w.cell;
  if (canvas.width !== cw * CELL || canvas.height !== ch * CELL) {
    canvas.width = cw * CELL;
    canvas.height = ch * CELL;
  }
  const ctx = canvas.getContext('2d');
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  drawWidget(ctx, w, { x: 0, y: 0, w: canvas.width, h: canvas.height });
}

async function renderCustomPreview(page) {
  if (!customCanvas || customPreviewBusy) return;
  customPreviewBusy = true;
  if (customCanvas.width !== W || customCanvas.height !== H) {
    customCanvas.width = W;
    customCanvas.height = H;
  }
  const ctx = customCanvas.getContext('2d');
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  const base = `${location.protocol}//${location.hostname}:8788`;
  try {
    // 只让 Rust renderer 画这一页；它不影响设备正在推的 active page。
    await fetch(`${base}/preview/page?page=${encodeURIComponent(page.id)}`, {
      method: 'POST', cache: 'no-store',
    });
    // renderer 1 Hz；等它重画一帧再取图。
    await new Promise((r) => setTimeout(r, 1100));
    const res = await fetch(`${base}/preview.png?t=${Date.now()}`, { cache: 'no-store' });
    if (!res.ok) throw new Error(`${res.status}`);
    const bmp = await createImageBitmap(await res.blob());
    ctx.drawImage(bmp, 0, 0);
  } catch (e) {
    ctx.fillStyle = (state.layout.theme || {}).bg || THEME.bg;
    ctx.fillRect(0, 0, W, H);
    ctx.fillStyle = '#ffd7d7';
    ctx.font = '400 20px -apple-system, "PingFang SC", "Microsoft YaHei", sans-serif';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    ctx.fillText('Rust renderer 未运行 / preview 不可用', 40, 60);
    ctx.fillStyle = '#8b8c96';
    ctx.font = '400 15px ui-monospace, monospace';
    ctx.fillText(String(e).slice(0, 90), 40, 92);
  } finally {
    customPreviewBusy = false;
  }
}

export async function renderAll() {
  const page = activePage();
  if (!page) return;
  if (page.type === 'grid') {
    for (const w of page.widgets) renderTile(w);
  }
  // custom 页面由 Rust renderer 的 /preview.png 刷新（mountCustom 里的 timer），
  // 不跟随 200ms 的 grid tick；否则会把预览图每 200ms 清掉。
}

function scheduleSave() {
  if (readOnly) return;
  clearTimeout(saveTimer);
  saveTimer = setTimeout(async () => {
    try {
      await saveLayout();
    } catch (e) {
      toast('保存失败：' + e.message, 'warn');
    }
  }, 250);
}

// ---------------- 页面 tab ----------------
function renderTabs() {
  const box = el('#page-tabs');
  box.innerHTML =
    state.layout.pages
      .map(
        (p) => `<button class="tab${p.id === state.layout.activePage ? ' active' : ''}" data-id="${p.id}">
          <span class="tab-name">${esc(p.name)}</span>
          <span class="tab-kind">${p.type === 'custom' ? '自定义' : p.widgets.length + ' 组件'}</span>
        </button>`
      )
      .join('') + `<button class="tab tab-add" id="tab-add" title="新建空白画布">＋</button>
      <span class="hint" style="margin-left:10px">双击 tab 改名 · 右上角「删除画布」删掉当前页</span>`;

  box.querySelectorAll('.tab[data-id]').forEach((tab) => {
    tab.onclick = () => switchPage(tab.dataset.id);
    tab.ondblclick = () => {
      const page = state.layout.pages.find((p) => p.id === tab.dataset.id);
      const name = prompt('画布名称', page.name);
      if (name) {
        page.name = name;
        renderTabs();
        scheduleSave();
      }
    };
  });
  const add = box.querySelector('#tab-add');
  if (add) {
    add.onclick = () => {
      const id = 'page-' + Math.random().toString(36).slice(2, 6);
      state.layout.pages.push({ id, name: '新画布 ' + (state.layout.pages.length + 1), type: 'grid', widgets: [] });
      switchPage(id);
    };
  }
}

async function switchPage(id) {
  if (state.layout.activePage === id) return;
  await setActivePage(id);
  state.selected = null;
  state.pageChangedAt = Date.now();
  await mountActivePage();
  renderTabs();
  hooks.onLayoutChange && hooks.onLayoutChange(); // 换页 = 整帧重推
}

// ---------------- 网格页面 ----------------
function makeTileElement(w) {
  const wrap = document.createElement('div');
  wrap.className = 'grid-stack-item';
  wrap.setAttribute('gs-id', w.id);
  wrap.setAttribute('gs-x', w.cell[0]);
  wrap.setAttribute('gs-y', w.cell[1]);
  wrap.setAttribute('gs-w', w.cell[2]);
  wrap.setAttribute('gs-h', w.cell[3]);
  wrap.innerHTML = `
    <div class="grid-stack-item-content">
      <div class="tile" data-id="${w.id}">
        <canvas></canvas>
        <div class="tile-hit"></div>
        <div class="tile-bar">
          <button data-act="dup" title="复制 (⌘D)"><svg viewBox="0 0 16 16"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/><path d="M10.5 5.5v-3h-8v8h3"/></svg></button>
          <button data-act="del" title="删除 (⌫)"><svg viewBox="0 0 16 16"><path d="M4 4l8 8M12 4l-8 8"/></svg></button>
        </div>
      </div>
    </div>`;
  tiles.set(w.id, wrap.querySelector('canvas'));
  return wrap;
}

function bindTileEvents(wrap, w) {
  const tile = wrap.querySelector('.tile');
  tile.addEventListener('pointerdown', () => select(w.id));
  tile.querySelectorAll('[data-act]').forEach((btn) => {
    btn.addEventListener('pointerdown', (ev) => ev.stopPropagation());
    btn.addEventListener('click', (ev) => {
      ev.stopPropagation();
      if (btn.dataset.act === 'del') removeWidget(w.id);
      else duplicateWidget(w.id);
    });
  });
}

function mountGrid() {
  const page = gridPage();
  tiles.clear();
  // 换画布时要把上一张的 grid 实例和 DOM 一起拆掉：否则旧组件还留在屏幕上、新组件会叠上去
  if (grid) {
    grid.destroy(false);
    grid = null;
  }
  el('#grid').innerHTML = '';
  el('#custom-holder').hidden = true;
  el('#board').hidden = false;

  grid = GridStack.init(
    {
      column: COLS,
      maxRow: ROWS,
      cellHeight: CELL * state.zoom,
      margin: 0,
      float: true,
      animate: true,
      disableOneColumnMode: true,
      handle: '.tile-hit',
      resizable: { handles: 'nw,e,se,s,sw,w' },
      draggable: { scroll: true, appendTo: 'body' },
    },
    el('#grid')
  );
  applyZoom(state.zoom);

  for (const w of page.widgets) {
    touchWidget(w);
    const node = makeTileElement(w);
    el('#grid').appendChild(node);
    grid.makeWidget(node);
    bindTileEvents(node, w);
    renderTile(w);
  }

  // 拖动/缩放过程中只更新编辑器，推帧留到松手：整帧推送又慢（2 MB）又可能踩固件的 LZ4 坑，
  // 每 200ms 推一帧会让屏上的画面滞后好几秒。
  grid.on('dragstart resizestart', () => {
    dragging = true;
    if (!readOnly && hooks.onInteract) hooks.onInteract(true);
  });
  grid.on('dragstop resizestop', () => {
    dragging = false;
    if (readOnly) return;
    if (hooks.onInteract) hooks.onInteract(false);
    hooks.onLayoutChange && hooks.onLayoutChange();
  });
  grid.on('change', (_ev, nodes) => {
    (nodes || []).forEach(syncCellFromNode);
    if (readOnly) return;   // kiosk 只负责画；别把布局写回去，也别 markFull
    scheduleSave();
    if (!interacting()) hooks.onLayoutChange && hooks.onLayoutChange();
    renderInspector();
  });
}

function mountCustom() {
  if (grid) {
    grid.destroy(false);
    grid = null;
  }
  tiles.clear();
  el('#grid').innerHTML = '';
  clearInterval(customPreviewTimer);
  customPreviewTimer = null;
  el('#board').hidden = true;
  el('#custom-holder').hidden = false;
  customCanvas = el('#custom-canvas');
  applyZoom(state.zoom);
  const ctx = customCanvas.getContext('2d');
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.fillStyle = (state.layout.theme || {}).bg || THEME.bg;
  ctx.fillRect(0, 0, W, H);
  renderCustomPreview(activePage());
  customPreviewTimer = setInterval(() => {
    const p = activePage();
    if (p && p.type === 'custom') renderCustomPreview(p);
  }, 2500);
}

async function mountActivePage() {
  const page = activePage();
  if (!page) return;
  if (page.type === 'grid') mountGrid();
  else mountCustom();
  renderInspector();
}

/** kiosk 用：重新拉布局并原地重建，不整页 reload。
 * 整页 reload 会丢掉 pusher 的上一帧、重建整个 Chromium 页面（2~5 秒无帧），
 * 而原地重建只需要重画组件，pusher 的 diff 会把变化的 tile（或必要时整帧）推上去。 */
export async function reloadLayout() {
  await loadLayout();
  await mountActivePage();
  renderTabs();
}

function syncCellFromNode(node) {
  const w = widgetById(node.id || node.el.getAttribute('gs-id'));
  if (!w) return;
  w.cell = [node.x, node.y, node.w, node.h];
  renderTile(w);
}

// ---------------- 组件增删 ----------------
function addWidget(type, pos) {
  const page = gridPage();
  const def = WIDGETS[type];
  if (!page || !def) {
    toast('自定义画布不能放组件，先新建一个网格画布', 'warn');
    return null;
  }
  const [mw, mh] = def.min || [1, 1];
  const w = Math.max(2, mw);
  const h = Math.max(1, mh);
  const widget = {
    id: `${type}-${Math.random().toString(36).slice(2, 7)}`,
    type,
    cell: pos ? [pos.x, pos.y, w, h] : firstFree(w, h),
    config: { ...(def.config || {}) },
  };
  page.widgets.push(widget);
  const node = makeTileElement(widget);
  el('#grid').appendChild(node);
  grid.makeWidget(node);
  bindTileEvents(node, widget);
  renderTile(widget);
  select(widget.id);
  renderTabs();
  scheduleSave();
  hooks.onLayoutChange && hooks.onLayoutChange();
  return widget;
}

function firstFree(w, h) {
  const page = gridPage();
  for (let y = 0; y + h <= ROWS; y++) {
    for (let x = 0; x + w <= COLS; x++) {
      const clash = page.widgets.some((o) => {
        const [a, b, aw, ah] = o.cell;
        return x < a + aw && a < x + w && y < b + ah && b < y + h;
      });
      if (!clash) return [x, y, w, h];
    }
  }
  return [0, 0, w, h];
}

function removeWidget(id) {
  const page = gridPage();
  const w = widgetById(id);
  if (!page || !w) return;
  const node = el(`.grid-stack-item[gs-id="${id}"]`);
  if (node) grid.removeWidget(node, true);
  tiles.delete(id);
  delete state.data[id];
  page.widgets = page.widgets.filter((x) => x.id !== id);
  if (state.selected === id) select(null);
  renderTabs();
  scheduleSave();
  hooks.onLayoutChange && hooks.onLayoutChange();
}

function duplicateWidget(id) {
  const page = gridPage();
  const src = widgetById(id);
  if (!page || !src) return;
  const copy = {
    id: `${src.type}-${Math.random().toString(36).slice(2, 7)}`,
    type: src.type,
    cell: firstFree(src.cell[2], src.cell[3]),
    config: { ...src.config },
  };
  page.widgets.push(copy);
  const node = makeTileElement(copy);
  el('#grid').appendChild(node);
  grid.makeWidget(node);
  bindTileEvents(node, copy);
  renderTile(copy);
  select(copy.id);
  renderTabs();
  scheduleSave();
  hooks.onLayoutChange && hooks.onLayoutChange();
}

// ---------------- 选择 / 属性面板 ----------------
export function select(id) {
  state.selected = id;
  document.querySelectorAll('.tile').forEach((t) => t.classList.toggle('selected', t.dataset.id === id));
  renderInspector();
}

function renderInspector() {
  const box = el('#inspector-body');
  const page = activePage();
  if (!page) return;

  if (page.type === 'custom') {
    box.innerHTML = `
      <div class="section-label">自定义画布 · ${esc(page.name)}</div>
      <div class="field"><label>模块（相对 panel/web/）</label>
        <input class="input mono" data-page="module" value="${esc(page.module || 'pages/stock.js')}"></div>
      <div class="field"><label>配置（JSON，传给模块的 draw）</label>
        <textarea class="input mono" data-page="config" rows="6" style="height:auto;padding:6px">${esc(
          JSON.stringify(page.config || {}, null, 1)
        )}</textarea></div>
      <div class="hint">页面是一个 ES 模块：<code>export default { needs, draw(ctx, size, data, cfg, theme) }</code><br>
        示例见 <code>panel/web/pages/stock.js</code>（自选股 + 分时图），改完保存即生效。</div>`;
    box.querySelectorAll('[data-page]').forEach((input) => {
      input.addEventListener('change', () => {
        if (input.dataset.page === 'module') page.module = input.value.trim();
        else {
          try {
            page.config = JSON.parse(input.value || '{}');
          } catch (e) {
            toast('JSON 解析失败：' + e.message, 'warn');
            return;
          }
        }
        if (hooks.invalidatePage) hooks.invalidatePage(page.id);
        renderCustomPreview(page);
        scheduleSave();
        hooks.onLayoutChange && hooks.onLayoutChange();
      });
    });
    return;
  }

  const w = widgetById(state.selected);
  if (!w) {
    box.innerHTML = `<div class="empty-state">
      <svg viewBox="0 0 24 24"><path d="M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z"/></svg>
      <div>点画布里的组件来编辑<br><span class="hint">左侧组件库点击添加</span></div>
    </div>`;
    return;
  }
  const def = WIDGETS[w.type] || {};
  const fields = FIELDS[w.type] || [];
  box.innerHTML = `
    <div class="section-label">${def.label || w.type} · ${w.type}</div>
    <div class="field"><label>标题（留空用默认）</label>
      <input class="input" data-cfg="title" value="${esc(w.config.title || '')}" placeholder="${def.label || ''}"></div>
    ${fields
      .map(([key, label, options]) =>
        options
          ? `<div class="field"><label>${label}</label>
             <select class="input" data-cfg="${key}">
               ${options
                 .map((o) => `<option value="${esc(o)}"${(w.config[key] ?? options[0]) === o ? ' selected' : ''}>${esc(o)}</option>`)
                 .join('')}
             </select></div>`
          : `<div class="field"><label>${label}</label>
             <input class="input mono" data-cfg="${key}" value="${esc(w.config[key] ?? '')}"></div>`
      )
      .join('')}
    ${w.type === 'agent' ? agentToggles(w) : ''}
    ${w.type === 'news' ? newsToggles(w) : ''}
    <div class="section-label">位置 / 尺寸（格）</div>
    <div class="grid4">
      ${['x', 'y', 'w', 'h']
        .map(
          (k, i) =>
            `<input class="input" type="number" data-cell="${i}" value="${w.cell[i]}" min="${
              i < 2 ? 0 : 1
            }" max="${i % 2 === 0 ? COLS : ROWS}">`
        )
        .join('')}
    </div>
    <div class="row" style="margin-top:12px">
      <button class="btn btn-sm" data-act="dup">复制</button>
      <button class="btn btn-sm" data-act="del">删除</button>
    </div>
    ${def.needs ? `<div class="hint" style="margin-top:10px">数据源 <code>${def.needs.kind}</code>，缓存 ${typeof def.needs.ttl === 'function' ? '动态' : Math.round((def.needs.ttl || 0) / 1000) + 's'}</div>` : ''}`;

  box.querySelectorAll('[data-cfg]').forEach((input) => {
    input.addEventListener('change', () => {
      w.config[input.dataset.cfg] = input.value;
      if (hooks.invalidateData) hooks.invalidateData(w.id);
      renderTile(w);
      scheduleSave();
    });
  });
  box.querySelectorAll('[data-cell]').forEach((input) => {
    input.addEventListener('change', () => {
      const i = Number(input.dataset.cell);
      const v = Math.max(0, Number(input.value) || 0);
      const next = [...w.cell];
      next[i] = v;
      next[0] = Math.min(next[0], COLS - next[2]);
      next[1] = Math.min(next[1], ROWS - next[3]);
      w.cell = next;
      const node = el(`.grid-stack-item[gs-id="${w.id}"]`);
      if (node) grid.update(node, { x: next[0], y: next[1], w: next[2], h: next[3] });
      renderTile(w);
      scheduleSave();
      hooks.onLayoutChange && hooks.onLayoutChange();
    });
  });
  box.querySelectorAll('[data-hidetask]').forEach((input) => {
    input.addEventListener('change', () => {
      const hidden = new Set(w.config.hideTasks || []);
      if (input.checked) hidden.delete(input.dataset.hidetask);
      else hidden.add(input.dataset.hidetask);
      w.config.hideTasks = [...hidden];
      renderTile(w);
      scheduleSave();
      hooks.onLayoutChange && hooks.onLayoutChange();
    });
  });
  box.querySelectorAll('[data-hide]').forEach((input) => {
    input.addEventListener('change', () => {
      const hidden = new Set(w.config.hide || []);
      if (input.checked) hidden.delete(input.dataset.hide);
      else hidden.add(input.dataset.hide);
      w.config.hide = [...hidden];
      renderTile(w);
      scheduleSave();
      hooks.onLayoutChange && hooks.onLayoutChange();
    });
  });
  if (w.type === 'news') bindNewsControls(box, w);
  box.querySelector('[data-act="dup"]').onclick = () => duplicateWidget(w.id);
  box.querySelector('[data-act="del"]').onclick = () => removeWidget(w.id);
}

/** Agent 组件配置：agent 分组 + 每个 agent 下的任务，都能单独取消勾选 */
function agentToggles(w) {
  const cfg = w.config;
  const hidden = new Set(cfg.hide || []);
  const hiddenTasks = new Set(cfg.hideTasks || []);
  const items = (state.data[w.id] && state.data[w.id].items) || [];

  const groups = new Map();
  for (const id of Object.keys(AGENT_LABELS)) groups.set(id, []);
  for (const it of items) {
    if (!groups.has(it.agent)) groups.set(it.agent, []);
    groups.get(it.agent).push(it);
  }

  const ageText = (secs) => {
    if (!secs) return 'now';
    if (secs < 60) return secs + 's';
    if (secs < 3600) return Math.round(secs / 60) + 'm';
    if (secs < 86400) return Math.round(secs / 3600) + 'h';
    return Math.round(secs / 86400) + 'd';
  };
  const cut = (str) => {
    const t = String(str || '');
    return t.length > 24 ? t.slice(0, 24) + '…' : t || '（无标题）';
  };

  const rows = [];
  for (const [agent, list] of groups) {
    const on = !hidden.has(agent);
    rows.push(`<label class="check">
      <input type="checkbox" data-hide="${esc(agent)}" ${on ? 'checked' : ''}>
      <span>${esc(AGENT_LABELS[agent] || agent)}</span>
      <span class="count">${list.length || ''}</span>
    </label>`);
    for (const it of list) {
      const tOn = on && !hiddenTasks.has(it.id);
      rows.push(`<label class="check task${on ? '' : ' muted'}" title="${esc(it.title || '')}">
        <input type="checkbox" data-hidetask="${esc(it.id)}" ${tOn ? 'checked' : ''} ${on ? '' : 'disabled'}>
        <span class="name">${esc(cut(it.title))}</span>
        <span class="age">${ageText(it.age)}</span>
      </label>`);
    }
  }

  return `<div class="section-label">显示哪些 Agent / 任务</div>
    <div class="checks">${rows.join('')}</div>
    <div class="hint">取消 Agent ＝ 整组隐藏；只取消某个任务 ＝ 该 Agent 保留、这个任务不显示${
      hidden.size || hiddenTasks.size ? `（已隐藏 ${hidden.size} 个 Agent、${hiddenTasks.size} 条任务）` : ''
    }</div>`;
}

/** 资讯组件：勾选的数据源 = 顶部 Tab（可拖动排序 + 改 Tab 名字） */
function newsToggles(w) {
  const on = newsSources(w.config);
  const labels = w.config.labels || {};
  const off = NEWS_TABS.map(([id]) => id).filter((id) => !on.includes(id));

  const onRow = (id, i) => `
    <div class="check news-row">
      <input type="checkbox" data-news="${esc(id)}" checked title="取消勾选＝不显示这个 Tab">
      <input class="input news-name" data-news-label="${esc(id)}" value="${esc(labels[id] || '')}"
             placeholder="${esc(NEWS_LABEL[id])}" title="Tab 显示名，留空用默认">
      <button class="btn btn-sm icon" data-news-up="${esc(id)}" ${i === 0 ? 'disabled' : ''} title="上移">↑</button>
      <button class="btn btn-sm icon" data-news-down="${esc(id)}" ${i === on.length - 1 ? 'disabled' : ''} title="下移">↓</button>
    </div>`;

  const offRow = (id) => `
    <label class="check muted">
      <input type="checkbox" data-news="${esc(id)}">
      <span>${esc(NEWS_LABEL[id])}</span>
    </label>`;

  return `<div class="section-label">数据源（顶部 Tab，按顺序轮播）</div>
    <div class="checks">
      ${on.map(onRow).join('')}
      ${off.map(offRow).join('')}
    </div>
    <div class="hint">勾选＝启用；↑↓ 调顺序；中间输入框可以改 Tab 名字（留空用默认名）。</div>`;
}

/** 资讯组件的排序 / 改名 / 勾选事件 */
function bindNewsControls(box, w) {
  const commit = () => {
    if (hooks.invalidateData) hooks.invalidateData(w.id);
    renderTile(w);
    scheduleSave();
    hooks.onLayoutChange && hooks.onLayoutChange();
  };
  box.querySelectorAll('[data-news]').forEach((input) => {
    input.addEventListener('change', () => {
      const id = input.dataset.news;
      const on = newsSources(w.config);
      w.config.sources = input.checked
        ? [...on.filter((x) => x !== id), id]          // 新启用排到最后
        : on.filter((x) => x !== id);
      commit();
      renderInspector();
    });
  });
  box.querySelectorAll('[data-news-up], [data-news-down]').forEach((btn) => {
    btn.addEventListener('click', (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      const up = 'newsUp' in btn.dataset;
      const id = up ? btn.dataset.newsUp : btn.dataset.newsDown;
      const list = newsSources(w.config);
      const i = list.indexOf(id);
      const j = up ? i - 1 : i + 1;
      if (i < 0 || j < 0 || j >= list.length) return;
      [list[i], list[j]] = [list[j], list[i]];
      w.config.sources = list;
      commit();
      renderInspector();
    });
  });
  box.querySelectorAll('[data-news-label]').forEach((input) => {
    input.addEventListener('change', () => {
      const id = input.dataset.newsLabel;
      const name = input.value.trim();
      const labels = { ...(w.config.labels || {}) };
      if (name && name !== NEWS_LABEL[id]) labels[id] = name;
      else delete labels[id];
      w.config.labels = labels;
      commit();
    });
  });
}

const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

// ---------------- 启动 ----------------
export async function initEditor(h) {
  hooks = h || {};
  readOnly = !!hooks.readOnly;
  await loadLayout();
  buildPalette();
  await mountActivePage();
  renderTabs();

  el('#grid').addEventListener('pointerdown', (ev) => {
    if (ev.target.closest('.tile')) return;
    select(null);
  });
  document.addEventListener('keydown', onKey);
  return grid;
}

function buildPalette() {
  const box = el('#palette');
  box.innerHTML = Object.entries(WIDGETS)
    .map(
      ([type, def]) => `
      <div class="palette-item" data-type="${type}" title="点击添加 ${def.label}">
        <svg viewBox="0 0 16 16">${def.icon || ''}</svg>
        <span>${def.label}</span>
        <small>${def.min ? def.min.join('×') + ' 起' : ''}</small>
      </div>`
    )
    .join('');
  box.querySelectorAll('.palette-item').forEach((item) => {
    item.addEventListener('click', () => addWidget(item.dataset.type));
  });
}

function onKey(ev) {
  if (ev.target.matches('input, textarea, select')) return;
  const w = widgetById(state.selected);
  if (!w) return;
  const bump = (dx, dy) => {
    const next = [
      Math.max(0, Math.min(COLS - w.cell[2], w.cell[0] + dx)),
      Math.max(0, Math.min(ROWS - w.cell[3], w.cell[1] + dy)),
      w.cell[2],
      w.cell[3],
    ];
    w.cell = next;
    const node = el(`.grid-stack-item[gs-id="${w.id}"]`);
    if (node) grid.update(node, { x: next[0], y: next[1], w: next[2], h: next[3] });
    renderTile(w);
    renderInspector();
    scheduleSave();
    hooks.onLayoutChange && hooks.onLayoutChange();
    ev.preventDefault();
  };
  if (ev.key === 'ArrowLeft') bump(-1, 0);
  else if (ev.key === 'ArrowRight') bump(1, 0);
  else if (ev.key === 'ArrowUp') bump(0, -1);
  else if (ev.key === 'ArrowDown') bump(0, 1);
  else if (ev.key === 'Backspace' || ev.key === 'Delete') {
    removeWidget(w.id);
    ev.preventDefault();
  } else if ((ev.metaKey || ev.ctrlKey) && ev.key.toLowerCase() === 'd') {
    duplicateWidget(w.id);
    ev.preventDefault();
  }
}

export function applyZoom(z) {
  state.zoom = Math.max(0.3, Math.min(1, z));
  const cellH = CELL * state.zoom;
  const boardW = COLS * CELL * state.zoom;
  const boardH = ROWS * CELL * state.zoom;
  const board = el('#board');
  board.style.width = boardW + 20 + 'px';
  board.style.setProperty('--cell-w', CELL * state.zoom + 'px');
  board.style.setProperty('--cell-h', cellH + 'px');
  if (grid) grid.cellHeight(cellH);
  const holder = el('#custom-holder');
  holder.style.width = boardW + 20 + 'px';
  const canvas = el('#custom-canvas');
  canvas.style.width = boardW + 'px';
  canvas.style.height = boardH + 'px';
  el('#zoom-val').textContent = Math.round(state.zoom * 100) + '%';
}

export async function deleteCurrentPage() {
  if (state.layout.pages.length <= 1) {
    toast('至少保留一个画布', 'warn');
    return;
  }
  const page = activePage();
  if (!confirm(`删除画布「${page.name}」？`)) return;
  state.layout.pages = state.layout.pages.filter((p) => p.id !== page.id);
  await setActivePage(state.layout.pages[0].id);
  await mountActivePage();
  renderTabs();
  await saveLayout();
  hooks.onLayoutChange && hooks.onLayoutChange();
  toast('已删除');
}

export { addWidget, removeWidget, duplicateWidget, mountActivePage, toast };
