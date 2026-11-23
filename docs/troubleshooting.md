# 排障：现象 → 原因 → 处理

## 屏上一直是「等待连接」页

固件 `QD_HOST_TIMEOUT_MS = 5000`：**5 秒收不到主机任何命令就自己重画等待页**。按顺序查：

| 检查 | 命令 / 位置 |
|---|---|
| 主机有没有在推帧 | `curl 127.0.0.1:8787/api/status` → `last` 应该是 `patch` / `frame` |
| host 有没有在跑 | 同上，`device` 应为 `up`；`connecting`/`down` 见下一节 |
| 桌面壳窗口是不是被关了 | 关窗只是隐藏到托盘；**退出 App 就没人推帧** |
| 客户端页面是不是还在 | 屏保期间由 host 推；非屏保时必须有一个页面在跑（见「前端没更新」） |

## `device` 不是 `up`

| `device` / `reason` | 含义 | 处理 |
|---|---|---|
| `down` + `open-failed` | 系统里根本没有 `2c7c:0901` | 换 Type-C **数据**线、换口、接 DC 12V，然后点顶栏设备标签重连 |
| `down` + `no-ack-patch` / `no-ack-keepalive` | 设备在，但不应答（固件卡死/重启中） | 等 1–5 秒自动重连；不动就按设备「菜单」键或断电重上 |
| `no-sidecar` | `panel/qdc1` 不存在或没编译 | 按 `docs/windows.md` / README 第 4 步编译 |
| `host-down` | 页面连不上 host | host 没起来或端口不对 |

`/api/status` 的 `session` 每次 sidecar 重连都会 +1；前端看到 `session` 变化会**整帧重推**。

## 屏每隔几分钟/十几分钟闪回「等待连接」，几秒后又自己恢复

先分清两种「断开」：

| 证据 | 含义 | 处理 |
|---|---|---|
| `dmesg -T` 有 `USB disconnect` / `new high-speed USB device`，`/api/status` 的 `session` 也涨 | **真 USB 掉线**（重新枚举） | 查 Type-C 数据线 / DC 12V / 别挂 HUB（按上面 `device` 表） |
| `session` 不涨，`sidecar.log` 出现 `slow patch ack ... status=8 ... rx≈3000000 dec=0 draw=0` | 固件 3 s 没把命令收全 → 5 s 后画等待页 → 重试后恢复 | 本节的 512 字节 ZLP 坑 |

2026-11 Ubuntu 实测：`clen=3552` 的 patch + 32 B QDC1 头 = **3584 = 7×512**，正好是
USB 高速度 OUT 端点 `wMaxPacketSize=512` 的整数倍。HCD 会补一个**零长度包（ZLP）**结束
这次 bulk 传输；固件的 QDC1/CDC 接收路径不认 ZLP，于是整条命令一直等到 3 s 接收超时，
回 `status=8`。ACK 里的 `rx≈3.0 s`、`dec=0`、`draw=0` 就是证据（设备根本没开始解码/绘制）。
这 3 s 里没有新命令，超过固件 `QD_HOST_TIMEOUT_MS=5000` 就画等待页；重试或整帧恢复后
看起来就是「自动重连」。整帧 LZ4 的 `clen=16864`（32+16864=33×512）是**同一个坑**，
不是 LZ4 解码器。

**已修**（`panel/qdc1.c` 的 `raw_write_all()`）：一次写的总长度是 512 整数倍时，拆成
`len-1` + `1` 两次写。QDC1 是字节流，设备侧无感；日志会留一行
`cs80: split write 6656 B (512-multiple, avoid ZLP)`。
验证：拆包后同尺寸 patch 3 ms 内 `status=0`，`rx` 回到几十 µs，`session` 不涨。

> 如果 `dmesg` 里持续出现 `USB disconnect` / `device number` 变化，那是真的 USB 掉线，
> 按上面 `device` 不是 `up` 的表查线/供电/HUB；本节只解决「USB 没断但屏回到等待页」。

