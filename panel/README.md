# panel/ —— 客户端（画布编辑器 + 常驻 runtime）

```
桌面壳（Tauri）→ 窗口加载 http://127.0.0.1:8787/
   浏览器画布 800×1280（编辑器 = 预览 = 设备像素源）
   │ 每次只把脏的 32×16 tile 发出去
   ▼
（Linux 设备端 2026-11 起：../renderer/ 的 Rust + skia-safe 直接画帧并 POST /api/frame|patch；
下方 headless Chromium 的 kiosk 仅作回退。编辑器/自定义页面预览仍走浏览器，见 ../renderer/README.md）

host.py  ── 独占 USB 的常驻进程（HTTP :8787 + 数据源 + 屏保 + 锁屏监听）
   │ stdin 二进制消息：整帧 RGB / 局部 tile / 控屏 / 屏保整帧 / 音频（type 7·8·9）
   ▼
qdc1（C sidecar，单进程独占设备；macOS=IOKit，Windows=COM 口）
   ▼
CS80 屏（QDisplay 固件：QDC1 协议 + BGR565 framebuffer + DSI 上屏）
```

协议、硬件、刷机、排障分别见 [`../docs/protocol.md`](../docs/protocol.md)、[`../docs/hardware.md`](../docs/hardware.md)、
[`../docs/firmware.md`](../docs/firmware.md)、[`../docs/troubleshooting.md`](../docs/troubleshooting.md)。

## 跑起来

```bash
cd panel/shell && npm install && npx tauri build     # 产出 .app / .msi（推荐）
open panel/shell/src-tauri/target/release/bundle/macos/"CS80 Panel.app"

# 或者不打包：
python3 panel/host.py        # 然后浏览器打开 http://127.0.0.1:8787
sh panel/run.sh              # macOS：host + Chrome 应用窗口（会自动停掉旧的 launchd 服务）
panel\run-windows.cmd        # Windows：host + Edge/Chrome 应用窗口（无地址栏，独立 profile）
```

**必须常驻**：固件 5 秒收不到主机命令就回「等待连接」页。桌面壳把「关窗」改成「隐藏到托盘」，
并带一个 host 监督线程（host 死了自动拉起，退避 2→10 s）。

## 文件

```
qdc1.c / lz4.c / lz4.h      USB sidecar（QDC1 协议、LZ4、影子 framebuffer、保活、看门狗、音频上传/播放）
host.py                     常驻 host：HTTP API、数据源、Agent 扫描、屏保(ffmpeg)、锁屏监听、音频（任意格式→MP3→UFS）
kiosk-guard.sh              Linux 无桌面：守护 headless Chromium，页面卡住就重启（见 ../deploy/）
web/index.html  styles.css  编辑器骨架 + 设计系统（token 来自 glance 的暗色中性 + 金色 accent）
web/widgets.js              全部组件 + 卡片外壳 card() + 主题 THEME
web/pages/*.js              自定义页面（示例：自选股 + 分时图）
web/core.js                 状态、API、数据调度（按组件 ttl 轮询）
web/editor.js               gridstack 网格、组件库、属性面板、快捷键
web/pusher.js               把格子拼成整帧、tile 差分、整帧/局部推送
web/main.js                 接线 + 顶栏（亮度/待机/屏保/推送统计）
web/lunar.js                农历换算（1900–2100 位表）
web/media/                  屏保视频（内置 wallpaper.mp4）
web/logos/                  Agent logo（Pi / Claude / OpenAI / DeepSeek）
shell/                      Tauri 2 壳（见 shell/README.md）
add-screensaver.sh          把任意视频转成 800×1280 / 12 fps 放进屏保目录
```

## 组件（17 个）

数据有两层缓存：host 侧按源缓存（股票 15s / 资讯 180s…），浏览器侧把 `state.data` 落到
`localStorage`（单组件 ≤200 KB）→ **换面板、重开窗口先用旧数据画出来**（组件右上角标 `22:52 更新`），
后台按各组件 TTL 拉新的再替换。

所有列表类组件（股票 / 资讯 / Agent / 加密）行高按内容定，**高度不够就少显示几条，不压缩**。
数据拉取失败不清空：继续显示上一份，小字标黄「更新失败·重取中」，拉成功自动恢复。

