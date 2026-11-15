# Rust + Skia 设备渲染器（实验分支）

> 状态：设计/骨架阶段。**没有替换现有 Chromium kiosk**，`main` 行为不变。
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

### 2.2 自定义页面（`panel/web/pages/*.js`）怎么办？

当前功能允许用户写 ES module 直接画整屏。Rust 不能直接执行 JS。选项：

| 方案 | 说明 |
|---|---|
| A. 砍掉 | 设备端只支持内置组件；自定义页面仅编辑器预览，不推设备 |
| B. Rust/WASM 页面 | 新插件 ABI；破坏现有 JS 页面生态 |
| C. 保留 Chromium 子进程 | 只让 Chromium 跑自定义页面，内置组件走 Rust；没有完全去掉浏览器 |
| D. 嵌 JS 引擎 | QuickJS/Boa + Canvas 2D shim；复杂度高，不建议 |

**建议：如果自定义页面是核心功能，先选 C 作为过渡；否则 A。**

### 2.2b 变体：Rust + JS 引擎 + Skia Canvas 2D shim

如果“单份组件代码 + 自定义页面”比“纯 Rust”更重要，还有一条路：

```text
QuickJS/Boa (JS)  ── Canvas 2D 子集 shim ──► skia-safe
```

- 复用现有 `widgets.js` / `pages/*.js` / `lunar.js` / 数据循环，不重写 17 个组件；
- 只实现实际用到的 Canvas 2D 子集：路径、文本/measureText、drawImage、渐变、虚线、
  `globalCompositeOperation`、`roundRect`、save/restore；
- QuickJS 很小（~1 MB），Boa 纯 Rust 但更重；1 Hz/5 Hz 的脚本量完全跑得动；
- 难点：JS↔Rust 桥、Promise/fetch/timer/Image/localStorage shim、文本度量、调试。

**和 Rust 重写的取舍：**

| | Rust 重写 17 个组件 | Rust + QuickJS/Canvas shim |
|---|---|---|
| 组件代码 | 两套（JS 编辑器 + Rust 设备） | 一套 JS |
| 自定义页面 | 需另定方案 | 可直接复用 |
| 开发量 | 17 个组件 + 图表/列表 + 数据循环 | Canvas shim + 运行时/异步/图片桥 |
| 长期维护 | 每次改两遍 | 改一遍 |
| 调试 | Rust 原生，简单 | JS 引擎 + FFI，复杂 |
| 风险 | 视觉漂移、双份代码 | 引擎/桥/Canvas API 覆盖 |

**建议：P0 先花 1 天做 QuickJS + Skia 的最小 spike（画一个 card + 中文 + 渐变），
和 Rust 重写的 card 对比开发量；如果 shim 顺利，优先走这条，避免永久双份维护。**

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
| P0 | 装 Rust toolchain + 跑通 `skia-safe` 最小 800×1280 面；渲染 card + 中文 + 渐变 + 圆角 | 编译成功；单帧 PNG 与 Chrome 近似 |
| P1 | 纵切：layout + data + push + `clock_dial` + `ha` | 真机 patch 成功；1 小时内存 <120 MB、CPU <5% 单核；截图 diff 可接受 |
| P2 | 简单组件：weather/crypto/github/note/timer/holiday/clock | golden test 通过 |
| P3 | 复杂组件：stock/minute/kline/news/agent/runcat/system/music | 数据/布局/字体 parity |
| P4 | 自定义页面决策 + 编辑器预览策略 | 功能不回退或明确取舍 |
| P5 | systemd 服务、回退开关、文档、性能报告 | 可替换 kiosk；保留 Chromium 回退 |

## 7. 风险

- `skia-safe` 首次构建/下载较重；交叉编译到 ARM 需要预编译或源码构建。
- 字体/emoji/SVG/`measureText` 的观感差异；必须 golden test。
- 双份绘制代码的长期维护成本（见 2.1）。
- 自定义页面（见 2.2）是最大产品取舍。
- 17 个组件 + 自定义页 + lunar/交易时段/时区，不是一次小改动；粗估 4–8 周。

## 8. 构建环境（当前机器）

当前 `cargo` / `rustc` 未安装；`skia-safe` 需要 Rust + C++ 工具链/预编译 Skia。
P0 第一件事是安装 toolchain 并确认 `skia-safe` 能在本机编译/运行；
如果本机不适合构建，需要在 CI/开发机构建后交叉验证。

## 9. 回退

- 新 renderer 作为独立 systemd 服务，不删除 `cs80-kiosk.service`；
- 通过启用/停用服务切换；layout/host/qdc1 不变；
- 如果 parity/内存不达标，直接停用新服务，回退 Chromium。
