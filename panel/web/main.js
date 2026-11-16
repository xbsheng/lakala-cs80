// 入口：把编辑器、数据调度、设备推送接起来
import { H, W, api, postJson, saveLayout, state, startDataLoop, startStatusLoop } from './core.js';
import { Pusher } from './pusher.js';
import { applyZoom, deleteCurrentPage, initEditor, renderAll, renderTile, reloadLayout, tiles, toast } from './editor.js';

const el = (sel) => document.querySelector(sel);
const pusher = new Pusher();
const QS = new URLSearchParams(location.search);
const KIOSK = QS.has("kiosk");
const EDIT_ONLY = !!window.__CS80_HEADLESS__ && !KIOSK;
if (EDIT_ONLY) pusher.paused = true;  // 无桌面主机：浏览器只编辑，推帧交给后台 kiosk

// 如果本机 Rust renderer 的 preview server 在跑，设备由它推；浏览器不要和它抢 USB。
if (!EDIT_ONLY) {
  const previewBase = `${location.protocol}//${location.hostname}:8788`;
  fetch(`${previewBase}/health`, { cache: 'no-store' })
    .then((r) => r.json())
    .then((j) => {
      if (!j || !j.ok) return;
      pusher.paused = true;
      const b = el('#btn-pause');
      if (b) {
        b.disabled = true;
        b.classList.add('primary');
        b.textContent = 'Rust 推屏中';
        b.title = '设备由 Rust renderer 推送；这里只编辑布局';
      }
      toast('Rust renderer 正在推屏，编辑器已暂停推送');
    })
    .catch(() => {});
}

function paintStats(st) {
  const pill = el('#push-pill');
  if (st.kind === 'error') {
    pill.textContent = '推送失败';
    pill.className = 'pill mono off';
    return;
  }
  if (st.ok === false) {
    // host 的队列满了（设备 ACK 慢）：这一帧没进队，下一轮会带上漏掉的 tile
    pill.textContent = `设备积压 · ${st.ms}ms`;
    pill.className = 'pill mono off';
    return;
  }
  const what = st.kind === 'full' ? '整帧' : st.kind === 'patch' ? `${st.tiles} tile` : '无变化';
  pill.textContent = `推送 ${what} · ${st.ms}ms`;
  pill.className = 'pill mono';
}

let saver = { files: [], file: '', autoLock: true, video: null, active: false, byLock: false };
let tickMs = 200;
let tickTimer = null;
let setRateFn = null;
let wasUp = null;
let lastDeviceToast = 0;
/** 设备重连提示限流：链路抖动时 session 会连着变，别每 5 秒弹一次 */
function toastDevice(msg) {
  if (Date.now() - lastDeviceToast < 30000) return;
  lastDeviceToast = Date.now();
  toast(msg);
}
let lastFullPush = 0;
let loadedBuild = null;
let usbWarned = false;
let replugWarned = false;
let reloadedFor = null;

