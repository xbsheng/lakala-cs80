# win-flash/flash —— 刷机与恢复工具

> 概览与验收标准看 [`../../docs/firmware.md`](../../docs/firmware.md)；本文件是命令级细节。

两个入口，都靠纯 Python 走 USB（无需厂商工具）：

| 脚本 | 作用 |
|---|---|
| **`flash_oneclick.py`** | **一键刷机**：判代次 →（R05 先整片备份 + 刷 `fw.pac` 降级）→ 写 APP → 验证 QDC1。仓库根目录的 `一键刷机.cmd` 是它的双击入口（首次自动解压便携 Python + 装 pyserial）。详见 [`../../docs/firmware.md`](../../docs/firmware.md) §0.6 |
| **`qdisplay_app.py`** | 把 **QDisplay native APP** 写到 `0x60260000`（**只写 APP 分区**，不格式化、不动 NV）。当前设备就是这个状态 |
| **`patch_waiting_page.py`** | 给 0.3.1+ 的 `qdisplay_native.bin` 打 1 字节补丁：**关掉「等待连接」页每秒重绘**（倒计时），打完后等待页真静态。0.3.0 bin 会直接报「不用打补丁」，详见 [`../../docs/firmware.md`](../../docs/firmware.md) 的「2.5 关掉等待页每秒重绘」 |
| **`patch_pclk.py`** | 给 0.3.0 / 0.3.1 的 `qdisplay_native.bin` 改 `dsi_pclk_rate`（默认 100 → 200）：修复「白/黑不闪、中间灰闪」（issue #4）。只改 1 字节 + 重算 APP2 CRC32，详见 [`../../docs/firmware.md`](../../docs/firmware.md) 的「2.6」 |
| **`appimg.py`** | APP2 容器头 `u32@8 = crc32(整文件)` 的重算/校验 —— **手改任何字节后必须重算**，否则 loader 不启动 APP（屏黑、COM15 无应答、AT 口正常） |
| **`reset_download.py`** | 设备卡在下载模式（`PDL connect` 超时，其实 FDL2 还在跑）时走 BSL 发 `NORMAL_RESET` 重启，不用拔插 |
| **`flash_at.py`** | 按 `fw.pac` **整包刷 QuecPython**（含格式化文件系统 + 重建 NV）。用于「恢复成 Python 环境」 |
| **`dump_flash.py`** | 整片 flash 备份 / 裸写回滚（默认 `0x60000000` 起 8 MiB）。降级基带、换固件前必做；`--restore` 把 dump 原样写回。详见 [`../../docs/firmware.md`](../../docs/firmware.md) §0.5 |

---

> 依赖说明：脚本全部走**串口**（`pyserial`）跑 PDL/FDL 协议，**不需要 pyusb / libusb / Zadig**。
> `third_party/sprdflash/usb_transport.py` 是另一条 libusb 路径（`transport='usb'`），本项目没有用到，
> 留着只是因为它是上游 sprdflash 的一部分。

## 0. 一键刷机（推荐）

给「新设备 + 新电脑」用，不需要先看下面的一堆前置：

```bat
:: 仓库根目录双击（或在 cmd 里跑）
一键刷机.cmd --dry-run      :: 先看诊断和计划
一键刷机.cmd                :: 刷（刷前确认一次）

:: 等价写法（在 win-flash\flash 目录）
py\python.exe flash_oneclick.py [--yes|--dry-run|--force|--no-backup] [--app <bin>]
```

它会自己完成：设备判代次 →（R05 新批次先整片 8 MiB 备份 + 刷 `fw.pac` 降到 R03）→
写 APP 分区并读回校验 → 复位 → 扫 COM 口发 QDC1 HELLO 验证。
日志在 `win-flash/dumps/flash-*.log`，备份也在同目录。

> 脚本会先关掉 `cs80-panel.exe` / `qdc1.exe`——它们（`qdc1 --probe`）会把 COM 口抢走。

## 0. 一次性准备

设备侧无需操作，PC 侧需要：

1. **移远 USB 驱动**（出 COM 口）：解压 `../../quectel-usb-driver.zip`（若存在）里的 `setup.exe` 安装；
   本机已装好，`设备管理器` 里能看到 `Quectel USB AT Port (COMx)`
2. **便携 Python**：`py/` 目录（Python 3.13 embed + pyserial）。若已丢失：

```bat
powershell -NoProfile -Command "Expand-Archive -Force -LiteralPath 'python-embed.zip' -DestinationPath 'py'"
:: _pth 加上 site-packages，再把 pyserial 解进去
>  py\python313._pth echo python313.zip
>> py\python313._pth echo .
>> py\python313._pth echo Lib/site-packages
mkdir py\Lib\site-packages
copy /y pyserial.whl tmp.zip
powershell -NoProfile -Command "Expand-Archive -Force -LiteralPath 'tmp.zip' -DestinationPath 'py\Lib\site-packages'"
del tmp.zip
py\python.exe -c "import serial; print(serial.__version__)"
```