## 画面一卡一卡 / 几秒到几分钟不更新（Linux 无桌面）

按这个顺序看，三条都踩过：

1. **kiosk 页面被 Chromium 冻结/节流**。headless 页面也会被当成「后台/被遮挡」：先节流到 1 s，
   5 分钟后降到 1 次/分钟，最后页面生命周期冻结。特征：`ps` 里 renderer CPU 0%，
   `/api/status` 的 `last` 停在几分钟前，但进程都在。
   - 修：kiosk 改用 Playwright 的 **chrome-headless-shell**（没有窗口/遮挡），unit 带
     `--disable-background-timer-throttling --disable-backgrounding-occluded-windows
     --disable-renderer-backgrounding --disable-features=IntensiveWakeUpThrottling`；
     再加 `panel/kiosk-guard.sh`：页面每 5 s POST `/api/heartbeat`，`beat_age` 连续两次 >30 s 就重启 Chromium。
2. **USB 全速（12 Mbps）**。`/api/status` 的 `usb.speed` 是 12 而不是 480 时，整帧 2 MB 要 1.5~2.5 s，
   （本机实测：拔插一次 Type-C 后从 12 恢复到 480，所以先怀疑 HUB/线/枚举，不要先改代码）
   60 秒一次的整帧兜底就是每分钟冻结一次。修：整帧兜底改 10 分钟；`CS80_RAW_FULL` 自动打开
   （全速下裸块比「LZ4 等 3 s 超时 + 再发裸块」快）；更根本的是**直插主机 USB 口**，别挂两层 HUB。
3. **host 的发送队列造成 patch 洪水**。设备偶尔 3 s 才回 ACK（固件 `status=8` 接收超时）；
   如果把积压的几十条 patch 一口气灌回去，固件会连续超时，越积越多，最后回到「必须拔插」的状态。
   - 修：host 队列只有 4 条、满了直接返回 `ok:false`（前端保留 `prev`，下一轮合并重试）；
     sidecar 每成功一条 patch 后 sleep 50 ms；同一个 patch 失败（`status=8/5`）原地重试 3 次，
     最终失败只回滚影子 framebuffer，不立刻推 2 MB 整帧。

`/api/status` 出现 `needs_replug: true` 说明连续 6 次推帧都失败：**拔插 Type-C（或断一下 DC 12V）**。
不要用软件 `USBDEVFS_RESET` / `authorized` 开关去复位 —— 实测可能把固件那半边 USB 功能复位死，
设备直接从总线上消失，只能物理拔插。

## 拔插 Type-C 后一直连不上（**macOS 专有，仍在定位**）

**现象**：拔掉 Type-C 再插回，屏上停在「等待连接」页（不是黑屏，说明 APP 还在跑），Mac 侧
`ioreg -p IOUSB` 里没有 `Android@`、kernel 日志也没有任何 USB 事件，`sidecar.log` 刷
`打不开设备 (code=-1)`。拔一次 DC 12V 能恢复。

**已排除「设备侧」**（2026-11 实测）：同一台设备、同一套接法（**DC 12V 一直插着**），
在 Windows 上只拔插 Type-C 就能自动重连，不需要碰电源 → 模组在 DC 供电下是支持重新枚举的。
所以这是 **macOS 侧**的事，客户端可以修。

**当前怀疑（待验证）**：`qdc1.c` 的 `open_link()` 用 `USBDeviceOpen()`（**整机独占**）+ `SetConfiguration(dev,1)`
去打开设备，而 attach 时 `AppleUSBCDCCompositeDevice` 也在抢同一个整机（日志里 Chrome / 豆包 / 夸克网盘
也在抢，`openGated: provider is already opened for exclusive access`）。首次 attach 我们抢到了所以能用；
重新 attach 时复合驱动先绑定，我们那套要么被拒、要么把配置重下一遍 → QDC1 那一半不再应答。
Windows 上 Quectel 驱动只 open 一个 COM 口，不碰整机配置，所以没这个问题。