| 组件 | 最小尺寸 | 数据源 |
|---|---|---|
| 时钟 | 2×1 | 本地时间 + 农历 + 右上角天气；`font` 可选 `mono`/`sans`/`lcd`（自绘七段数码管） |
| 时钟 · 表盘 | 3×2 | 刻度圈 + 左右大数字 + 走时指针 + 天气（参考 Watch 表盘风格） |
| Agent | 2×2 | 自动扫描本机 Pi / Claude Code / Codex / DeepSeek Harness 会话 |
| 股票 | 2×1 | 一格高一只（2 行 = 2 只），走 `/api/data/portfolio`，右上角小字 `18:30 更新`。**按时段自动切换三组代码**：A 股盘中（北京 9:30–15:00）= `symbols`；美股盘中（纽约 9:30–16:00）= `us`（默认 `100.SPX,100.NDX`）；其余时间（含美股盘前）= `usFut`（默认 `103.ES00Y,103.NQ00Y` 期货连续）。标题会自动变成「自选股 / 美股指数 / 美股期货」。行情接口偶发空数据时**逐只兜底**：分时拿不到就沿用上一份（30 分钟内）、快照也失败就整条沿用，不会突然变「无分时」 |
| K 线 | 2×1 | 东方财富日线 |
| 加密 | 1×1 | coingecko |
| GitHub | 1×1 | GitHub API（未授权 60 次/时） |
| 天气 | 1×1 | open-meteo（可选 macOS 快捷指令，见下） |
| 音乐 | 1×1 | macOS「音乐」App 正在播放的曲目（其它平台返回未在播放） |
| 电脑指标 | 2×1 | `host_statistics64` CPU + `vm_stat` 内存 + 网速 |
| RunCat | 2×2 / 5×2 | CPU/MEM 环 + 奔跑猫，速度随 CPU；**右上角显示本机局域网 IP**（取默认路由网卡、会跳过 Clash/FlClash 的 198.18.x TUN 和 VMware 虚拟网卡；`cfg.meta: 'host'` 可改回主机名） |
| RunCat 条 | 5×2 | 左边奔跑猫 + 右边 CPU/MEM 大数字和折线，宽度不够会自己缩 |
| 倒计时 | 1×1 | 本地计算 |
| 便签 | 1×1 | 纯文本 |
| 资讯 | 3×2 | 热榜（接口照 newsnow，共 12 个源）：今日头条 / 百度 / 知乎 / 微博 / 贴吧 / V2EX / 掘金 / 少数派 / IT之家 / 36氪 / 华尔街见闻 / Hacker News。属性面板里**勾选启用、↑↓ 调 Tab 顺序、输入框改 Tab 名字**（如「今日头条」→「头条」），`rotate` 秒数轮播；行数由高度决定；右上角小字 `22:52 更新`（全挂了才变「更新失败·重取中」，数据仍保留） |
| 假期 | 1×1 | 下一个假期倒计时（数据照 holiday-cn，按国务院公布的调休表）；四种版式按宽高自动切：宽扁=大数字 / 竖版=假期列表 / 方形=名字+大数字 / 大卡=左边大数字+右边后续假期 |
| Home Assistant | 5×2 / 2×2 | 本地 HA REST API：室内温度/湿度、室外天气、空调、灯/开关、门窗/门锁、功率、媒体；token 在 `~/.cs80_panel/config.json`，浏览器只拿 entity_id 的 state。见下 |

### Home Assistant 概览

`host.py` 读 `~/.cs80_panel/config.json` 的 `ha`（或环境变量 `HA_URL` / `HA_TOKEN`）：

```json
{
  "ha": {
    "url": "http://127.0.0.1:8123",
    "token": "HA 长期访问令牌"
  }
}
```

token 用 HA「个人资料 → 安全 → 长期访问令牌」创建，建议命名 `cs80-panel`；只存 host，不要写进
layout.json。组件 config 只放 entity_id：

```json
{
  "title": "家里",
  "temp": "sensor.xxx_temperature",
  "humidity": "sensor.xxx_humidity",
  "air": "weather.xxx",
  "climate": "climate.xxx",
  "lights": "light.a,light.b",
  "switches": "switch.a,switch.b",
  "doors": "binary_sensor.a,binary_sensor.b",
  "lock": "lock.xxx",
  "power": "sensor.xxx_power",
  "energy": "sensor.xxx_energy",
  "media": "media_player.xxx",
  "chips": "climate,lights,switches,media"
}
```

`air` 可以是 `sensor`（PM2.5/CO2）也可以是 `weather`（显示室外温度 + 天气）。
`chips` 决定中间状态条显示哪些角色；没配实体或实体 `unavailable` 会显示 `—`/灰色，HA 离线时
保留上一次数据、右上角变红。

## 默认看板（新装即用）

`host.py` 里的 `DEFAULT_LAYOUT` 就是新用户第一次打开看到的布局（没有 `~/.cs80_panel/layout.json` 时用它）：