function paintDevice(st) {
  // 后端 web/ 变了（重新 build）就让页面自己重载，避免一直跑旧代码
  if (st.build) {
    if (loadedBuild === null) loadedBuild = st.build;
    else if (st.build !== loadedBuild && reloadedFor !== st.build) {
      reloadedFor = st.build;
      toast('检测到前端更新，正在重载…');
      setTimeout(() => location.reload(), 400);
      return;
    }
  }
  const up = st.device === 'up';
  const usb = st.usb;
  if (up && st.needs_replug && !replugWarned) {
    replugWarned = true;
    toast('设备收不到推帧（固件接收卡死），请拔插一下 Type-C 复位', 'warn');
  }
  if (up && usb && usb.speed && usb.speed < 480 && !usbWarned) {
    usbWarned = true;
    toast(`USB ${usb.speed} Mbps（全速）：整帧要 1~2 秒，插到主机直连 USB 口可提速`, 'warn');
  }
  const pill = el('#device-pill');
  pill.className = 'pill ' + (up ? 'on' : st.device === 'connecting' ? '' : 'off');
  el('#device-text').textContent =
    up ? '设备在线' : st.device === 'connecting' ? '连接设备…' : st.device === 'host-down' ? 'host 未运行' : '设备离线';
  // 设备重新上线（拔插/休眠唤醒后 sidecar 重启）→ 必须整帧重推，设备端 framebuffer 状态已丢
  if (up && wasUp === false) {
    pusher.markFull();
    toastDevice('设备已重连，重推整帧');
  }
  wasUp = up;
  // 局部更新失败（设备换了/固件重启导致 base 哈希不匹配）→ 退回整帧
  const side = st.sidecar || {};
  if ((side.event === 'patch' || side.event === 'frame') && side.ok === 0) pusher.markFull();
  const b = st.state && st.state.brightness;
  if (typeof b === 'number') {
    el('#brightness').value = b;
    el('#brightness-val').textContent = b;
    el('#brightness').style.setProperty('--pct', b + '%');
  }
  const edges = st.state && st.state.edges;
  const bits = st.features && Number(st.features.bits);
  // 0x40 = QD_FEATURE_SCREEN_POWER：0.3.1 起才有（等待页也开始每秒重画倒计时）
  const fw = typeof bits === 'number' ? (bits & 0x40 ? '0.3.1+' : '0.3.0') : '—';
  const pwm = typeof edges === 'number' ? (edges ? `运行 ${edges}` : '停（100%）') : '—';
  const au = st.audio || {};
  const audioLabel = au.event === 'audio'
    ? (au.ok ? `OK${typeof au.ms === 'number' ? ' ' + au.ms + 'ms' : ''}` : (au.err || 'FAIL'))
    : '—';
  el('#device-detail').innerHTML = up
    ? `块大小 <code>${(st.ready && st.ready.block) || '—'}</code><br>
       特性位 <code>${typeof bits === 'number' ? '0x' + bits.toString(16) : '—'}</code> · 固件 <code>${fw}</code><br>
       背光 <code>${typeof b === 'number' ? b + '%' : '—'}</code> · PWM <code>${pwm}</code><br>
       喇叭 <code>${audioLabel}</code><br>
       USB <code>${usb && usb.speed ? usb.speed + ' Mbps' : '—'}</code>${usb && usb.speed && usb.speed < 480 ? '（全速，整帧慢）' : ''} ·
       session <code>${st.session ?? '—'}</code> · 最近事件 <code>${side.event || '—'}</code>
       ${st.needs_replug ? '<br><b>设备推帧持续失败：拔插 Type-C 重新枚举</b>' : ''}
       ${side.wire ? ` · ${side.wire} B` : ''}`
    : `未连上设备${st.reason ? '（' + st.reason + '）' : ''}<br>检查 Type-C 数据线 / 供电 / 官方上位机是否占着串口。<br>
       日志 <code>${(st.logs && st.logs.sidecar) || '~/.cs80_panel/sidecar.log'}</code>`;

  // sidecar 重连（device 重启/拔插）后设备丢了帧状态 → 整帧重推
  if (pusher.session !== null && st.session !== pusher.session) {
    pusher.markFull();
    toastDevice('设备重连，已整帧重推');
  }
  pusher.session = st.session;
}