**修法方向**：不碰整机（去掉 `USBDeviceOpen`/`SetConfiguration`），直接按 `bInterfaceNumber = 32`
开接口，顺带不再和 Chrome/豆包/夸克抢整机。

**已做的容忍**：`qdc1.c` 握手改成**最多重试 10 s**。刚插上线时固件要 2–5 s 才应答 QDC1，
以前第一条 HELLO 超时就 `link_lost` → 整个 sidecar 白重启一轮（2026-11 实测：插上后枚举成功，
但 `endpoint 0x8b` 三次读超时 → `no-ack-data`，重启后即正常）。

## 前端改了但屏上没变

这是最容易误判的一类。屏上的内容来自**桌面 app 窗口里的页面**，而 app 用的是打包副本：

- 打包版：host 服务的是 `CS80 Panel.app/Contents/Resources/panel/web/` 的**拷贝** → 改完 `panel/web/**` 要么
  `rsync -a panel/web/ ".../Resources/panel/web/"`（host.py 改动还要重启 host），要么 `npx tauri build` 重新打包
- 新增 `web/**` 文件还要同步 `panel/shell/src-tauri/tauri.conf.json` 的 `bundle.resources`（显式列举）

现在有兜底：`/api/status` 带 `build`（web 文件 mtime 汇总），页面发现 `build` 变了会**自己 reload**，
静态资源也带 `?v=<build>` 绕过 WebView 缓存。

自查顺序：`curl /api/status | grep build` → 与 `curl /widgets.js | grep <新代码>` 对比。

## 多实例抢 USB

`panel/qdc1` 独占设备。同时起两个（比如手动 host + 桌面壳自己的 host）会出现「一个能用、一个报错」。
`panel/run.sh` 和桌面壳启动时都会先 `launchctl bootout com.cs80.panel`；
手动调试时先 `pkill -f cs80-panel`（壳会拉起自己的 host，也要一起停）。

`host.py` 每次 spawn sidecar 前会 `pkill -f <sidecar 路径>` 清掉上一次 host 被 kill 时留下的孤儿进程
（macOS 上它握着整机独占，新 sidecar 只会一直报「打不开设备 (code=-1)」）。

## 屏闪

先分清「静态页也在闪」和「只有动的东西才裂」。设备上会重绘静态画面的**只有固件自己画的等待页**，
所以对着「等待连接」页看最准：

| 现象 | 原因（固件源码） | 客户端能不能解决 |
|---|---|---|
| 亮度 < 100 时整屏微闪 | `backlight.h`：硬件 PWM0（pin 70）没接背光，固件用 **1 kHz ISR 开关 GPIO8** 做软件调光；`duty=100` 时 PWM 完全停 | 想要完全不闪就把亮度设 100；`/api/status` 里 `state.edges` 一直涨 = PWM 正在跑。**host 不会自己改亮度**（见下） |
| 等待页**每秒闪一下**，能看到两位倒计时（90→0） | **0.3.1 固件** `waiting_countdown()`：每秒把数字写进 framebuffer 后 `submit_frame()` —— 整帧 2 MB D-cache clean + 写 `0x08807038` 触发 DSI 提交；单缓冲、无 vblank → 每秒一次闪/裂 | ⚠️ 只能改固件（见下方「等待页补丁」），或者让 host 常驻：固件 5 s 收不到命令才画等待页，保活不断就永远不画 |
| 等待页**完全没变化**却有小幅「颜色在抖」 | **0.3.0 固件**（本仓 `qdisplay_native.bin`，`特性位 0x23f`）：等待页只 `submit_frame()` 一次，代码里没有任何定时重绘 → 往下一层找：SC7705 静态色闪（上游 2026-10 README 明写「未完全解决」）、只靠 Type-C 供电带不动 10.1" 背光、FPC 接触 | ❌ 客户端无解，按下面 4 步定位（最可能是供电/面板） |

