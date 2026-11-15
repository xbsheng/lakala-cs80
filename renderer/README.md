# Rust + Skia 设备渲染器（实验分支）

> 状态：设计 + P0 spike 完成。**没有替换现有 Chromium kiosk**，`main` 行为不变。
> 分支：`feat/rust-skia-renderer`

## 1. 目标与非目标

**目标**

- 用 `skia-safe` 在 Rust 里直接渲染 800×1280 的 BGR565 设备帧，经 host 的
  `/api/frame` / `/api/patch` 推到 `qdc1`，彻底去掉设备端的 Chromium。
- 预期内存：当前 renderer 约 894 MB RSS / 834 MB PSS → **40–80 MB**；
  CPU 不再有浏览器 compositor/network service；没有 2 MB `/tmp` shm fd 泄漏。
- 启动快、无 Chromium 版本升级/GPU/字体子系统差异；systemd 管一个二进制。

**非目标（第一阶段）**

- 不改 `host.py` / `qdc1` / USB 协议。
- 不改编辑器交互（gridstack、属性面板仍用浏览器）。
- 不追求一次移植全部 17 个组件；先做纵切验证。

## 2. 必须先拍板的三个问题

### 2.1 两套绘制代码怎么维护？

设备端 Rust 重写后，编辑器里的 `panel/web/widgets.js` 仍是 JS。每个组件的视觉改动
要改两次，且有漂移风险。三种做法：

| 方案 | 说明 | 代价 |
|---|---|---|
| A. 接受双份 + golden test | JS 编辑器、Rust 设备各一份；CI 对每个组件做 Chrome vs Rust 截图 diff | 每个组件双改；长期维护成本高 |
| B. 编辑器预览也走 Rust | Rust renderer 提供 `/preview.png`（或帧流），编辑器只做拖拽/属性面板 | 需要预览服务；交互预览延迟；但视觉单一来源 |
| C. Rust 变成唯一实现 | 逐步废弃 JS `widgets.js`，编辑器只显示 Rust 渲染图 | 最大改造；自定义页面受影响 |

**建议：先按 A 做纵切验证；如果视觉/维护成本可接受，再决定是否转 B。**

### 2.2 自定义页面：已决定用 Rust 实现

不再保留 `panel/web/pages/*.js` 这种 ES module 页面。自定义页面改为 Rust `Page` trait +
registry，编译进 renderer：

```rust
pub trait Page {
    fn needs(&self, cfg: &Value) -> Vec<Need>;
    fn draw(&self, c: &mut Canvas2D, size: Size, cfg: &Value, data: &Value, t: &Theme);
}
```

- 现有 `pages/stock.js` → `src/pages/stock.rs`，作为第一个 Rust 自定义页面。
- layout schema 从 `{type:"custom", module:"pages/stock.js"}` 迁移到
  `{type:"rust", page:"stock", config:{...}}`；`host.py`/编辑器要兼容旧 schema（P4）。
- 编辑器不再执行自定义页面 JS；自定义页面 tab 显示 Rust renderer 的
  `GET /preview.png?page=stock&config=...` 预览图，并编辑 config JSON。
- 页面注册表是静态的（编译期），不是运行时用户上传代码；以后要动态加载，
  再考虑 `.so`/WASM 插件 ABI。

> 已否决：QuickJS/Boa + Canvas shim。自定义页面确定用 Rust，不再保留 JS 页面运行时；
> 这样可以少一个 JS 引擎/FFI/异步桥。内置组件的编辑器预览仍是 JS（见 2.1）。

### 2.3 编辑器与屏上一致性

- 必须做 **golden image 测试**：同一份 layout/data，在 Chromium 和 Rust 各渲染一次，
  比较 pixel diff/SSIM。
- 字体必须统一：建议 bundle `Inter` + `Noto Sans SC` + `JetBrains Mono`，
  浏览器用 `@font-face`，Rust 用 `Typeface::from_data`。这会改变当前 DejaVu/文泉驿的观感。
- SVG logo 预栅格化成 2x/3x PNG；`⚡` 等 emoji 换成路径/PNG；
  `roundRect` 用 `arcTo` 自绘，避免退化成方角。
- `measureText` 的度量差异会移动 `fitFont`/省略号；golden test 必须覆盖长中文/英文/数字。

## 3. 架构

```text
~/.cs80_panel/layout.json
        │  poll /api/layout (2s)
        ▼
cs80-renderer (Rust + skia-safe)
  ├─ layout model (serde)
  ├─ data scheduler  ── GET /api/data/<kind>?… ──► host.py
  ├─ widgets/* (Rust)
  ├─ Skia raster surface 800×1280 (N32 premul)
  ├─ BGRA → BGR565, 32×16 tile diff
  ├─ POST /api/frame (RGB888, 首次/布局变)
  └─ POST /api/patch (二进制 tile, 后续)
        │
        ▼
host.py ── stdin ──► qdc1 ──► CS80 屏
```

**保持不变的边界**：host 继续独占 USB、做数据缓存、屏保、音频、亮度；
Rust renderer 只是一个“画帧并提交”的客户端，和现在的 kiosk 页面同级。

**屏保**：renderer 轮询 `/api/status`，`saver.active` / `screen` 进入屏保时暂停推送，
否则会盖掉 host 直接推的屏保帧。

## 4. Crate 结构（计划）