async function boot() {
  pusher.onStats = paintStats;

  const dataLoop = startDataLoop((w) => renderTile(w));
  await initEditor({
    readOnly: KIOSK,   // kiosk 不写布局、不响应拖拽；布局变更由 reloadLayout 原地应用
    onLayoutChange: () => {
      if (!KIOSK) pusher.markFull();
    },
    // 拖动/缩放期间屏上先不动，松手统一推一帧
    onInteract: (busy) => {
      pusher.interacting = busy;
      if (!busy && !KIOSK) pusher.markFull();
    },
    invalidateData: (id) => dataLoop.invalidate(id),
    invalidatePage: (id) => dataLoop.invalidatePage(id),
  });
  el('#btn-delpage').onclick = () => deleteCurrentPage();

  document.addEventListener('layout:invalid', () => pusher.markFull());

  // 顶栏：亮度
  const slider = el('#brightness');
  slider.addEventListener('input', () => {
    el('#brightness-val').textContent = slider.value;
    slider.style.setProperty('--pct', slider.value + '%');
  });
  slider.addEventListener('change', async () => {
    try {
      await postJson('/api/control', { cmd: 'brightness', value: Number(slider.value) });
      toast(`亮度 ${slider.value}`);
    } catch (e) {
      toast('亮度设置失败：' + e.message, 'warn');
    }
  });
  let paused = false;
  el('#btn-pause').onclick = () => {
    paused = !paused;
    pusher.paused = paused;
    el('#btn-pause').classList.toggle('primary', paused);
    el('#btn-pause').textContent = paused ? '继续推送' : '暂停推送';
    if (!paused) {
      pusher.markFull();
      lastFullPush = Date.now();
    }
    toast(paused ? '已暂停推帧（屏上定格）' : '已恢复推送');
  };
  if (EDIT_ONLY) {
    const b = el('#btn-pause');
    b.disabled = true;
    b.textContent = '后台推送中';
    b.classList.add('primary');
    b.title = '屏幕由本机 kiosk 服务持续推送；这里只编辑布局';
  }
  // 纯色自检：整屏刷一种颜色，用来看屏自身的闪（背光 PWM / 面板 / 固件重绘）
  // 先把推送暂停，否则下一次 tick 会立刻把纯色盖回去
  for (const btn of document.querySelectorAll('#diag-colors button')) {
    btn.onclick = async () => {
      const color = btn.dataset.color;
      if (!pusher.paused) el('#btn-pause').click();
      try {
        await postJson('/api/device/pattern', { color });
        pusher.markFull();
        toast(`屏上已刷纯色：${btn.textContent}（看完点「继续推送」恢复画面）`);
      } catch (e) {
        toast('纯色自检失败：' + e.message, 'warn');
      }
    };
  }
  el('#device-pill').onclick = async () => {
    await postJson('/api/device/reconnect', {});
    toast('正在重连设备…');
  };
  el('#btn-sleep').onclick = () => postJson('/api/control', { cmd: 'sleep' }).then(() => toast('已待机'));
  el('#btn-wake').onclick = () => postJson('/api/control', { cmd: 'wake' }).then(() => toast('已唤醒'));
  // 音频播放：独立弹窗（非 MP3 在 host 侧用 ffmpeg 转成 128k MP3 再上传给设备）
  const audioModal = el('#audio-modal');
  el('#btn-audio-open').onclick = () => {
    audioModal.hidden = false;
    refreshAudio();
  };
  el('#audio-close').onclick = () => { audioModal.hidden = true; };
  audioModal.addEventListener('click', (ev) => {
    if (ev.target === audioModal) audioModal.hidden = true;
  });
  document.addEventListener('keydown', (ev) => {
    if (ev.key === 'Escape') audioModal.hidden = true;
  });
  async function refreshAudio() {
    try {
      const j = await postJson('/api/audio', { action: 'query' });
      const au = j.audio || {};
      el('#audio-state').textContent = au.ok
        ? `设备音量 ${au.receive ?? '—'} · ${au.decode ? '播放中' : '空闲'}`
        : `查询失败：${au.err || '未连接设备'}`;
      if (typeof au.receive === 'number' && au.receive >= 0) {
        el('#audio-volume').value = au.receive;
        el('#audio-volume-val').textContent = au.receive;
      }
    } catch (e) {
      el('#audio-state').textContent = '查询失败：' + e.message;
    }
  }
  el('#btn-audio-pick').onclick = () => el('#audio-file').click();
  el('#audio-file').addEventListener('change', async (ev) => {
    const f = ev.target.files[0];
    ev.target.value = '';
    if (!f) return;
    const state = el('#audio-state');
    state.textContent = `上传中… ${f.name}（${(f.size / 1024).toFixed(0)} KB）`;
    try {
      const res = await fetch('/api/audio/file?play=1&name=' + encodeURIComponent(f.name), {
        method: 'POST',
        headers: { 'Content-Type': 'application/octet-stream' },
        body: f,
      });
      const j = await res.json();
      if (!j.ok) throw new Error(j.error || (j.play && j.play.err) || (j.upload && j.upload.err) || '播放失败');
      const c = j.upload && j.upload.convert;
      const conv = c && c.converted ? `${c.format} → mp3 · ${c.ms}ms` : 'mp3 直传';
      state.textContent = `播放中 · ${f.name} · ${conv}`;
    } catch (e) {
      state.textContent = '失败：' + e.message;
      toast('音频：' + e.message, 'warn');
    }
  });
  el('#btn-audio-stop').onclick = async () => {
    try {
      const j = await postJson('/api/audio', { action: 'stop' });
      el('#audio-state').textContent = j.ok ? '已停止' : '停止失败';
    } catch (e) {
      el('#audio-state').textContent = '停止失败：' + e.message;
    }
  };
  const vol = el('#audio-volume');
  vol.addEventListener('input', () => (el('#audio-volume-val').textContent = vol.value));
  vol.addEventListener('change', async () => {
    try {
      await postJson('/api/audio', { action: 'volume', value: Number(vol.value) });
      el('#audio-state').textContent = `设备音量 ${vol.value}`;
    } catch (e) {
      el('#audio-state').textContent = '设置音量失败：' + e.message;
    }
  });

  el('#btn-full').onclick = () => {
    pusher.markFull();
    toast('整帧重推');
  };

  // 缩放
  el('#zoom-in').onclick = () => applyZoom(state.zoom + 0.06);
  el('#zoom-out').onclick = () => applyZoom(state.zoom - 0.06);

  // 布局导入导出
  el('#btn-export').onclick = () => {
    const blob = new Blob([JSON.stringify(state.layout, null, 2)], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'cs80-layout.json';
    a.click();
    URL.revokeObjectURL(a.href);
  };
  el('#btn-import').onclick = () => el('#file-import').click();
  el('#file-import').addEventListener('change', async (ev) => {
    const file = ev.target.files[0];
    if (!file) return;
    try {
      const layout = JSON.parse(await file.text());
      await api('/api/layout', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(layout),
      });
      location.reload();
    } catch (e) {
      toast('导入失败：' + e.message, 'warn');
    }
  });
  el('#btn-default').onclick = async () => {
    if (!confirm('恢复默认布局？当前布局会被覆盖')) return;
    await api('/api/layout', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ reset: true }),
    });
    location.reload();
  };

  startStatusLoop(paintDevice);

  // 渲染 + 推送：时钟秒级；屏保时提速到 ~11fps
  const tick = () => {
    if (!saver.active) renderAll();
    if (pusher.interacting) return; // 拖动中不推帧（见 editor.js）
    // 周期整帧只是兜底：patch 链有 base/target 哈希校验，出错会 markFull；
    // 整帧会触发固件重绘/可能的 LZ4 坑，拉长到 60 分钟，减少屏上「等待连接」的次数。
    if (Date.now() - lastFullPush > 3600000 && !saver.active) {
      pusher.markFull();
      lastFullPush = Date.now();
    }
    pusher.tick(tiles);
  };
  const setRate = (ms) => {
    if (tickMs === ms && tickTimer) return;
    tickMs = ms;
    if (tickTimer) clearInterval(tickTimer);
    tickTimer = setInterval(tick, ms);
  };
  setRate(100);
  setRateFn = setRate;

  // kiosk：布局被编辑器改动后原地重建（不整页 reload），并给外部看门狗发心跳
  if (KIOSK) {
    // 页面被浏览器冻结/崩溃时这个定时器会停 → panel/kiosk-guard.sh 会重启 Chromium。
    // 注意：只让 kiosk 发，编辑器 tab 不能刷新它，否则 kiosk 冻结时会被掩盖。
    const beat = () => fetch('/api/heartbeat', { method: 'POST' }).catch(() => {});
    beat();
    setInterval(beat, 5000);
    let sig = null;
    let applying = false;
    setInterval(async () => {
      if (applying) return;
      try {
        const l = await (await fetch('/api/layout', { cache: 'no-store' })).json();
        const s = JSON.stringify(l);
        if (sig === null) {
          sig = s;
          return;
        }
        if (s === sig) return;
        sig = s;
        applying = true;
        try {
          await reloadLayout();
          toast('布局已更新');
        } catch (e) {
          console.warn('apply layout failed, reload', e);
          location.reload();
        } finally {
          applying = false;
        }
      } catch (e) { /* host 重启中，下一轮再试 */ }
    }, 2000);
  }

  await initScreensaver();

  // 调试/自动化用的最小句柄
  window.__cs80 = { pusher, state, renderAll, saver, editor: await import('./editor.js'), setRate: () => setRateFn };
}