判断顺序（每步 30 秒；纯色自检 + 暂停推送就是为这个加的）：

> **实测（2026-11，本机 Windows，固件 0.3.0 / `features=0x23f`）**：
> 亮度 95 时 `state = {brightness:95, backlight:1, edges:9877196}` 而且 `edges` 每秒涨 ~1000 →
> 1 kHz 软件 PWM 在跑，屏上有可观察的微闪；用 `HELLO flags=11` 设到 100 后立刻变成
> `{brightness:100, backlight:0, edges:9893090}` 且 3 秒后 `edges` 一点不动 → PWM 完全停、微闪消失。
> 亮度存在固件 RAM 里，重新上电回到 100；但只要有人（`−` 键或上位机）调低就会又跑 PWM。

1. `/api/status` → `state.brightness`、`state.edges`。`edges` 在涨 → PWM 还在跑，把亮度打到 100。
2. 侧栏「纯色自检」依次点 白 / 灰 / 暗 / 蓝 / 黑（`POST /api/device/pattern`）：
   每种颜色都闪 → 背光/供电；只有灰、暗蓝这类中间色闪 → 面板 dither/静态色闪；都不闪 → 闪来自固件重绘或我们推帧的提交瞬闪。
3. 插上 **DC 12V**（别只靠 Type-C）重看一遍 —— 背光是这块屏最大的负载。
4. 顶栏「暂停推送」：屏上定格后还闪 → 不是我们的提交；不闪 → 每次 PATCH 触发的整帧提交都在闪（单缓冲的已知限制）。

### 等待页补丁（0.3.1+）

`win-flash/flash/patch_waiting_page.py` 只改 1 个字节：把 `waiting_countdown()` 里的
`if (seconds == previous) return;` 比较指令 `cmp r3,r6`（`b3 42`）改成 `cmp r3,r3`（`9b 42`，恒等）→
每次都走 return，不再画倒计时、不再每秒整帧提交。等待页从此是真静态（倒计时数字直接不出现，0.3.0 也没有）；
90 s 后背光自动关闭、按键调亮度等逻辑都不动。APP2 容器头不改，刷机仍然只写 `0x60260000` 的 APP 区。

```bat
cd win-flash\flash
py\python.exe patch_waiting_page.py qdisplay_native.bin   :: 官方 0.3.1 是 155456 B
py\python.exe qdisplay_app.py qdisplay_native_static.bin --auto
:: 回退：拿原 bin 再刷一次
```

本仓 `win-display/qdisplay/qdisplay_native.bin` 是 **0.3.0**（154400 B，`特性位 0x23f`），
里面根本没有倒计时那段代码 → 脚本会直接告诉你「不用打补丁」。

### 深挖：面板驱动 / MIPI 时序（已定位：`dsi_pclk_rate`）

亮度 100（`backlight=0`、`edges` 冻结）、DC 12V、等待页只提交一次，**屏上仍然闪**，
纯色自检结果：

| 整屏纯色 | 闪？ |
|---|---|
| 白 `#ffffff`、黑 `#000000`、红、蓝 | 不闪 |
| 灰 `#808080`、暗 `#181a20`、藏青 `#0f2033` | 闪 |
| 绿 | 轻微 |

→ 典型的**中间灰电平抖动**（极值两端不闪），跟 VCOM/反转/刷新率有关，与我们在推什么帧无关。

拿到的官方资料（都来自 QuecPython 官方库）：

- `SC7705_10.1.pdf`（Sitronix SC7705 137 页 datasheet）
- `sc7705.py`（官方驱动，**两套** 800×1280 init：`init_800X1280` / `init_800X1280_2`）

由此可以排除/确认几件事：

1. **不是 dither**：`CDh` 是 `DGC_R`（首字节 bit1=`DITH_EN`、bit0=`DGC_EN`，默认 `0x04`）。
   原厂 init 写的是 `CD 00 01 00`（数据 `0x00`）→ dither + DGC 都关。
   上游那个 `--disable-panel-dither` 实验在这块屏上其实是空操作。