```
 0 ┌───────────────┬────────┐
 1 │ 表盘时钟 3×2  │ RunCat │   时钟下方带天气（默认城市：北京）
   │               │  2×2   │   RunCat 右上角显示本机局域网 IP
 2 ├───────────────┴────────┤
 3 │ 自选股 5×2             │   一格一只：sh000001 上证指数 / sz399001 深证成指
 4 ├────────────────────────┤
 5 │ 热榜资讯 5×2           │   今日头条 / 百度 / 微博，60 秒轮播一个 Tab
 6 ├────────────────────────┤
 7 │ Agent 5×2             │   本机 Pi / Claude Code / Codex / DeepSeek 会话
   └────────────────────────┘
```

![默认看板](../docs/assets/default-layout.png)

改默认布局：直接改 `host.py` 的 `GRID_PAGE`（5 列 × 8 行，每格 160px，`cell = [列, 行, 宽, 高]`）。
自己的机器上点「恢复默认」会按这份配置重建（会覆盖当前布局）。

## 编辑器

- 5×8 网格（每格 160 px），画布高度固定为整屏 8 行，空格子也画线
- 添加：点组件库；移动：拖；缩放：拖四角/四边；微调 `←↑↓→`；复制 `⌘D`；删除 `⌫`
- 多画布：顶部 tab 切换（单击切换、双击改名、`＋` 新建、「删除画布」删当前页）
- 布局文件 `~/.cs80_panel/layout.json`，可导出/导入 JSON

## 多画布与自定义页面

```json
{ "version": 2, "activePage": "main",
  "pages": [
    { "id": "main", "name": "主页", "type": "grid", "widgets": [ ... ] },
    { "id": "stockpage", "name": "股票自选", "type": "custom",
      "module": "pages/stock.js", "config": { "symbols": "sh000001,sz399001", "title": "自选股" } }
  ] }
```

自定义页面就是一个 ES 模块：

```js
export default {
  needs: { kind: 'portfolio', ttl: 20000, query: (cfg) => ({ symbols: cfg.symbols }) },
  draw(ctx, size, data, cfg, theme) { /* 800×1280 设备像素，随便画 */ },
};
```

## Agent 组件

扫描各 Agent 自己落在本地的会话日志（无需埋点）：

| Agent | 会话位置 | 运行判定 |
|---|---|---|
| Pi | `~/.pi/agent/sessions/**/*.jsonl` | 进程 `pi` 存活 |
| Claude Code | `~/.claude/projects/**/*.jsonl` | 进程 `claude` 存活 |
| Codex | `~/.codex/sessions/**/*.jsonl` | 进程 `codex` 存活 |
| DeepSeek Harness | 会话在 IndexedDB 里读不到，只判进程 | 进程存在 |

- 状态色：运行中（绿，呼吸）/ 等待（琥珀，呼吸）/ 异常退出（红）/ 1 小时内结束（蓝）/ 更早（灰）
- 排序：运行中 → 等待 → 异常 → 已结束，组内按最近活动倒序；无滚动条，按高度取行数
- 配置项是两级勾选树：整组隐藏某个 Agent，或只隐藏某一条会话（`config.hide` / `config.hideTasks`）

## 音频（喇叭）

固件 0.3.0 自带音频通路（`features=0x23f` 的 bit2），PA（QL GPIO22）由固件自己开关，
所以**不需要重刷固件**，主机侧只要把 MP3 传上去再触发播放。**固件只解 MP3**，
其它格式（WAV / FLAC / M4A / AAC / OGG / OPUS / WMA / AIFF / AMR，以及 MP4/MOV/MKV 等视频音轨）
都在主机侧用 ffmpeg 转成 128 kbps MP3 再上传；`ffmpeg_path()` 会找 PATH / Homebrew 常见路径，也可用 `CS80_FFMPEG` 指定。

- UI：侧栏「设备」卡片 → **音频播放…** 打开独立面板（选文件、音量滑杆、停止、状态行含转码信息，Esc 关闭）
- `POST /api/audio/file?play=1&name=xxx.wav`：body 是原始文件字节 → 转码 → 上传 → 播放（浏览器用这个）；
  输入 ≤64 MiB，转码后需 ≤4 MiB
- `POST /api/audio`（JSON）：

| action | 参数 | 说明 |
|---|---|---|
| `query` | — | 音量 / 播放状态 / 路由（`detail=0x31445541` = AUD1） |
| `stop` | — | 停止播放并终止上传 |
| `volume` | `value` 0..100 | 设音量 |
| `play` | `file`（可选） | 无 `file` 播设备上已有文件；有则先上传（哈希相同会跳过）再播 |
| `upload` | `file` | 只上传，不播放 |
| `verify` | — | 读回设备文件大小 / XXH32 |
| `diag` | — | 固件诊断（sub 8）：路由 / 硬件档位 / PA / 增益，只读 |
| `route` | `value` 0..2 | 停止后改音频路由（0=speaker，正常值） |
| `say` | `text`、`voice`（可选）、`rate`（可选 80–400） | macOS：`say` + `ffmpeg` 合成 MP3 后播放；`voice` 传 `say -v '?'` 里的名字（zh_CN 是 `Tingting`），不传就用系统默认音色；音色没装中文数据时返回 400 而不是上传一段静音 |