```text
renderer/
  Cargo.toml
  src/
    main.rs          # 参数、循环、layout/status 轮询、退出/暂停
    layout.rs        # serde: Layout/Page/Widget/Cell/Config
    data.rs          # host API、TTL 调度、JSON 缓存、失败保留旧值
    push.rs          # BGR565、整帧/局部、HTTP keep-alive、重试
    canvas.rs        # Skia Surface + Canvas2D 薄封装（fillText/measureText/path/rrect…）
    theme.rs         # THEME 颜色/字体
    assets.rs        # 字体、PNG、SVG→PNG、RunCat、agent logo
    lunar.rs         # lunar.js 1900–2100 表移植
    widgets/
      mod.rs         # Widget trait + registry
      clock.rs clock_dial.rs weather.rs stock.rs kline.rs crypto.rs
      github.rs music.rs timer.rs note.rs news.rs holiday.rs
      ha.rs agent.rs system.rs runcat.rs runcat_bar.rs
    pages/
      mod.rs         # Page trait + registry
      stock.rs       # 原 pages/stock.js
```

**Widget trait 草案**

```rust
pub trait Widget {
    fn needs(&self, cfg: &serde_json::Value) -> Vec<Need>;
    fn draw(&self, c: &mut Canvas2D, r: Rect, cfg: &Value, data: &Value, t: &Theme);
}
```

`Need` 对应现在 `widgets.js` 的 `needs: { kind, ttl, query }`；`data` 是 host 返回的 JSON。
配置用 `serde_json::Value` 透传，各 widget 自己反序列化，避免为一个组件改全局 schema。

## 5. 渲染循环

- **基础 1 Hz**；只有 RunCat/表盘秒针/交互时才 5 Hz。Rust 没有浏览器 tick 开销，
  但仍避免无意义重绘。
- 首次、layout 变化、patch 失败：整帧（`POST /api/frame`，RGB888 一次性 3 MB）。
- 其余：复用两块 `Vec<u16>`（prev/cur），按 32×16 tile diff，`POST /api/patch`。
- 每次只分配必要缓冲；避免像浏览器 pusher 那样每 tick `getImageData`/`toBgr565` 新分配。
- HTTP 用 keep-alive，patch 很小；不再有 Chromium 网络服务的 2 MB shm。

## 6. 迁移阶段

| 阶段 | 内容 | 验收 |
|---|---|---|
| P0 | ✅ 已完成：Rust + skia-safe 最小 800×1280 面；card + 中文 + 渐变 + 圆角 + measureText | 单帧 PNG 正常；release 3.7 MB / 13 MB RSS |
| P1 | 纵切：layout + data + push + `clock_dial` + `ha` | 真机 patch 成功；1 小时内存 <120 MB、CPU <5% 单核；截图 diff 可接受 |
| P2 | 简单组件：weather/crypto/github/note/timer/holiday/clock | golden test 通过 |
| P3 | 复杂组件：stock/minute/kline/news/agent/runcat/system/music | 数据/布局/字体 parity |
| P4 | Rust 自定义页面：Page trait/registry、`stock.rs`、layout schema 迁移、编辑器 `/preview.png` | 自定义页面功能可用；旧 schema 可迁移 |
| P5 | systemd 服务、回退开关、文档、性能报告 | 可替换 kiosk；保留 Chromium 回退 |

## 7. 风险

- `skia-safe` 首次构建/下载较重；交叉编译到 ARM 需要预编译或源码构建。
- 字体/emoji/SVG/`measureText` 的观感差异；必须 golden test。
- 双份绘制代码的长期维护成本（见 2.1）。
- 自定义页面改为 Rust 后，layout schema/编辑器预览要迁移；旧 JS 页面不再执行。
- 17 个组件 + 自定义页 + lunar/交易时段/时区，不是一次小改动；粗估 4–8 周。

## 8. 构建环境与本机实测

本机已装好：

- `rustc 1.100.0` / `cargo 1.100.0`（rustup minimal，`RUSTUP_HOME=/root/.rustup`、`CARGO_HOME=/root/.cargo`）
- `clang 18.1.3`、`libclang-dev`、`ninja 1.12.1`、`libfreetype-dev`、`libfontconfig1-dev`、`pkg-config`、`python3`
- `skia-safe 0.157.0` + `skia-bindings 0.157.0`：本机没有命中预编译 binary cache，
  走的是源码构建；首次构建较慢，`target/` 约 1 GB。
- 构建需要代理时用 `https_proxy=http://127.0.0.1:7890`（本机 mihomo）；仓库里不提交代理配置。

```bash
cd renderer
export PATH=/root/.cargo/bin:/usr/local/bin:$PATH
cargo build --release
./target/release/cs80-renderer
```

当前 `src/main.rs` 是 **P0 spike**：画一张 800×1280 的 card，验证中文、圆角、渐变、
`measure_text`；还没有接 host/设备。P1 会把它替换为真正的 layout/data/push 主循环。

### P0 实测

| 项目 | 结果 |
|---|---|
| 面 | 800×1280 N32 premul，PNG 输出正常 |
| 中文 | 系统 WQY ZenHei，`室内温度`/`17.4°C` 正常 |
| 圆角/渐变 | `RRect` + `Gradient` 正常 |
| release 二进制 | 3.7 MB（动态链接 fontconfig/freetype） |
| release 运行 | 约 0.04 s，max RSS **13.2 MB** |
| debug 二进制 | 23 MB，max RSS 14.5 MB |
| 构建 | release 在已有 Skia 构建后约 4 分钟；`target/` 约 1 GB |

## 9. 回退

- 新 renderer 作为独立 systemd 服务，不删除 `cs80-kiosk.service`；
- 通过启用/停用服务切换；layout/host/qdc1 不变；
- 如果 parity/内存不达标，直接停用新服务，回退 Chromium。