2. **VCOM 改不了**：datasheet 写明 VCOM 走 OTP（4 次 MTP 烧录）由工厂标定，
   运行时只能通过 vendor 寄存器微调（未公开）。
3. **我们的 init = 原厂 R05 dump**（= 官方 `init_800X1280_2` + `CDh 00` + 原厂 `E0` gamma），
   所以面板驱动参数本身是出厂值；**不一样的是 MIPI 消隐**：

   | | h_sync | hbp | hfp | vsync | vbp | vfp | htotal×vtotal |
   |---|---|---|---|---|---|---|---|
   | QDisplay | 20 | 40 | 40 | 3 | 11 | 16 | 900×1310 |
   | 原厂/QuecPython `mipi_init` | 120 | 140 | 140 | 113 | 140 | 156 | 1200×1689 |

   另外实测本固件 `submit_frame()` 每次 **≈98 ms**（≈10.2 Hz）——很像帧周期而不是 cache clean，
   如果真是帧周期，那屏实际只有 ~10 Hz 刷新，中间灰闪就说得通了。

**已封档的实验**（2026-11，本机实测）：

| 试过 | 结果 |
|---|---|
| 把 MIPI 消隐换成原厂 R05 那套（120/140/140/113/140/156） | 刷进去能启动、但**面板锁不住 → 黑屏**；说明这颗 SoC 的 DSI PLL 不会按 `frame_rate+totals` 重算 |
| `frame_rate` 60 → 20 | 闪的节奏**变了（更快）**，证明闪跟随刷新率；但 255 是 `movs #imm8` 的上限，靠它压不住 |
| 面板 init 的 8 个单字节变体（`B4`/`B6`/`BA[6]`/`E3`/`B1`/`CDh`/`BF`/`B8`） | 全部**没有区别**（仍然只有白色不闪），说明这些寄存器不是这个闪的开关 |
| `dsi_pclk_rate` 100 → 50 | 无效 —— **但这次测错了指标**，见下 |

#### 修复：`dsi_pclk_rate` 100 → 200（issue #4）

~~得出结论：中间灰闪属于面板本体，VCOM 走 OTP，软件改不了~~ —— 至少在 **DSI 像素时钟**这个
维度上是错的。真正的开关是 `mipi_init` 的最后一个常量 `dsi_pclk_rate`（QDisplay 私有参数，
QuecPython 的 `mipi_init` 没对外暴露），文件偏移 `0x1690`，改 1 字节 + 重算 APP2 头部 CRC32：

```bat
cd win-flash\flash
py\python.exe patch_pclk.py qdisplay_native.bin 200
py\python.exe qdisplay_app.py qdisplay_native_pclk200.bin --auto
:: 回退：把原来的 qdisplay_native.bin 再刷一次
```

旧结论错在两处：

1. **只往下试了 100 → 50，从没往上试**。有效区间在 **200~210**；195 以下更差，220/240 又变差
   （不是越大越好，`movs #imm8` 上限是 255）。
2. **测的是 `submit_frame()` 耗时**，那是内存搬运，跟闪烁无关；该看的是**闪烁幅度**。

本机复测（2026-11，0.3.0 = 154400 B，亮度 100 / `backlight=0` / 12V，`flicker_probe.py` 推
静止纯色）：`#808080` 改前肉眼明显闪、改后看不出；`#c0c0c0` / `#ffffff` / `#000000` 同样静止
都不再复现闪烁；`features` 仍是 `0x23f`，patch 耗时仍在 40~50 ms 区间（与改前同档）。

0.3.1（155456 B）的 `mipi_init` 常量块逐字节相同，`patch_pclk.py` 能正常定位并出包
（`0x1690` 同样是 `0x64`），但那版固件没有刷机实测。

