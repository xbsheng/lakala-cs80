#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""一键刷机：拉卡拉 CS80 客显屏 → QDisplay 副屏固件。

给「第一次用、不想看文档」的人：把设备插好，双击仓库根目录的 `一键刷机.cmd`，
本脚本会自己判断设备代次、要不要先降级、要不要先备份，然后一路刷完并验证。

    py\\python.exe flash_oneclick.py               # 交互（刷写前确认一次）
    py\\python.exe flash_oneclick.py --yes         # 全自动，不确认
    py\\python.exe flash_oneclick.py --dry-run     # 只诊断 + 打印计划，不动设备
    py\\python.exe flash_oneclick.py --no-backup   # 不做整片备份（不建议）
    py\\python.exe flash_oneclick.py --app <bin>   # 换固件（如 0.3.1）
    py\\python.exe flash_oneclick.py --pclk 100    # 不打 dsi_pclk 补丁（保持原厂时钟）

按设备当前状态自动选路（每一步都可重入，中途失败重跑即可）：

| 设备状态 | 动作 |
|---|---|
| 已经是 QDisplay（QDC1 应答） | 只报状态，不动它（`--force` 可强制重刷） |
| R03 基带（`…03A04M08_OCPU_QPY`） | 只刷 `0x60260000` 的 APP 分区（10 秒级） |
| R05 / 拉卡拉 1.6 出厂（`…05A01M08_OPU_BETA1215`） | 先整片备份 → 刷 `fw.pac` 降到 R03 → 再刷 APP（10 分钟级） |
| 在下载模式（`0525:a4a7`）但拿不到版本 | 读 flash 头部判代次（`2PPA` 才算 R03 系） |
| 完全找不到设备 | 打印「线 / 供电 / 驱动 / HUB」四步排查；缺 Quectel 驱动可代装 |