// ---------------- 屏保 ----------------
async function loadSaverCfg() {
  try {
    const cfg = await (await fetch('/api/screensaver')).json();
    saver.files = cfg.files || [];
    saver.file = cfg.file || (saver.files[0] && saver.files[0].name) || '';
    saver.autoLock = cfg.autoLock !== false;
    renderSaverUI();
  } catch (e) {
    console.warn('screensaver cfg failed', e);
  }
}

async function startScreensaver() {
  if (saver.active) return;
  const res = await postJson('/api/screensaver', { action: 'play' });
  if (!res.ok) {
    toast('屏保启动失败：' + (res.error || '未知错误'), 'warn');
    return;
  }
  saver.active = true;
  pusher.paused = true; // 屏保由 host 直接推帧，浏览器别再抢
  renderSaverUI();
  toast('屏保已开始');
}

async function stopScreensaver() {
  if (!saver.active) return;
  await postJson('/api/screensaver', { action: 'stop' });
  saver.active = false;
  pusher.paused = false;
  pusher.markFull();
  renderSaverUI();
}

function renderSaverUI() {
  const sel = el('#saver-file');
  const btn = el('#btn-saver');
  if (!sel || !btn) return;
  if (sel.dataset.stamp !== String(saver.files.length)) {
    sel.innerHTML = saver.files.map((f) => `<option value="${f.name}">${f.name}</option>`).join('');
    sel.dataset.stamp = String(saver.files.length);
  }
  sel.value = saver.file;
  sel.onchange = async () => {
    saver.file = sel.value;
    if (saver.active) {
      stopScreensaver();
      startScreensaver();
    }
    await postJson('/api/screensaver', { file: saver.file });
  };
  btn.textContent = saver.active ? '退出屏保' : '屏保';
  btn.classList.toggle('primary', saver.active);
  const auto = el('#saver-auto');
  if (auto) {
    auto.checked = saver.autoLock;
    auto.onchange = () => {
      saver.autoLock = auto.checked;
      postJson('/api/screensaver', { autoLock: auto.checked });
    };
  }
}

async function initScreensaver() {
  await loadSaverCfg();
  el('#btn-saver').onclick = () => (saver.active ? stopScreensaver() : startScreensaver());
  // 锁屏 → 自动播放；解锁 → 恢复画布
  setInterval(async () => {
    try {
      const st = await (await fetch('/api/screensaver')).json();
      if (st.files && st.files.length && st.files.length !== saver.files.length) {
        saver.files = st.files;
        saver.file = st.file || saver.files[0].name;
      }
      saver.autoLock = st.autoLock !== false;
      const playing = !!(st.saver && st.saver.playing);
      if (playing !== saver.active) {
        saver.active = playing;
        pusher.paused = playing;      // 屏保期间浏览器不推帧
        if (!playing) pusher.markFull();
        renderSaverUI();
      }
    } catch (e) {
      /* host 抖动忽略 */
    }
  }, 2500);
}

boot().catch((e) => {
  console.error(e);
  document.body.innerHTML = `<pre style="color:#ef6f6f;padding:24px;font:12px/1.6 monospace">启动失败：${e.stack || e}</pre>`;
});