⚠️ 边界：只验证了 0.3.0 + 这一块屏；pclk 翻倍 = DSI 时钟翻倍，功耗 / EMI / 长时可靠性没有数据。
**一键刷机已经默认打这个补丁**（`--pclk 100` 可关），仓库里的原 bin 不动。物理上它更像把 SoC 的 DSI 扫描节奏挪进面板的舒适区
（掩盖采样失配），不等于把 VCOM/OTP 的问题修好了 —— VCOM 那条结论仍然成立。

固件侧另外落地的还是「等待页换成纯黑纯白」（见 [`firmware.md`](firmware.md) 1.6，工具
`make_waiting_page.py` + `patch_waiting_page_image.py`），因为纯黑/纯白本来就不闪。

## 屏一直停在「等待连接」页，而且亮度调节窗反复弹出

2026-11 实测定位到两个叠加的 bug（都已修）：

1. **整帧 LZ4 单块超过 512000 字节 → 固件静默丢包**。`valid_header()` 要求 `DATA.length<=512000`，
   带走势图/图片的整帧很容易压出 600 KB~2 MB，结果固件一个 ACK 都不回 → sidecar 记成
   `no-ack-data` 退出 → 永远没有整帧提交 → 屏就一直停在等待页。
   修法：`qdc1.c` 的 `push_full()` 改成**每 512000 字节一块**分别 LZ4、分别 `DATA`。
2. **亮度 OSD 反复弹**：host 以前每连上一次就盲发一次「设亮度 100」，而 sidecar 在 (1) 里不停重启
   → 每次重启都弹一次 1.8 s 的亮度窗。修法：连上后先**读状态**（`HELLO flags=10`），
   只有当前亮度不等于 100 时才设一次。

验证：修完后 `session` 保持 1 不涨、`last=patch ok=1 code=0`（每次 ~5 KB / ~100 ms）。
日志里现在也会带状态码：`链路丢失 (no-ack-data, status=8)`（8 = 设备接收超时）。

## 整帧推不上去：`no-ack-data status=8`，而且**同一画面必现**

> **2026-11 更新：根因已定位，不是 LZ4 解码器。** 日志里的 `clen=16864` + 32 B 头 =
> `16896 = 33×512`，正好是 USB 端点 `wMaxPacketSize` 的整数倍 → HCD 补的 ZLP 被固件
> 接收路径漏掉，3 s 后回 `status=8`。`qdc1.c` 的 `raw_write_all()` 已把这种长度拆成
> `len-1` + `1` 两次写，**新 sidecar 不会再因为这个原因失败**。下面的裸块兜底
> 仍保留，防其它接收异常。

2026-11 又抓到一类（和上面「单块 >512 KB」不是同一个）。当时的判断是**固件 LZ4 解码遇到某些
数据会直接回 `status=8 TIMEOUT`**，跟画面大小无关；后来（见上面的更新）确认是 512 整数倍 ZLP。
当时的特征：

- 同一个页面/组件，推帧必挂；换个页面（哪怕内容更复杂）就正常 → 看起来像"某个组件把设备搞坏了"
- 日志固定同一块失败：`cs80: data fail block=0 raw=512000 clen=16864 status=8 mode=lz4`
- sidecar 每次重启后又能握手、收 state、收 patch，只有那块 DATA 不过 → 设备没死，是解码器拒了这包
- 压缩后反而更小（16 KB < 512 KB），所以不是长度超限

**处理（已内置）**：`qdc1.c` 的 `push_full()` 现在失败会**原地重试**（不重启 sidecar），
重试顺序改成 **LZ4 → 裸块（`DATA flags=0`，2 MB，~200 ms）→ 再 LZ4**，
并且**记住踩坑的块**（原始像素哈希）+ 设 2 分钟「直接走裸块」窗口：

```
cs80: data fail block=0 raw=512000 clen=16864 status=8 mode=lz4   ← 第一次踩坑 ~3 s
cs80: full frame retry 1 (raw=0, 3009ms)                          ← 这次之后走裸块
```