实现：sidecar `0x05/0x06` → QDC1 `type 7/8`；主机按 16 KiB 分块上传并逐块等 ACK，
文件 XXH32 在 Python 里算（装了 `xxhash` 会更快），按路径+大小+mtime 缓存。
音频和画面共用一条命令流，`Device.audio_lock` 保证上传/播放不会互相穿插。

**播放自愈**：设备重启后音频路由可能被复位，`PLAY` 会回 `status=4 BAD_STATE`。
`Device.play_uploaded()` 会读诊断把路由改回 speaker、音量为 0 时补一条音量命令，再重试一次。

## 屏保

- 顶栏：下拉选视频、`屏保` 按钮、`锁屏播放` 勾选
- **画面由 host 侧 ffmpeg 解码**（`bgr565le` → sidecar 的 `0x04` 消息），锁屏后不受 WebView 节流影响，稳定 12 fps
- 素材：`panel/web/media/*.mp4` + `~/Movies/cs80-screensaver/*.mp4`
- 加自己的视频：`sh panel/add-screensaver.sh ~/Downloads/xxx.mp4 名字`（转成 800×1280 / 12 fps）

## 屏闪自检

设备侧“纯闪”的成因只有三类（固件重绘 / 背光 PWM / 面板与供电），`docs/troubleshooting.md#屏闪` 有完整对照表。
客户端只做两件事帮定位：

- **亮度会被记住，不会被自动改**：顶栏滑杆 / 设备上的 `+ -` 键设的值都写进
  `~/.cs80_panel/config.json`，host 重启、设备断电重连后会**恢复成你选的值**（固件上电默认 100，
  host 读一次状态发现不一致才下发）。想强制锁一个值：`CS80_BRIGHTNESS=100`（老行为）；
  想完全不闪就把亮度设 100 —— `<100` 时固件走 1 kHz 软件 PWM 调光，中间灰会轻微抖。
- **夜间自动调光**：`~/.cs80_panel/config.json` 里写

  ```json
  "night": {"from": "00:00", "to": "07:00", "brightness": 5}
  ```

  host 每 30 秒看一次时间，窗口内把亮度压到 `brightness`（**最低 5**，`<5` 会被拒），
  出窗口恢复 `brightness` 字段里的白天偏好；`from > to` 表示跨零点（如 `23:00 → 07:00`），
  `{"night": {"enabled": false}}` 关闭。窗口内你在顶栏/按键手动改亮度，**这一夜听你的**，
  而且不会把夜里那个值写成白天偏好。重连后被固件重置成 100 也会在下一次 tick 压回夜间值。
  状态查询：`/api/status` 的 `night`（`{"active": true, "value": 5}`）。
  自检：`python3 panel/test_night.py`。
- **纯色自检**：侧栏「设备」卡片里的 白/灰/暗/蓝/黑 → `POST /api/device/pattern {"color":"gray"}`，
  host 直接在 Python 里合成 800×1280 BGR565 整帧走 sidecar 的 `0x04`（跟屏保同一条路），
  逐色看就知道是背光/供电（全闪）还是面板静态色闪（只有中间色闪）；
  设备详情里同时显示 `背光 xx%` 与 `PWM 运行 <edges>`（`state.edges` 在涨 = PWM 没停）

## 天气数据

| 来源 | 说明 |
|---|---|
| open-meteo（默认） | 免费无 key，`geocoding` + `forecast` |
| macOS 系统天气（可选） | 系统没有公开接口；自建一个 Shortcuts 快捷指令输出 `{"temp":16,"low":15,"high":19,"code":3}`，然后 `CS80_WEATHER_SHORTCUT="天气"` 起 host |

## 已知限制

- Windows：host + sidecar + 推帧已在真机验收（见 [`../docs/windows.md`](../docs/windows.md)），Tauri 桌面壳未在 Windows 打包验证
- 音乐组件只在 macOS 可用（Windows/Linux 固定返回未在播放）
- Agent 的「等待输入」是「进程活着但 120 秒没写入」的近似，不是真状态
- DeepSeek Harness 读不到会话内容，只能显示「运行中」
- 默认每 200 ms 一次推送循环（时钟秒级）；屏保期间浏览器暂停推帧，由 host 独占