为什么 R05 要先降级：新批次出厂固件（拉卡拉 1.6）的 APP 区不是 `2PPA` 容器、
USB 也没有 if32（QDC1 数据口），直接刷 QDisplay 会「写入成功 + 读回一致 + 黑屏」。
详见 docs/firmware.md §0.5。
"""
import argparse
import hashlib
import os
import struct
import subprocess
import sys
import tempfile
import time

try:  # Windows 控制台是 GBK 时，print 里的 ✗/✓ 会直接抛 UnicodeEncodeError
    sys.stdout.reconfigure(errors="replace")
except Exception:
    pass

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
os.chdir(HERE)                      # 导入的模块里有相对路径（fw.pac / py/），统一以本目录为 cwd
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, "panel"))

import serial
import serial.tools.list_ports as lp

import appimg
import dump_flash as df
import patch_pclk
from third_party.sprdflash import native, protocol as sp
from third_party.sprdflash.flasher import DEFAULT_CHUNK, send_stage

APP_ADDR = 0x60260000
DL_VID_PID = (0x0525, 0xA4A7)
AT_VID = 0x2C7C
PAC = os.path.join(HERE, "fw.pac")
DEFAULT_APP = os.path.join(ROOT, "win-display", "qdisplay", "qdisplay_native.bin")
DUMPS = os.path.join(ROOT, "win-flash", "dumps")
DRIVER_ZIP = os.path.join(HERE, "quectel-usb-driver.zip")
R03_HINT = ("03A04M08", "OCPU_QPY")     # 可直接刷 APP2 的那代
R05_HINT = ("05A01M08", "gCS80")

_log_fp = None


def log(msg=""):
    print(msg, flush=True)
    if _log_fp:
        _log_fp.write(msg + "\n")
        _log_fp.flush()


def banner(text):
    log("")
    log("=" * 68)
    log(text)
    log("=" * 68)


def ask(prompt, auto=False):
    if auto:
        log(prompt + " [--yes] 自动继续")
        return True
    try:
        return input(prompt + " [回车继续 / Ctrl+C 取消] ").strip().lower() in ("", "y", "yes")
    except (EOFError, KeyboardInterrupt):
        log("\n已取消")
        return False


# ---------------------------------------------------------------- 设备发现
def find_at_port():
    return df.find_port(AT_VID, None, "AT Port")


def find_download_port():
    return df.find_port(*DL_VID_PID)


def all_coms():
    return [p.device for p in lp.comports()]


def at_cmd(port, cmd, wait=0.8):
    try:
        s = serial.Serial(port, 115200, timeout=wait, write_timeout=1.0)
    except Exception as exc:
        return "OPEN_FAIL: %s" % exc
    try:
        s.dtr = True
        s.rts = True
        time.sleep(0.15)
        s.reset_input_buffer()
        s.write((cmd + "\r\n").encode())
        time.sleep(wait)
        return s.read(1024).decode("utf-8", "replace").strip()
    finally:
        try:
            s.close()
        except Exception:
            pass


def usb_presence():
    """返回 {'modem': n, 'download': n, 'broken': n}，只解析 ASCII ID，避免编码问题。"""
    ps = ("Get-CimInstance Win32_PnPEntity | Where-Object {$_.PNPDeviceID -match "
          "'VID_2C7C|VID_0525|VID_0000&PID_0002'} | ForEach-Object { $_.PNPDeviceID }")
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", ps],
                             capture_output=True, text=True, timeout=60).stdout
    except Exception:
        return {"modem": 0, "download": 0, "broken": 0}
    ids = [ln.strip() for ln in out.splitlines() if ln.strip().upper().startswith("USB")]
    return {
        "modem": sum(1 for i in ids if "VID_2C7C" in i.upper()),
        "download": sum(1 for i in ids if "VID_0525" in i.upper()),
        "broken": sum(1 for i in ids if "VID_0000&PID_0002" in i.upper()),
    }


def qdc1_probe():
    """扫所有 COM 口发一个 QDC1 HELLO，返回 (com, resp) 或 (None, None)。"""
    try:
        import qdc1_ping
    except Exception as exc:                      # 没有 panel/ 也能刷机，只是省掉这一步
        log("  （跳过 QDC1 探活：%s）" % exc)
        return None, None
    for com in all_coms():
        try:
            dev = qdc1_ping.Dev(com)
        except Exception:
            continue
        try:
            r = dev.ask(1, wait=0.8)
        except Exception:
            r = None
        finally:
            try:
                dev.s.close()
            except Exception:
                pass
        if r:
            return com, r
    return None, None


# ---------------------------------------------------------------- 代次判定
def classify_version(text):
    """从 ATI/AT+QGMR 输出判代次：'r03' / 'r05' / 'unknown'。"""
    t = (text or "").upper()
    if all(h.upper() in t for h in R03_HINT) or "OCPU_QPY" in t:
        return "r03"
    if any(h.upper() in t for h in R05_HINT) or "BETA" in t:
        return "r05"
    return "unknown"


def detect_from_flash():
    """在下载模式里读 flash 头部判代次。

    返回 dict(app2=bool, build=str)。不看版本号也能判：APP2 容器头是 `2PPA`。
    """
    port = find_download_port()
    if not port:
        port = df.enter_download()
    if not port:
        return None
    s = None
    io = None
    try:
        s, io = df.connect_fdl(port)
        boot = df.read_range(io, 0x60000000, 0x100, 0x1000)
        app = df.read_range(io, APP_ADDR, 0x100, 0x1000)
    finally:
        if io is not None:
            try:
                io.send(sp.BSL_CMD_NORMAL_RESET)
                time.sleep(0.4)
            except Exception:
                pass
        if s is not None:
            try:
                s.close()
            except Exception:
                pass
    build = "".join(chr(b) if 32 <= b < 127 else " " for b in boot[0x20:0x60]).strip()
    build = " ".join(build.split())
    return {"app2": app[:4] == b"2PPA", "build": build}


# ---------------------------------------------------------------- 驱动
def install_driver(auto=False):
    log("")
    log("模组已经插上了，但 Windows 没有它的 COM 口 —— 缺 Quectel USB 驱动。")
    log("仓库里自带 `win-flash/flash/quectel-usb-driver.zip`，可以代装（会弹 UAC，需要管理员）。")
    if not ask("现在装驱动？", auto):
        log("已跳过：请手动解压 quectel-usb-driver.zip 运行 setup.exe，装完重插 Type-C 再跑本脚本。")
        return find_at_port() is not None
    if not os.path.exists(DRIVER_ZIP):
        log("✗ 找不到 %s" % DRIVER_ZIP)
        return False
    tmp = tempfile.mkdtemp(prefix="cs80drv_")
    log("解压驱动到 %s ..." % tmp)
    subprocess.run(["powershell", "-NoProfile", "-Command",
                    "Expand-Archive -Force -LiteralPath '%s' -DestinationPath '%s'"
                    % (DRIVER_ZIP, tmp)], check=False)
    exe = None
    for root, _dirs, files in os.walk(tmp):
        for f in files:
            if f.lower() == "setup.exe":
                exe = os.path.join(root, f)
    if not exe:
        log("✗ 压缩包里没找到 setup.exe")
        return False
    log("启动 %s（请在 UAC 弹窗里点「是」，装完会自己继续）" % exe)
    subprocess.run(["powershell", "-NoProfile", "-Command",
                    "Start-Process -FilePath '%s' -Verb RunAs -Wait" % exe], check=False)
    log("驱动安装程序已退出。请把 Type-C 拔插一次，等 Windows 枚举完 ...")
    deadline = time.time() + 180
    while time.time() < deadline:
        if find_at_port():
            log("✓ AT 口出现了：%s" % find_at_port())
            return True
        time.sleep(1.0)
    log("✗ 还没有 AT 口。Check: 设备管理器里应出现 `Quectel USB AT Port (COMx)`")
    return False


# ---------------------------------------------------------------- 刷写
def backup_full(addr=0x60000000, size=0x800000):
    """整片 dump。返回保存路径，失败返回 None。"""
    port = find_download_port() or df.enter_download()
    if not port:
        return None
    log("")
    log("--- 整片备份 0x%08X..0x%08X（%.0f MiB，这个速度下约 %d 分钟）---"
        % (addr, addr + size, size / 1048576.0, max(1, int(size / 1024.0 / 28 / 60))))
    s = None
    io = None
    try:
        s, io = df.connect_fdl(port)
        chunk = df.pick_chunk(io, addr)
        log("  读 chunk 0x%X，开始备份 ..." % chunk)
        t0 = time.time()
        data = bytearray()
        step = 0x40000
        for off in range(0, size, step):
            data += df.read_range(io, addr + off, min(step, size - off), chunk)
            done = len(data)
            print("\r  %5.1f%%  %6.1f KB  %.0f KB/s"
                  % (done * 100.0 / size, done / 1024.0,
                     done / 1024.0 / max(time.time() - t0, 1e-6)), end="", flush=True)
        print()
    finally:
        if io is not None:
            try:
                io.send(sp.BSL_CMD_NORMAL_RESET)
                time.sleep(0.4)
            except Exception:
                pass
        if s is not None:
            try:
                s.close()
            except Exception:
                pass

    data = bytes(data)
    if len(data) != size:
        log("✗ 备份长度不对（%d != %d）" % (len(data), size))
        return None
    sha = hashlib.sha256(data).hexdigest()
    os.makedirs(DUMPS, exist_ok=True)
    path = os.path.join(DUMPS, "full_flash_0x%08X_%dMiB_%s.bin"
                        % (addr, size // 0x100000, sha[:8]))
    with open(path, "wb") as f:
        f.write(data)
    log("✓ 备份完成：%s" % path)
    log("  sha256=%s" % sha)
    log("  回滚：py\\python.exe dump_flash.py --restore \"%s\" --addr 0x%08X" % (path, addr))
    return path


def flash_pac():
    port = find_download_port() or df.enter_download()
    if not port:
        log("✗ 进不了下载模式")
        return False
    native._open_serial = _open_serial_retry
    last = {"t": 0.0}

    def progress(file_id, sent, total):
        pct = sent * 100 // total if total else 100
        print("\r  %-14s %3d%%" % (file_id, pct), end="", flush=True)
        if pct >= 100:
            print()

    log("")
    log("--- 刷 QuecPython 整包（降到 R03 基带；会格式化文件系统 + 重建 NV）---")
    t0 = time.time()
    native.native_flash(port, PAC, progress=progress, format_fs=True, do_reset=True)
    log("  完成 %.0f s" % (time.time() - t0))
    return True


def _open_serial_retry(com, timeout=0.3):
    """CDC 下载口刚枚举出来时会短暂打不开，重试 200 次（同 flash_at.open_serial）。"""
    import flash_at
    return flash_at.open_serial(com, timeout=timeout)


def flash_app(bin_path, app_data):
    port = find_download_port() or df.enter_download()
    if not port:
        log("✗ 进不了下载模式")
        return False
    log("")
    log("--- 写 APP 分区 0x%08X（%d B）---" % (APP_ADDR, len(app_data)))
    s = None
    io = None
    try:
        s, io = df.connect_fdl(port)
        t0 = time.time()
        send_stage(io, APP_ADDR, app_data, DEFAULT_CHUNK)
        log("  写完 %.1f s，读回校验 ..." % (time.time() - t0))
        back = native._read_flash(io, APP_ADDR, len(app_data), chunk=1024)
        if back != app_data:
            diff = sum(1 for a, b in zip(back, app_data) if a != b)
            log("✗ 读回不一致（%d 字节不同）。保持下载模式，重跑本脚本即可。" % diff)
            return False
        log("  ✓ 读回一致")
        io.send(sp.BSL_CMD_NORMAL_RESET)
        time.sleep(0.5)
        return True
    finally:
        if s is not None:
            try:
                s.close()
            except Exception:
                pass


# ---------------------------------------------------------------- 等待/验证
def wait_at_port(seconds=240, auto_hint=True):
    deadline = time.time() + seconds
    shown = False
    while time.time() < deadline:
        at = find_at_port()
        if at:
            return at
        if not shown and auto_hint and time.time() > deadline - seconds + 60:
            log("  还在等设备重启回来 ... 若一直没出现：拔掉 DC 和 Type-C，等 10 秒，")
            log("  先插 12V 再插 Type-C（刚刷完基带模块会重启两三轮，USB 会短暂整个消失）")
            shown = True
        time.sleep(1.0)
    return find_at_port()


def verify(timeout=90, force=False):
    log("")
    log("--- 验证：等设备重启后发 QDC1 HELLO ---")
    deadline = time.time() + timeout
    while time.time() < deadline:
        com, resp = qdc1_probe()
        if com:
            d = qdc1_ping_features(com)
            log("✓ QDC1 通了：%s  block=%s  features=0x%x"
                % (com, resp.get("detail"), d))
            return True
        if find_download_port():
            log("  设备又掉回下载模式了，发正常复位 ...")
            try:
                import reset_download
                reset_download.main()
            except Exception as exc:
                log("  复位失败：%s" % exc)
        time.sleep(2.0)
    if not force:
        log("✗ 90 秒内没有 QDC1 应答。排查：")
        log("  1) 屏上是「等待连接」页吗？黑屏说明 APP 没跑起来（重刷 / 看 docs/firmware.md §0.5）")
        log("  2) 设备管理器里应有 if32 → `Quectel USB REPL Port`；没有就是基带还不对")
        log("  3) 桌面壳 cs80-panel 开着会抢占该 COM 口，先关掉它")
    return False


def qdc1_ping_features(com):
    try:
        import qdc1_ping
        dev = qdc1_ping.Dev(com)
        try:
            r = dev.ask(1, flags=2) or {}
            return r.get("detail", 0)
        finally:
            dev.s.close()
    except Exception:
        return 0


# ---------------------------------------------------------------- 桌面壳/ sidecar
# 桌面壳拉起 qdc1.exe --probe 会逐 个 COM 口扫并占住 QDC1 那个口，导致
# 「探活失败 → 以为没刷 → 又去重刷一遍」。所以刷机前先把它关掉。
def panel_apps():
    ps = ("Get-CimInstance Win32_Process | Where-Object {$_.Name -match 'cs80-panel|qdc1' -or "
          "($_.Name -eq 'python.exe' -and $_.CommandLine -match 'host\\.py')} | "
          "ForEach-Object { '{0} {1}' -f $_.ProcessId, $_.Name }")
    try:
        out = subprocess.run(["powershell", "-NoProfile", "-Command", ps],
                             capture_output=True, text=True, timeout=60).stdout
    except Exception:
        return []
    found = []
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        pid, _, name = line.partition(" ")
        if pid.isdigit():
            found.append((int(pid), name.strip()))
    return found


def stop_panel_apps(auto=False, dry_run=False):
    procs = panel_apps()
    if not procs:
        return False
    log("")
    log("检测到桌面壳 / QDC1 sidecar 正在运行（会抢占 COM 口，刷机前必须关）：")
    for pid, name in procs:
        log("  pid %-6d %s" % (pid, name))
    if dry_run:
        log("  （--dry-run：不关。真刷时会先自动关掉；也可能因此探活误判成「未刷」）")
        return True
    if not ask("关掉它们？", auto):
        log("已保留。请手动退出 cs80-panel，然后重跑本脚本。")
        return True
    for pid, _name in procs:
        subprocess.run(["powershell", "-NoProfile", "-Command",
                        "Stop-Process -Id %d -Force -ErrorAction SilentlyContinue" % pid],
                       check=False)
    time.sleep(1.5)
    left = panel_apps()
    if left:
        log("⚠ 还有 %d 个进程没关掉，刷机可能因 COM 口被占而失败" % len(left))
    else:
        log("✓ 已关掉（刷完重新双击 cs80-panel.exe 即可）")
    return True


# ---------------------------------------------------------------- 主流程
def main():
    global _log_fp
    ap = argparse.ArgumentParser(add_help=True, description="CS80 一键刷机（QDisplay）")
    ap.add_argument("--yes", action="store_true", help="不确认，直接刷")
    ap.add_argument("--dry-run", action="store_true", help="只诊断+打印计划，不动设备")
    ap.add_argument("--no-backup", action="store_true", help="降级前不做整片备份（不建议）")
    ap.add_argument("--force", action="store_true", help="设备已是 QDisplay 也强制重刷")
    ap.add_argument("--app", default=DEFAULT_APP, help="APP 固件 bin（默认 0.3.0）")
    ap.add_argument("--pclk", type=int, default=200,
                    help="DSI 像素时钟 dsi_pclk_rate（默认 200，修中间灰闪；100 = 保持原厂）")
    args = ap.parse_args()

    os.makedirs(DUMPS, exist_ok=True)
    log_path = os.path.join(DUMPS, "flash-%s.log" % time.strftime("%Y%m%d-%H%M%S"))
    _log_fp = open(log_path, "a", encoding="utf-8")

    banner("拉卡拉 CS80 客显屏 → QDisplay 副屏    一键刷机")
    log("日志：%s" % log_path)

    # 前置文件
    for path, desc in ((PAC, "官方 PAC"), (args.app, "APP 固件")):
        if not os.path.exists(path):
            log("✗ 缺文件：%s（%s）" % (path, desc))
            return 1
    app_data = src_data = open(args.app, "rb").read()
    if not appimg.check(app_data):
        log("✗ %s 的 APP2 头 CRC32 不对（手改过？先跑 appimg.fix）" % args.app)
        return 1
    log("固件：%s  %d B  APP2 CRC ok" % (os.path.basename(args.app), len(app_data)))
    try:
        app_data, site = patch_pclk.patch(src_data, args.pclk)
    except ValueError as exc:
        log("⚠ 没打 dsi_pclk 补丁（%s），按原值刷" % exc)
    else:
        old = src_data[site]
        if old == args.pclk:
            log("dsi_pclk_rate = %d（已是目标值，不改）" % old)
        else:
            log("dsi_pclk_rate %d → %d（偏移 0x%x，修中间灰闪 issue #4；--pclk 100 关闭）"
                % (old, args.pclk, site))

    apps_running = stop_panel_apps(args.yes, args.dry_run)

    # 1) 已经刷过？
    if not args.force:
        log("")
        log("--- 探一下设备是不是已经在跑 QDisplay ---")
        com, resp = qdc1_probe()
        if com:
            feats = qdc1_ping_features(com)
            log("✓ 设备已经是 QDisplay 了：%s  block=%s  features=0x%x"
                % (com, resp.get("detail"), feats))
            log("  无需刷机。要强制重刷加 --force。")
            log("  看画面：跑 panel\\run-windows.cmd（或打包好的 cs80-panel.exe）")
            return 0
        if apps_running:
            log("  （注意：上面的探活可能因桌面壳占着 COM 口而误判，真刷时会先关掉它）")

    # 2) 找设备 / 判代次
    log("")
    log("--- 找设备 ---")
    dl = find_download_port()
    at = find_at_port()
    gen = "unknown"
    build = ""
    if dl:
        log("设备已在下载模式：%s" % dl)
        info = detect_from_flash()
        if info is None:
            log("✗ 读不到 flash 头部")
            return 2
        gen = "r03" if info["app2"] else "r05"
        build = info["build"]
        log("  flash 头部：APP2=%s  build=%s" % (info["app2"], build or "(未识别)"))
    elif at:
        log("AT 口：%s" % at)
        ver = at_cmd(at, "ATI")
        log("  ATI -> %s" % " | ".join(x for x in ver.splitlines() if x.strip()))
        gen = classify_version(ver)
        if gen == "unknown":
            log("  版本号没认出来，改读 flash 头部判代次 ...")
            info = detect_from_flash()
            if info is not None:
                gen = "r03" if info["app2"] else "r05"
                build = info["build"]
                log("  flash 头部：APP2=%s  build=%s" % (info["app2"], build or "(未识别)"))
    else:
        log("没找到设备。")
        usb = usb_presence()
        log("  USB 设备：模组=%d  下载模式=%d  描述符失败=%d"
            % (usb["modem"], usb["download"], usb["broken"]))
        if usb["modem"] and not at:
            if install_driver(args.yes):
                at = find_at_port()
                ver = at_cmd(at, "ATI")
                log("  ATI -> %s" % " | ".join(x for x in ver.splitlines() if x.strip()))
                gen = classify_version(ver)
            else:
                return 2
        elif usb["broken"]:
            log("")
            log("排查（按顺序，最常见的在前）：")
            log("  1) 换一根**能传数据**的 Type-C 线（只充电的线就是这个现象）")
            log("  2) 直插主机 USB 口，别走 HUB / 扩展坞")
            log("  3) 确认 DC 12V 也接着（10.1\" 背光电流大，只靠 Type-C 会掉）")
            log("  4) 拔掉 DC + Type-C，等 10 秒，先插 12V 再插 Type-C")
            return 2
        else:
            log("")
            log("排查：Type-C 数据线 / DC 12V 供电 / 设备是否开机（屏亮不亮）")
            log("  如果这台设备之前被刷坏过（AT 口完全没响应）：docs/firmware.md §5 短接 pin55 进 BootROM")
            return 2

    need_downgrade = gen != "r03"
    log("")
    log("判代次：%s%s" % (gen.upper(), ("（%s）" % build) if build else ""))
    if gen == "unknown":
        log("✗ 判不出代次，为安全起见不动它。把上面 ATI / build 发我看看。")
        return 3

    plan = []
    if need_downgrade:
        if not args.no_backup:
            plan.append("整片备份 8 MiB → win-flash/dumps/（约 5 分钟）")
        plan.append("刷 fw.pac 把基带降到 R03（约 5 分钟，会格式化文件系统）")
        plan.append("重新进下载模式")
    plan.append("写 APP 分区 0x60260000（%d B）" % len(app_data))
    plan.append("复位并验证 QDC1 握手")
    banner("计划（%s）" % ("R05 新批次：先降级再刷" if need_downgrade else "R03：直接刷 APP"))
    for i, item in enumerate(plan, 1):
        log("  %d. %s" % (i, item))
    if args.dry_run:
        log("")
        log("--dry-run：到此为止，没有动设备。")
        return 0
    if not ask("", args.yes):
        return 1

    # 3) 降级（可选）
    if need_downgrade:
        if not args.no_backup:
            if not backup_full():
                log("✗ 备份失败，拒绝继续（--no-backup 可跳过，但不建议）")
                return 4
        if find_at_port() is None and find_download_port() is None:
            log("✗ 设备不见了，重跑本脚本")
            return 4
        if not flash_pac():
            log("✗ 整包刷写失败。重跑本脚本（可重入），或看 docs/firmware.md §5 硬刷兜底")
            return 5
        log("")
        log("--- 等设备用 R03 基带重启 ---")
        at = wait_at_port(240)
        if not at:
            log("✗ 设备没有回来。拔掉 DC + Type-C，等 10 秒，先插 12V 再插 Type-C，然后重跑本脚本。")
            return 5
        ver = at_cmd(at, "ATI")
        log("  ATI -> %s" % " | ".join(x for x in ver.splitlines() if x.strip()))
        if classify_version(ver) != "r03":
            log("✗ 重启后版本仍不是 R03，先别继续刷 APP（可能不是同一代硬件），把日志发我。")
            return 5

    # 4) 刷 APP
    if not flash_app(args.app, app_data):
        return 6

    # 5) 验证
    if not verify():
        return 7

    banner("完成")
    log("屏上现在应该是「等待连接」页（客户端连上后换成你的画布）。")
    log("下一步：")
    log("  · 直接看画面：双击 panel\\run-windows.cmd，或跑打包好的 cs80-panel.exe")
    log("  · 亮度想完全不闪就设 100（<100 是 1 kHz 软件 PWM，中间灰会轻微抖）")
    log("  · 想恢复原厂固件：docs/firmware.md §0.5 的 --restore 命令")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        log("\n已中断")
        sys.exit(130)