---

## 1. 刷 QDisplay（日常用这个）

```bat
py\python.exe qdisplay_app.py ..\..\win-display\qdisplay\qdisplay_native.bin --auto
```

`--auto` 会自己找 AT 口发 `AT+QDOWNLOAD=1`，然后：

```
ATI(R03) → AT+QDOWNLOAD → 0525:a4a7 → PDL → FDL1 → FDL2
        → 写 0x60260000 (154 400 B) → 读回校验 → 复位
```

其它用法：

```bat
py\python.exe qdisplay_app.py <app.bin> --port COM13   :: 已在下载模式时直接指定端口
py\python.exe qdisplay_app.py <app.bin> --no-verify    :: 跳过读回校验（不建议）
```

---

## 2. 恢复成 QuecPython

```bat
py\python.exe flash_at.py --check   :: 只读：找 AT 口 + ATI + PDL 握手
py\python.exe flash_at.py           :: 整包刷 fw.pac（会问 y）
```

写入 `fw.pac`（官方 `QPY_OCPU_V0004_EC600U_CNLB_FW.zip` 解出的 PAC），**含 `--format`**：
会格式化内部文件系统并重建 NV。恢复后设备回到 `EC600UCNLBR03A04M08_OCPY`（Python 可用，但屏只能显示纯色，见主 README §5）。

---

## 3. 进下载模式

- **软**：正常工作态下发 `AT+QDOWNLOAD=1`（两个脚本都会自动做；设备会重新枚举成 `0525:a4a7`）
- **硬**（软的不行时）：断电 → 短接模组 **pin55 (USB_BOOT) ↔ GND** → 上电 → 约 2 秒后松开

---

## 4. 踩过的坑（都已在代码里处理）

| 坑 | 现象 | 处理 |
|---|---|---|
| 先用 BSL autobaud 探测 | `no VER response after N autobaud attempts`，之后 PDL 也超时 | `AT+QDOWNLOAD=1` 之后设备**就在 PDL 状态**，必须直接发 PDL `0xAE` 帧（`qdisplay_app.py` 已是 PDL 优先） |
| 串口没拉控制线 | 端口能开但设备无应答 | CDC 下载口要置 `DTR=1, RTS=1`（`native._open_serial` 已做） |
| 用 `native._read_flash()` 默认 chunk 校验 | 从 `0x3000` 起全是差异，误判「写失败」 | 该函数默认 chunk 太大（0x3000）读回不可靠；校验要用 **1 KB / 4 KB 分块** |
| 设备卡在「下载模式但不收数据」 | 写超时 / `ClearCommError` | 断电重上电（DC 和 Type-C 都拔），回正常工作态再走一遍 |
| 便携 Python 的 `sys.path` | 同目录模块 import 失败 | embed 版 `_pth` 不含脚本目录；脚本里 `sys.path.insert(0, ".")` |
| `READ_FLASH` 大 chunk | 回一个 0 字节的应答，不报错 → dump 出 0 字节文件 | chunk 不超过 `0x8000`，并且每条读都校验返回长度（`dump_flash.py` 已做） |
| 刷机时 AT 口“无响应” | `PermissionError(13, '拒绝访问')` / `Cannot configure port` | 桌面壳 `cs80-panel` 开着时它的 `qdc1.exe --probe` 会抢 COM 口，刷机前先退出 |
| 在 git-bash 里传 Windows 路径 | `FileNotFoundError: /tmp/...` | 用 `cygpath -w` 或直接给 `C:/...` 形式 |

---

## 5. 主要文件

| 文件 | 说明 |
|---|---|
| `qdisplay_app.py` | 只写 APP 分区（PDL → FDL1 → FDL2 → BSL 写 → 读回 → 复位） |
| `patch_waiting_page.py` | 关掉 0.3.1+ 等待页的每秒重绘（1 字节：`cmp r3,r6` → `cmp r3,r3`） |
| `appimg.py` | APP2 头部 `u32@8 = crc32(整文件)`，手改字节后必须重算 |
| `reset_download.py` | 卡下载模式时用 BSL `NORMAL_RESET` 让设备重启 |
| `flash_at.py` | 整包刷 PAC（QuecPython 恢复） |
| `dump_flash.py` | 整片备份 / 回滚（`dump_flash.py` / `dump_flash.py --restore <bin> --addr 0x60000000`） |
| `fw.pac` | 官方 PAC（FDL1/FDL2/BOOT/AP/APPIMG/PS/NV…） |
| `third_party/sprdflash/` | 纯 Python 的 Spreadtrum/UNISOC 刷写实现（PDL/BSL/PAC） |
| `py/` | 便携 Python + pyserial（gitignore，可重建） |

> QuecPython 时代的设备端脚本（`cs80.py` / `main.py` / `panel_init.py` / `deploy.py` / `device_exec.py` / `at_test.py`）
> 已归档到 `../../archive/quecpython/`，当前流程不再需要。