固件对这块不 ACK 时它自己的接收超时就是 3 s，所以第一次踩坑约 3 s，之后同类画面直接发裸块（~0.2 s）。
只有裸块也失败才会 `link_lost`。

**配套的拖动体验**：拖动/缩放组件时**不再推帧**（`editor.js` 只更新编辑器，松手时才 `markFull()` 推一帧）。
以前拖一下要推十几帧、每帧还可能踩上面这个 3 s 的坑，屏上就会滞后好几秒。

顺带：`host.py` 对"刚起来就死"的 sidecar 做了指数退避，设备类提示（"设备重连"）30 秒内只弹一次。

## 锁屏后画面卡成幻灯片

已修。原因：浏览器播视频时，窗口被锁屏盖住 → 页面 `hidden` → WebView 把 `setInterval` 节流到 ~1 s。
现在屏保由 **host 侧 ffmpeg** 解码推帧，不受窗口可见性影响。如果你在别的实现里遇到同类问题，记住这条。

## 股票/期货有价格但没有分时，或整块显示「加载中…」

**先看数据源，不是渲染问题。** 这台 Linux 上实测（2026-11）：

- 东财 `push2*.eastmoney.com` 从主机直接请求是 **Empty reply from server**（没有 HTTP 响应），
  代理规则也没救回来；浏览器里看到的期货价格/名称是上一次成功抓取后缓存的。
- 新浪 `hq.sinajs.cn` / `stock2.finance.sina.com.cn` **直连正常**。
- Yahoo `query1.finance.yahoo.com` 直连 403，走本机 mihomo（`127.0.0.1:7890`）才 200。

- 新浪的 `gb_$inx` 里 `$` **不能被 URL 编码**（`%24` 会被当成另一个代码返回空串）；裸 6 位代码要先补 `sh`/`sz`。
- `host.py` 给东财加了 **60 秒熔断**：失败一次就这段时间不再试，直接把机会让给新浪，免得每条请求都等 6 秒 TLS 握手。

所以 `host.py` 的行情兜底是：

| 数据 | 首选 | 兜底 |
|---|---|---|
| 外盘期货快照 | 东财 `push2delay` | 新浪 `hf_ES` / `hf_NQ`（`103.XXX00Y` → `hf_XXX`） |
| 外盘期货分时 | 东财 `trends2` | 新浪 `GlobalFuturesMinLine` |
| A 股分时 | 东财 `trends2` | 新浪 `CN_MarketDataService` |
| US 指数快照 | 东财 | 新浪 `gb_$inx` / `gb_$ndx` |
| US 指数分时 | 东财 `trends2` | 新浪 `US_MinlineService.getMinline`（390 点 + 昨收，`100.SPX`→`.INX`） |

快速验证（不用看屏）：

```bash
curl -s '127.0.0.1:8787/api/data/portfolio?symbols=103.ES00Y,103.NQ00Y' | head -c 300
curl -s '127.0.0.1:8787/api/data/minute?symbol=sh000001' | head -c 200
```

`points` 为空/报错才是数据源问题；有 `points` 但屏上是「无分时」再查前端。

## 中文变方块

canvas 里用的是系统字体栈（`PingFang SC` / `Microsoft YaHei`）。缺中文字体时会回退到方块，
装一个中文字体即可；`panel/web/widgets.js` 的 `FONT` 常量可以改成指定字体。

## 数据源抓不到

组件会保留上次值并显示占位（「暂无数据」/「占位数据」），失败后 15 秒重试（不会等满 TTL）。
常见原因：公司网络挡了 `open-meteo` / 新浪 / 东财 / GitHub；换网络或改用可访问的数据源。

## Windows 特有

见 [`docs/windows.md`](windows.md) 的「故障排查」一节（COM 口不出现、`--probe` 全 false、512 KB 单次写入超时等）。
