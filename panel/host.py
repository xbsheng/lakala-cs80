#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""CS80 状态屏 host：独占 USB 的常驻进程 + 本地 HTTP（画布编辑器 / 数据源）。

    python3 panel/host.py            # 然后浏览器打开 http://127.0.0.1:8787
    python3 panel/host.py --port 9000

职责边界：
  - 唯一持有 panel/qdc1 sidecar（USB 独占），把帧/局部更新/控屏转给它
  - 存布局 ~/.cs80_panel/layout.json
  - 抓数据（系统指标 / agent / 天气 / 行情 / 热榜 / 假期 / Home Assistant），带缓存，组件只读缓存
不负责画像素：像素由浏览器画布算出来再 POST 进来（路线 A：一份渲染代码即编辑器又是设备像素源）。
"""
import argparse
import datetime
import hashlib
import json
import os
import queue
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
WEB = os.path.join(HERE, "web")
SIDECAR = os.path.join(HERE, "qdc1.exe" if os.name == "nt" else "qdc1")
LAYOUT = os.path.join(os.path.expanduser("~"), ".cs80_panel", "layout.json")
STATE_FILE = os.path.join(os.path.expanduser("~"), "cs80_display", "state.json")
CONF_DIR = os.path.join(os.path.expanduser("~"), ".cs80_panel")  # 跨平台：日志/配置都放这儿
AGENT_STATUS_DIR = os.path.join(CONF_DIR, "agent-status")  # Agent extension/hook 写出的精确状态
SIDECAR_LOG = os.path.join(CONF_DIR, "sidecar.log")
WEB_LOG = os.path.join(CONF_DIR, "web.log")
# 连上设备时把亮度拉回这个值（0 = 不动）。固件默认 100、PWM 全停；但 +/- 键或
# 上位机设过的低亮度会留在固件 RAM 里，断开主机后固件自己画的等待页仍在 1 kHz 软件
# PWM 下微闪 —— 所以每次连上都重置一次。用户在本进程里手动设过亮度就不再覆盖。
# 亮度策略：**默认不自动设**（以前连上就拉回 100，会把用户手动调的 50% 几十秒后又刷回去）。
# 现在改成“记住用户选的值”：见 Device._bright_state()。
#   CS80_BRIGHTNESS=100  → 强制锁定到 100（老行为，想要 PWM 完全不停时用）
BRIGHTNESS_PIN = int(os.environ.get("CS80_BRIGHTNESS", "0") or 0)

# kiosk 存活心跳：浏览器页面每 5 秒 POST 一次 /api/heartbeat。
# 看门狗脚本（panel/kiosk-guard.sh）用它判断页面是否被浏览器冻结/崩了。
BEAT = {"at": 0.0}
PUSH = {"at": 0.0, "n": 0}
_USB_INFO = {"at": 0.0, "value": None}

GRID_PAGE = {
    "id": "main",
    "name": "主页",
    "type": "grid",
    "widgets": [
        # 左上：3×2 表盘时钟（右上角带天气，城市 北京）
        {"id": "clock", "type": "clock_dial", "cell": [0, 0, 3, 2],
         "config": {"city": "北京", "showWeather": True}},
        # 右上：2×2 RunCat（CPU/MEM 环 + 奔跑猫，右上角显示本机局域网 IP）
        {"id": "runcat", "type": "runcat", "cell": [3, 0, 2, 2], "config": {}},
        # 中间：5×2 自选股（一格一只，只在 A 股交易时段轮询）
        {"id": "stock", "type": "stock", "cell": [0, 2, 5, 2],
         "config": {"symbols": "sh000001,sz399001"}},
        # 5×2 热榜：勾选的数据源作为 Tab，按 60 秒轮播
        {"id": "news", "type": "news", "cell": [0, 4, 5, 2],
         "config": {"sources": ["toutiao", "baidu", "weibo"], "rotate": 60}},
        # 底部：5×2 Agent 会话列表（自动扫描本机 Pi / Claude Code / Codex / DeepSeek）
        {"id": "agent", "type": "agent", "cell": [0, 6, 5, 2], "config": {}},
    ],
}

DEFAULT_LAYOUT = {
    "version": 2,
    "theme": {"bg": "#141519", "card": "#1a1c22", "line": "#2a2c34"},
    "activePage": "main",
    "pages": [
        GRID_PAGE,
        {
            "id": "stockpage",
            "name": "股票自选",
            "type": "custom",
            "module": "pages/stock.js",
            "config": {"symbols": "sh000001,sz399001,sh600519", "title": "自选股"},
        },
    ],
}


def migrate_layout(layout):
    """v1（顶层 widgets）→ v2（pages）。idempotent。"""
    if not layout:
        return json.loads(json.dumps(DEFAULT_LAYOUT))
    if layout.get("version") == 2 and isinstance(layout.get("pages"), list) and layout["pages"]:
        pass
    elif isinstance(layout.get("widgets"), list):
        layout = {
            "version": 2,
            "theme": layout.get("theme") or {},
            "activePage": "main",
            "pages": [{"id": "main", "name": "主页", "type": "grid", "widgets": layout["widgets"]}],
        }
    else:
        return json.loads(json.dumps(DEFAULT_LAYOUT))
    pages = []
    for i, page in enumerate(layout["pages"]):
        p = dict(page)
        p.setdefault("id", "page%d" % i)
        p.setdefault("name", p["id"])
        p.setdefault("type", "grid")
        if p["type"] == "grid":
            p["widgets"] = p.get("widgets") or []
        else:
            p.setdefault("module", "pages/stock.js")
            p.setdefault("config", {})
        pages.append(p)
    if not any(p["type"] == "custom" for p in pages):
        pages.append(json.loads(json.dumps(DEFAULT_LAYOUT["pages"][1])))  # 默认带一个自定义股票页
    layout["pages"] = pages
    ids = [p["id"] for p in pages]
    if layout.get("activePage") not in ids:
        layout["activePage"] = ids[0]
    return layout


# ---------------- 屏保 ----------------
SCREENSAVER_CFG = os.path.join(os.path.expanduser("~"), ".cs80_panel", "screensaver.json")
MEDIA_DIRS = [os.path.join(HERE, "web", "media")]
for _d in (("Videos",) if os.name == "nt" else ("Movies",)):
    MEDIA_DIRS.append(os.path.join(os.path.expanduser("~"), _d, "cs80-screensaver"))
MEDIA_DIRS.append(os.path.join(os.path.expanduser("~"), "Movies", "cs80-screensaver"))  # mac 习惯目录也认
SCREEN_STATE = {"locked": False, "asleep": False, "at": 0.0}
SAVER = None


def _windows_locked():
    """Windows：锁屏/UAC 时输入桌面变成 secure desktop → OpenInputDesktop 失败。"""
    try:
        import ctypes

        u32 = ctypes.windll.user32
        u32.OpenInputDesktop.restype = ctypes.c_void_p
        u32.OpenInputDesktop.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
        u32.CloseDesktop.argtypes = [ctypes.c_void_p]
        h = u32.OpenInputDesktop(0, False, 0x0100)  # DESKTOP_READOBJECTS
        if not h:
            return True
        u32.CloseDesktop(h)
        return False
    except Exception:
        return False


def screen_locked():
    """锁屏判定：macOS 用 CGSessionCopyCurrentDictionary；Windows 用 OpenInputDesktop。"""
    if os.name == "nt":
        return _windows_locked()
    if sys.platform != "darwin":
        return False
    try:
        import ctypes
        import ctypes.util

        cg = ctypes.CDLL(ctypes.util.find_library("CoreGraphics"))
        cf = ctypes.CDLL(ctypes.util.find_library("CoreFoundation"))
        cg.CGSessionCopyCurrentDictionary.restype = ctypes.c_void_p
        ref = cg.CGSessionCopyCurrentDictionary()
        if not ref:
            return False
        cf.CFDictionaryGetValue.restype = ctypes.c_void_p
        cf.CFDictionaryGetValue.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        cf.CFStringCreateWithCString.restype = ctypes.c_void_p
        cf.CFStringCreateWithCString.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_uint32]
        cf.CFBooleanGetValue.argtypes = [ctypes.c_void_p]
        key = cf.CFStringCreateWithCString(None, b"CGSSessionScreenIsLocked", 0x08000100)
        val = cf.CFDictionaryGetValue(ref, key)
        locked = bool(cf.CFBooleanGetValue(val)) if val else False
        cf.CFRelease(ctypes.c_void_p(ref))
        cf.CFRelease(ctypes.c_void_p(key))
        return locked
    except Exception:
        return False


def display_asleep():
    """显示器是否休眠（拿不到就当作没睡，宁可不触发屏保）"""
    if os.name == "nt":
        return False  # Windows 没有便宜可靠的接口，锁屏（screen_locked）已经够用了
    try:
        out = subprocess.check_output(["pmset", "-g", "powerstate", "IODisplayWrangler"], text=True, timeout=4)
        return " 1" in out.split("\n")[-2]
    except Exception:
        return False


def _screen_watch():
    while True:
        locked = screen_locked()
        SCREEN_STATE.update(locked=locked, asleep=display_asleep(), at=time.time())
        if SAVER:
            cfg = load_saver_cfg()
            if locked and cfg.get("autoLock", True) and not SAVER.playing and cfg.get("file"):
                SAVER.start(cfg["file"], by_lock=True)
            elif not locked and SAVER.playing and SAVER.by_lock:
                SAVER.stop()  # 只停「锁屏自动起的」，手动播的不动
        time.sleep(2)


class Saver:
    """屏保：host 侧用 ffmpeg 解码成 BGR565 帧，直接写进 sidecar。

    为什么不放在浏览器里播：锁屏时 WebView 被遮挡，页面进入 hidden，
    setInterval 被节流到 ~1s、视频解码也降频 → 屏上就卡成幻灯片。
    ffmpeg 在 host 侧解码完全不受窗口可见性影响。
    """

    def __init__(self, device):
        self.device = device
        self.proc = None
        self.thread = None
        self.file = ""
        self.frames = 0
        self.error = ""
        self.lock = threading.Lock()
        self.last_at = 0.0
        self.by_lock = False  # 是锁屏自动起的还是手动起的

    @property
    def playing(self):
        return self.proc is not None and self.proc.poll() is None

    def _path(self, name):
        for d in MEDIA_DIRS:
            p = os.path.join(d, name)
            if os.path.isfile(p):
                return p
        return ""

    def start(self, name, fps=12, by_lock=False):
        path = self._path(name)
        if not path:
            self.error = "找不到视频：%s" % name
            return False
        if not shutil.which("ffmpeg"):
            self.error = ("没有 ffmpeg（Windows: winget install Gyan.FFmpeg"
                          if os.name == "nt" else "没有 ffmpeg（brew install ffmpeg）")
            return False
        self.stop()
        cmd = [
            "ffmpeg", "-v", "error", "-re", "-stream_loop", "-1", "-i", path,
            "-f", "rawvideo", "-pix_fmt", "bgr565le", "-s", "800x1280", "-r", str(fps), "-",
        ]
        try:
            self.proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, bufsize=0)
        except Exception as exc:
            self.error = str(exc)
            return False
        self.file = name
        self.frames = 0
        self.error = ""
        self.by_lock = by_lock
        self.thread = threading.Thread(target=self._pump, args=(self.proc,), daemon=True)
        self.thread.start()
        return True

    def _pump(self, proc):
        frame = 2048000

        def read_exact(n):
            chunks = []
            got = 0
            while got < n:
                chunk = proc.stdout.read(n - got)  # 管道 read 可能只给一部分，必须凑满
                if not chunk:
                    return None
                chunks.append(chunk)
                got += len(chunk)
            return b"".join(chunks)

        while proc.poll() is None:
            buf = read_exact(frame)
            if buf is None:
                break
            if not self.device.send(b"\x04" + buf):
                time.sleep(0.2)  # 设备没连上就慢一点，别空转
                continue
            self.frames += 1
            self.last_at = time.time()
        rc = proc.poll()
        if rc not in (0, None, -9) and not self.error:
            self.error = "ffmpeg 退出码 %s" % rc
        try:
            proc.stdout.close()
        except Exception:
            pass

    def stop(self):
        self.error = ""  # 主动停止不算错误
        proc, self.proc = self.proc, None
        if proc and proc.poll() is None:
            proc.kill()
            try:
                proc.wait(timeout=2)
            except Exception:
                pass
        self.file = ""

    def status(self):
        return {
            "playing": self.playing,
            "file": self.file,
            "frames": self.frames,
            "fps": 12 if self.playing else 0,
            "error": self.error,
        }


def screensaver_files():
    files = []
    for d in MEDIA_DIRS:
        if not os.path.isdir(d):
            continue
        for n in sorted(os.listdir(d)):
            if n.lower().endswith((".mp4", ".webm", ".m4v", ".mov")):
                path = os.path.join(d, n)
                files.append({"name": n, "size": os.path.getsize(path), "source": os.path.basename(d)})
    return files


def load_config():
    """~/.cs80_panel/config.json：目前只存亮度偏好，以后可放其它全局设置。"""
    try:
        with open(os.path.join(CONF_DIR, "config.json"), encoding="utf-8") as f:
            cfg = json.load(f)
        return cfg if isinstance(cfg, dict) else {}
    except Exception:
        return {}


def save_config(cfg):
    try:
        os.makedirs(CONF_DIR, exist_ok=True)
        path = os.path.join(CONF_DIR, "config.json")
        tmp = path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(cfg, f, ensure_ascii=False, indent=1)
        os.replace(tmp, path)
    except Exception:
        pass


def load_saver_cfg():
    try:
        with open(SCREENSAVER_CFG, encoding="utf-8") as f:
            cfg = json.load(f)
    except Exception:
        cfg = {}
    files = screensaver_files()
    names = [f["name"] for f in files]
    cfg.setdefault("autoLock", True)
    if cfg.get("file") not in names:
        cfg["file"] = names[0] if names else ""
    return cfg


def save_saver_cfg(cfg):
    os.makedirs(os.path.dirname(SCREENSAVER_CFG), exist_ok=True)
    with open(SCREENSAVER_CFG, "w", encoding="utf-8") as f:
        json.dump(cfg, f, ensure_ascii=False, indent=1)

# ---------------- 静态资产版本 ----------------
_WEB_BUILD = {"at": 0.0, "id": ""}


def web_build():
    """web/ 下所有文件的 mtime 汇总成一个版本号：改了前端，页面自己会重载。"""
    now = time.time()
    if now - _WEB_BUILD["at"] < 2 and _WEB_BUILD["id"]:
        return _WEB_BUILD["id"]
    stamp = 0.0
    for dirpath, _dirs, names in os.walk(WEB):
        for n in names:
            try:
                stamp = max(stamp, os.path.getmtime(os.path.join(dirpath, n)))
            except OSError:
                pass
    _WEB_BUILD.update(at=now, id=("%.0f" % (stamp * 1000)))
    return _WEB_BUILD["id"]


# ---------------- sidecar ----------------
# ---------------- sidecar ----------------
def _active_coms():
    """HKLM\\HARDWARE\\DEVICEMAP\\SERIALCOMM：当前真正在场的串口名（COM19 之类）。"""
    try:
        import winreg
    except Exception:
        return set()
    out = set()
    try:
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"HARDWARE\DEVICEMAP\SERIALCOMM") as k:
            for i in range(256):
                try:
                    _name, val, _t = winreg.EnumValue(k, i)
                except OSError:
                    break
                if isinstance(val, str) and val.upper().startswith("COM"):
                    out.add(val.upper())
    except OSError:
        pass
    return out


def _windows_qdc1_com():
    """Windows：QDC1 数据口是复合设备的 **MI_20** 接口（= USB interface 32）。

    注册表 `Enum\\USB\\VID_2C7C&PID_0901&MI_20\\<实例>\\Device Parameters\\PortName` 里有它的口名，
    但**拔插 / 换 USB 口之后会留下旧实例**（明明已经变成 COM22，注册表还写着 COM15）。
    所以只认「同时出现在 SERIALCOMM（当前在场串口）里」的那个；都不匹配就返回空，
    由 `_probe_qdc1()` 用 sidecar 的 `--probe` 真正问一遍。
    """
    try:
        import winreg
    except Exception:
        return ""
    active = _active_coms()
    base = r"SYSTEM\CurrentControlSet\Enum\USB\VID_2C7C&PID_0901&MI_20"
    try:
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, base) as k:
            for i in range(16):
                try:
                    inst = winreg.EnumKey(k, i)
                except OSError:
                    break
                try:
                    with winreg.OpenKey(k, inst + r"\Device Parameters") as dp:
                        port = str(winreg.QueryValueEx(dp, "PortName")[0] or "")
                except OSError:
                    continue
                if port and (not active or port.upper() in active):
                    return port
    except OSError:
        pass
    return ""


def _probe_qdc1():
    """兜底：让 sidecar 自带探针逐个 COM 发 HELLO，返回真应答的那个口。"""
    try:
        out = subprocess.run([SIDECAR, "--probe"], capture_output=True, text=True,
                             encoding="utf-8", errors="replace", timeout=120)
    except Exception:
        return ""
    for line in (out.stdout or "").splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            ev = json.loads(line)
        except Exception:
            continue
        if ev.get("qdc1"):
            return str(ev.get("com") or "")
    return ""


def _check_qdc1(com):
    """只验一个 COM（sidecar --hello）。注册表里可能残留拔插前的旧口（COM15），
    光看 SERIALCOMM 不够，必须真发一次 HELLO 才能确认它就是 QDC1 数据口。"""
    try:
        out = subprocess.run([SIDECAR, "--hello", com], capture_output=True, text=True,
                             encoding="utf-8", errors="replace", timeout=8)
    except Exception:
        return False
    for line in (out.stdout or "").splitlines():
        line = line.strip()
        if line.startswith("{"):
            try:
                return bool(json.loads(line).get("qdc1"))
            except Exception:
                return False
    return False


# ---------------- 音频（喇叭）----------------
# 固件 QDC1 type 7 的子命令（flags）
AUDIO_QUERY, AUDIO_STOP, AUDIO_VOLUME, AUDIO_BEGIN, AUDIO_END, AUDIO_PLAY, AUDIO_VERIFY = range(7)
AUDIO_CHUNK = 16384          # type 8/9 单块上限（固件约定 1..16384）
# 固件只认 MP3，所以别的格式先在主机上用 ffmpeg 转成 128 kbps MP3 再上传
AUDIO_IN_MAX = 64 * 1024 * 1024   # 上传原始文件上限
AUDIO_MAX = 4 * 1024 * 1024  # demo 固件的上传上限


def _rotl32(x, n):
    return ((x << n) | (x >> (32 - n))) & 0xFFFFFFFF


def xxh32(data, seed=0):
    """标准 xxHash32（seed 0），和设备侧 / sidecar 的 XXH32 一致。

    只在音频上传时用来算整个 MP3 的哈希；4 MiB 纯 Python 大约 1–2 秒，
    结果按 (路径, 大小, mtime) 缓存，重复播放不会再算。
    """
    try:  # 装了 xxhash 就快得多（可选依赖）
        import xxhash  # type: ignore
        return xxhash.xxh32(data, seed=seed).intdigest()
    except Exception:
        pass
    P1, P2, P3, P4, P5 = 2654435761, 2246822519, 3266489917, 668265263, 374761393
    n = len(data)
    p = 0
    if n >= 16:
        v1 = (seed + P1 + P2) & 0xFFFFFFFF
        v2 = (seed + P2) & 0xFFFFFFFF
        v3 = seed & 0xFFFFFFFF
        v4 = (seed - P1) & 0xFFFFFFFF
        limit = n - 16
        while p <= limit:
            v1 = (_rotl32((v1 + int.from_bytes(data[p:p + 4], "little") * P2) & 0xFFFFFFFF, 13) * P1) & 0xFFFFFFFF
            v2 = (_rotl32((v2 + int.from_bytes(data[p + 4:p + 8], "little") * P2) & 0xFFFFFFFF, 13) * P1) & 0xFFFFFFFF
            v3 = (_rotl32((v3 + int.from_bytes(data[p + 8:p + 12], "little") * P2) & 0xFFFFFFFF, 13) * P1) & 0xFFFFFFFF
            v4 = (_rotl32((v4 + int.from_bytes(data[p + 12:p + 16], "little") * P2) & 0xFFFFFFFF, 13) * P1) & 0xFFFFFFFF
            p += 16
        h = (_rotl32(v1, 1) + _rotl32(v2, 7) + _rotl32(v3, 12) + _rotl32(v4, 18)) & 0xFFFFFFFF
    else:
        h = (seed + P5) & 0xFFFFFFFF
    h = (h + n) & 0xFFFFFFFF
    while p + 4 <= n:
        h = (_rotl32((h + int.from_bytes(data[p:p + 4], "little") * P3) & 0xFFFFFFFF, 17) * P4) & 0xFFFFFFFF
        p += 4
    while p < n:
        h = (_rotl32((h + data[p] * P5) & 0xFFFFFFFF, 11) * P1) & 0xFFFFFFFF
        p += 1
    h ^= h >> 15
    h = (h * P2) & 0xFFFFFFFF
    h ^= h >> 13
    h = (h * P3) & 0xFFFFFFFF
    return (h ^ (h >> 16)) & 0xFFFFFFFF


class Device:
    """独占 USB 的 sidecar 进程 + 自愈：进程死了/链路死了都自动重启并重新枚举设备。"""

    def __init__(self):
        self.lock = threading.Lock()
        self.session = 0
        self.proc = None
        self.fail_streak = 0     # 连续 ok=0 的帧/patch 数 → UI 提示拔插复位
        self.status = {"device": "connecting"}
        self.last = None
        self.last_event = 0.0
        self.user_brightness = None  # 本进程里用户手动设过亮度
        self.bright_pref = load_config().get("brightness")  # 用户偏好（持久化到 ~/.cs80_panel/config.json）
        self.bright_expect = None    # 我们刚发出去、还没确认的值
        self.need_restore = False    # 刚(重)连上：第一条 state 用来“恢复偏好”，不拿来学习
        self.spawned_at = 0.0  # 本次 sidecar 启动时间，用来判断「刚起来就死」
        self.quick_deaths = 0  # 连续「刚起来就死」的次数 → 退避，别一秒一轮地重枚举 USB
        self.com = ""  # Windows：QDC1 数据串口（自动从注册表识别）
        self.audio_lock = threading.Lock()  # 上传/播放串行，避免两个请求交错
        self._audio_hash = {}  # (路径, 大小, mtime) → XXH32，重复播放不重算
        # ---- 发往 sidecar 的写队列 ----
        # 为什么不让 HTTP 线程直接写 sidecar 的 stdin：设备偶尔会 2~3 秒才回 ACK，
        # 期间 sidecar 不读 stdin，管道写满后 HTTP 请求就会卡住 → 浏览器 pusher.busy
        # 永远为 true，整条链路停摆。改成有界队列 + 单独写线程：HTTP 立刻返回，
        # 积压的帧按序补发，设备慢只会让画面晚一点，不会把上层一起拖死。
        # 只留很小的缓冲：设备正常 5 ms 回 ACK，4 条足够；队列一旦变大，
        # sidecar 会在设备卡顿后把积压一口气灌进去 → 固件接收超时（status=8）雪崩。
        self.q = queue.Queue(maxsize=4)
        self._q_lock = threading.Lock()
        self._q_bytes = 0
        self.Q_MAX_BYTES = 6 << 20  # 一条整帧 3 MB + 若干 patch；再多就返回 ok:false
        threading.Thread(target=self._writer, daemon=True).start()
        threading.Thread(target=self._supervise, daemon=True).start()
        threading.Thread(target=self._poll_state, daemon=True).start()

    def _writer(self):
        while True:
            sess, payload = self.q.get()
            with self._q_lock:
                self._q_bytes -= len(payload)
            # 换了 sidecar 会话：旧会话的帧（尤其是相对上一帧的 patch）已经没意义
            if sess != self.session:
                continue
            proc = self.proc
            if not proc or proc.poll() is not None or not proc.stdin:
                continue
            try:
                proc.stdin.write(payload)
                proc.stdin.flush()
            except (BrokenPipeError, OSError):
                self.status = dict(self.status, device="down")
            except Exception:
                pass

    def flush_pending(self):
        """整帧是完整快照：入队前把还没发出去的旧 patch 全丢掉。
        否则设备卡在 patch 重试里时，整帧要排在 3~4 条 patch 后面（每条可能 9 s），
        屏上「等待连接」会拖很久。"""
        with self._q_lock:
            while True:
                try:
                    _sess, payload = self.q.get_nowait()
                except queue.Empty:
                    break
                self._q_bytes -= len(payload)

    def _spawn(self):
        # 同时只能有一个 sidecar 持有 USB：先确认旧的已经退出，否则 Windows 上两个句柄
        # 会同时打开同一个 CDC 口 → 回包被抢走，看起来就是“时通时不通/卡住”。
        old = self.proc
        if old is not None and old.poll() is None:
            try:
                old.kill()
                old.wait(timeout=5)
            except Exception:
                pass
        # 上一次 host 被 Ctrl-C/kill 时留下的孤儿 sidecar 还握着 USB 接口（macOS 是整机独占），
        # 新 sidecar 只会一直「打不开设备 (code=-1)」→ 顺手清掉，反正是可执行路径精确匹配。
        if os.name != "nt":
            try:
                subprocess.run(["pkill", "-f", SIDECAR], timeout=3,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except Exception:
                pass
        self.spawned_at = time.time()
        self.session += 1
        os.makedirs(CONF_DIR, exist_ok=True)
        err = open(SIDECAR_LOG, "ab")
        cmd = [SIDECAR]
        if os.name == "nt":
            env_com = os.environ.get("CS80_COM")
            com = env_com
            if com and not _check_qdc1(com):
                # 显式指定也可能过期（拔插后 COM 号会变）：不应答就回退到自动探测
                print("CS80: CS80_COM=%s 不应答 QDC1，改为自动探测" % com, flush=True)
                com = ""
            if not com:
                # 依次验证：上次用的口 → 注册表映射的口；都不应答再全盘扫描。
                # 注册表在拔插/重枚举后会残留旧实例（比如 COM15），不能直接信任。
                active = _active_coms()
                for cand in (self.com, _windows_qdc1_com()):
                    if cand and (not active or cand.upper() in active) and _check_qdc1(cand):
                        com = cand
                        break
            if not com:
                com = _probe_qdc1()
            if not com:
                err.close()
                self.status = {"device": "down", "reason": "no-qdc1-port"}
                return False
            if com != self.com:
                print("CS80: 自动识别 QDC1 串口 = %s" % com, flush=True)
            self.com = com
            cmd += ["--com", com]
        # USB 全速（12 Mbps）下整帧裸块（2 MB）比「LZ4 等 3 s 超时 + 再发裸块」快一倍，
        # 让 sidecar 直接跳过 LZ4（只在 Linux/sysfs 能读到 speed 时做）。
        env = None
        if os.name != "nt":
            # 固件的整帧 LZ4 解码对某些块会卡 3 s（status=8），随后 patch 也会跟着超时；
            # 整帧现在只在重连/出错时发（60 分钟兜底），裸块 2 MB 在 480 Mbps 上约 0.2 s，
            # 直接跳过 LZ4 更稳。
            env = dict(os.environ, CS80_RAW_FULL="1")
        try:
            self.proc = subprocess.Popen(
                cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, bufsize=0, env=env
            )
        except FileNotFoundError:
            self.status = {"device": "no-sidecar"}
            return False
        finally:
            err.close()
        self.status = {"device": "connecting"}
        self.last_event = time.time()
        threading.Thread(target=self._reader, args=(self.proc,), daemon=True).start()
        return True

    def _reader(self, proc):
        for line in proc.stdout:
            try:
                ev = json.loads(line)
            except Exception:
                continue
            name = ev.get("event")
            self.last_event = time.time()
            # 音频事件很多（上传时每块一条），不要把“最近事件”挤成 audio；
            # 它们只进 status["audio"]，供 UI 的喇叭状态显示。
            if name not in ("audio", "audio-data", "pcm"):
                self.last = ev
            if name == "ready":
                self.status = dict(self.status, device="up", ready=ev)
                if BRIGHTNESS_PIN:
                    self.bright_expect = BRIGHTNESS_PIN
                    self.control(1, BRIGHTNESS_PIN)   # 强制模式（CS80_BRIGHTNESS=xxx）
                else:
                    # 不盲设，先读一次状态：拿它决定“要不要把用户偏好恢复回去”
                    self.need_restore = True
                    self.control(4)
            elif name == "features":
                self.status = dict(self.status, features=ev)
            elif name == "lost":
                self.status = dict(self.status, device="down", reason=ev.get("reason"))
            elif name == "exit":
                self.status = dict(self.status, device="down")
            elif name in ("frame", "patch"):
                extra = {"fw": ev["status"]} if "status" in ev else {}
                self.status = dict(
                    self.status, last=name, wait_ms=ev.get("ms"), wire=ev.get("wire"), ok=ev.get("ok"),
                    code=ev.get("code"), **extra
                )
                # 连续收不到 patch：多半是固件接收状态机卡死（status=8），重发/整帧都没用，
                # 只有断电/拔插 Type-C 重新枚举能清。这里只标记，让 UI 明确告诉人。
                if ev.get("ok") == 1:
                    self.fail_streak = 0
                    if self.status.get("needs_replug"):
                        self.status = dict(self.status, needs_replug=False)
                else:
                    self.fail_streak += 1
                    if self.fail_streak >= 3:
                        self.status = dict(self.status, needs_replug=True)
            elif name in ("brightness", "power", "state"):
                self.status = dict(self.status, **{name: {k: v for k, v in ev.items() if k != "event"}})
                if name == "state":
                    self._bright_state(ev.get("brightness"))
            elif name in ("audio", "audio-data", "pcm"):
                # 音频事件带自增 n，上传流程用它做同步；status 里保留最后一条供 UI 显示
                self.status = dict(self.status, audio=ev, audio_n=ev.get("n"))
        proc.wait()
        if self.proc is proc:
            self.proc = None
            self.status = dict(self.status, device="down")

    # ---- 亮度：记住用户选的值，不自己改 ----
    def _set_bright(self, value):
        """主动设亮度（用户操作 / 连上后恢复偏好）。记下期望值供回读确认，并持久化。"""
        cfg = load_config()
        self.bright_pref = value
        self.bright_expect = value
        if cfg.get("brightness") != value:
            cfg["brightness"] = value
            save_config(cfg)
        self.control(1, value)

    def _bright_state(self, cur):
        """收到 HELLO flags=10（亮度/背光/edges）时决定要不要动亮度。

        * 强制模式（`CS80_BRIGHTNESS=N`）：一直拉回 N
        * 刚（重）连上：固件会回到默认 100 → 把用户偏好恢复回去
        * 稳定运行中：设备上的 +/- 键改了亮度 → 以设备为准，学下来并持久化，**不跟用户抢**
        这样“手动调到 50%”不会再几十秒后被刷回 100%，而断电重连后也能回到 50%。
        """
        if not isinstance(cur, int):
            return
        if BRIGHTNESS_PIN:
            if cur != BRIGHTNESS_PIN:
                self.bright_expect = BRIGHTNESS_PIN
                self.control(1, BRIGHTNESS_PIN)
            return
        if self.need_restore:
            self.need_restore = False
            if self.bright_pref is not None and cur != self.bright_pref:
                self._set_bright(self.bright_pref)
            return
        if self.bright_expect is not None:
            self.bright_expect = None
            if cur != self.bright_pref:
                self._set_bright(cur)   # 设的瞬间又被按键改了 → 听用户的
            return
        if self.bright_pref is None or cur != self.bright_pref:
            self._set_bright(cur)       # 首次运行 / 设备上按了 +/-：学下来

    def _supervise(self):
        backoff = 1.0
        while True:
            proc = self.proc
            if proc is None or proc.poll() is not None:
                # 链路死得快（握手通、推帧就 no-ack-data）时会 5 秒一轮地重启 sidecar：
                # 退避一下，既少骚扰设备，也让日志/提示不至于刷屏。
                if self.spawned_at and 0 < time.time() - self.spawned_at < 8:
                    self.quick_deaths += 1
                else:
                    self.quick_deaths = 0
                if self.quick_deaths:
                    time.sleep(min(2 ** self.quick_deaths, 10))
                if not self._spawn():
                    time.sleep(backoff)
                    backoff = min(backoff * 2, 5.0)
                else:
                    # 等它握手成功（ready）或直接退出；没 ready 就退避重试，避免疯狂重枚举 USB。
                    # 注意 sidecar 自己的 HELLO 重试预算是 10 s 量级（刚上电/刚 USB reset 后
                    # 固件 USB 初始化要几秒），这里杀太快会让它永远来不及 ready。
                    deadline = time.time() + 15
                    while time.time() < deadline and self.proc is not None and self.status.get("device") != "up":
                        time.sleep(0.1)
                    if self.status.get("device") == "up":
                        backoff = 1.0
                    else:
                        self.status = dict(self.status, device="down", reason="open-failed")
                        if os.name == "nt" and not os.environ.get("CS80_COM"):
                            self.com = ""   # 下次重新探测（别在错的 COM 上死循环）
                        if self.proc is not None:
                            self.proc.kill()
                        time.sleep(backoff)
                        backoff = min(backoff * 2, 5.0)
            elif time.time() - self.last_event > 10:
                # 进程活着但 10 秒没有任何事件（正常保活 1.5s 一条）→ 链路已死，杀掉重启
                self.status = dict(self.status, device="down", reason="stalled")
                proc.kill()
                try:
                    proc.wait(timeout=5)
                except Exception:
                    pass
            time.sleep(0.5)

    def _poll_state(self):
        """定期回读设备亮度/待机状态，顺带给 UI 用（顺带当心跳）。"""
        time.sleep(2)
        while True:
            self.control(4)
            time.sleep(10)

    def reconnect(self):
        proc = self.proc
        if proc and proc.poll() is None:
            proc.kill()
        return True

    def send(self, payload, timeout=0.5):
        """入队即返回（不是「已写进 USB」）。队列满/设备不在线才返回 False。"""
        if self.status.get("device") != "up":
            return False
        n = len(payload)
        with self._q_lock:
            if self.q.full() or self._q_bytes + n > self.Q_MAX_BYTES:
                return False
            self._q_bytes += n
        try:
            self.q.put_nowait((self.session, payload))
            return True
        except queue.Full:
            with self._q_lock:
                self._q_bytes -= n
            return False

    def control(self, cmd, arg=0):
        return self.send(b"\x03" + bytes([cmd, arg]))

    # ---- 音频（喇叭）----
    def _wait_audio(self, n0, sub=None, timeout=10.0):
        """等 sidecar 的音频事件（带自增 n），避免和上一次命令的回包搞混。"""
        deadline = time.time() + timeout
        while time.time() < deadline:
            ev = self.status.get("audio") or {}
            if ev.get("n", 0) > n0 and (sub is None or ev.get("sub") == sub):
                return ev
            time.sleep(0.005)
        return None

    def audio(self, sub, offset=0, raw=0, payload=b"", timeout=15.0):
        """发一条 QDC1 type 7（音频控制），返回设备 ACK 解析后的事件。"""
        n0 = int(self.status.get("audio_n") or 0)
        msg = b"\x05" + bytes([sub, 0, 0, 0]) + struct.pack("<III", len(payload), offset, raw) + payload
        if not self.send(msg):
            return {"ok": False, "err": "device-down"}
        ev = self._wait_audio(n0, sub, timeout)
        return ev or {"ok": False, "err": "timeout"}

    def audio_block(self, offset, chunk):
        """发一块 MP3（type 8），等这一块的 ACK（detail = 累计字节）。"""
        n0 = int(self.status.get("audio_n") or 0)
        msg = b"\x06" + struct.pack("<II", offset, len(chunk)) + chunk
        if not self.send(msg):
            return {"ok": False, "err": "device-down"}
        deadline = time.time() + 5
        while time.time() < deadline:
            ev = self.status.get("audio") or {}
            if ev.get("n", 0) > n0 and ev.get("event") == "audio-data" and ev.get("offset") == offset:
                return ev
            time.sleep(0.005)
        return {"ok": False, "err": "timeout"}

    def _upload_locked(self, data, h):
        """调用方已持有 audio_lock：开始上传 → 分块 → 结束并核对文件哈希。"""
        r = self.audio(AUDIO_BEGIN, offset=len(data), raw=h, timeout=20)
        if not r.get("ok"):
            return r
        for off in range(0, len(data), AUDIO_CHUNK):
            r = self.audio_block(off, data[off:off + AUDIO_CHUNK])
            if not r.get("ok"):
                return dict(r, offset=off)
        r = self.audio(AUDIO_END, timeout=30)
        if not r.get("ok"):
            return r
        got = r.get("detail")
        if got and got != h:
            return {"ok": False, "err": "hash-mismatch", "expect": h, "got": got}
        return {"ok": True, "size": len(data), "hash": h}

    def play_uploaded(self, retry=True):
        """播放设备上已有的文件。PLAY 回 status=4（BAD_STATE）时多半是音频路由被复位了：
        读诊断（sub 8）→ 不是 speaker 就改回来（sub 9=0）→ 音量为 0 就设 70 → 重试一次。"""
        r = self.audio(AUDIO_PLAY, timeout=20)
        if r.get("ok") or not retry or r.get("status") != 4:
            return r
        d = self.audio(8)  # 诊断：rx=路由
        if d.get("ok") and d.get("rx") not in (None, 0):
            self.audio(9, offset=0)
        q = self.audio(AUDIO_QUERY)
        if q.get("ok") and not q.get("rx"):
            self.audio(AUDIO_VOLUME, offset=70)
        return self.audio(AUDIO_PLAY, timeout=20)

    def upload_mp3(self, data):
        if not data:
            return {"ok": False, "err": "empty"}
        if len(data) > AUDIO_MAX:
            return {"ok": False, "err": "too-large"}
        h = xxh32(data)
        with self.audio_lock:
            return self._upload_locked(data, h)

    def play_file(self, path):
        """上传（设备上已有同哈希文件则跳过）并播放一个 MP3。"""
        path = os.path.abspath(path)
        try:
            st = os.stat(path)
        except OSError as exc:
            return {"ok": False, "err": str(exc)}
        if st.st_size > AUDIO_MAX:
            return {"ok": False, "err": "too-large"}
        with open(path, "rb") as f:
            data = f.read()
        key = (path, st.st_size, int(st.st_mtime))
        h = self._audio_hash.get(key)
        if h is None:
            h = xxh32(data)
            self._audio_hash[key] = h
        with self.audio_lock:
            v = self.audio(AUDIO_VERIFY, timeout=15)
            if not (v.get("ok") and v.get("detail") == h):
                r = self._upload_locked(data, h)
                if not r.get("ok"):
                    return r
            r = self.play_uploaded()
            return dict(r, size=len(data), hash=h)


_FFMPEG = {"path": None}


def ffmpeg_path():
    """找 ffmpeg：包装版 App 从 Finder 启动时 PATH 只有 /usr/bin:/bin，which 找不到 Homebrew 的。"""
    if _FFMPEG["path"] is not None:
        return _FFMPEG["path"]
    cand = [os.environ.get("CS80_FFMPEG"), shutil.which("ffmpeg")]
    cand += ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg", "/opt/local/bin/ffmpeg",
             os.path.join(HERE, "ffmpeg.exe"), os.path.join(HERE, "ffmpeg")]
    found = next((c for c in cand if c and os.path.isfile(c) and os.access(c, os.X_OK)), "")
    _FFMPEG["path"] = found
    return found


def audio_to_mp3(data, name=""):
    """任意音视频 → 128 kbps / 44.1 kHz / 双声道 MP3（已经是 MP3 就原样返回）。

    返回 (mp3_bytes, info)。固件只解码 MP3，WAV/FLAC/M4A… 都在主机侧转好再上传。
    """
    ext = os.path.splitext(str(name or ""))[1].lower()
    is_mp3 = ext == ".mp3" or data[:3] == b"ID3" or (len(data) > 1 and data[0] == 0xFF and (data[1] & 0xE0) == 0xE0)
    if is_mp3:
        return data, {"converted": False, "format": (ext[1:] or "mp3")}
    ff = ffmpeg_path()
    if not ff:
        raise RuntimeError("非 MP3 需要 ffmpeg 转换（macOS: brew install ffmpeg）")
    tmp = tempfile.mkdtemp(prefix="cs80-audio-")
    src = os.path.join(tmp, "in" + (ext or ".bin"))
    dst = os.path.join(tmp, "out.mp3")
    try:
        with open(src, "wb") as f:
            f.write(data)
        t0 = time.time()
        r = subprocess.run([ff, "-y", "-v", "error", "-i", src, "-vn",
                            "-ac", "2", "-ar", "44100", "-b:a", "128k", dst],
                           capture_output=True, timeout=180)
        if r.returncode != 0 or not os.path.exists(dst):
            raise RuntimeError(((r.stderr or b"").decode("utf-8", "replace").strip().splitlines() or ["ffmpeg 转换失败"])[-1][:160])
        with open(dst, "rb") as f:
            out = f.read()
        return out, {"converted": True, "format": (ext[1:] or "?"), "ms": int((time.time() - t0) * 1000),
                     "inBytes": len(data), "outBytes": len(out)}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def tts_mp3(text):
    """macOS：用 say 生成语音，再让 ffmpeg 转成设备能解的 MP3。"""
    if sys.platform != "darwin":
        raise RuntimeError("say 播报只在 macOS 可用；其它平台请直接上传 MP3")
    ff = ffmpeg_path()
    if not ff:
        raise RuntimeError("需要 ffmpeg 才能把语音转成 MP3（brew install ffmpeg）")
    with tempfile.TemporaryDirectory() as d:
        aiff, mp3 = os.path.join(d, "say.aiff"), os.path.join(d, "say.mp3")
        subprocess.run(["say", "-o", aiff, text], check=True, timeout=60)
        subprocess.run(
            [ff, "-y", "-loglevel", "error", "-i", aiff, "-codec:a", "libmp3lame", "-q:a", "5", mp3],
            check=True, timeout=120,
        )
        with open(mp3, "rb") as f:
            return f.read()


# ---------------- 数据源 ----------------
class Cache:
    def __init__(self):
        self.lock = threading.Lock()
        self.items = {}

    def get(self, key, ttl, fn):
        now = time.time()
        with self.lock:
            hit = self.items.get(key)
            if hit and now - hit[0] < ttl:
                return hit[1]
        try:
            val = fn()
        except Exception as exc:  # 抓不到就把上次的值顶上去（标 stale），别让屏清空
            with self.lock:
                hit = self.items.get(key)
            if hit and isinstance(hit[1], dict):
                return dict(hit[1], stale=True, fetchedAt=int(hit[0] * 1000))
            return {"error": str(exc), "stale": True}
        with self.lock:
            self.items[key] = (now, val)
        # fetchedAt：前端据此显示「xx:xx 更新」；毫秒时间戳，和 JS Date.now() 同一口径
        if isinstance(val, dict):
            return dict(val, fetchedAt=int(now * 1000), stale=False)
        return val


CACHE = Cache()


def http_json(url, headers=None, timeout=6):
    req = urllib.request.Request(url, headers=headers or {"User-Agent": "cs80-panel/1.0"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8", "replace"))


def _shortcut_weather(city):
    """可选：让用户自己建一个快捷指令（Shortcuts）读系统天气，输出 JSON。
    约定：shortcuts run "$CS80_WEATHER_SHORTCUT" 的文本输出为
    {"temp":16,"low":15,"high":19,"code":3,"city":"上海"}
    """
    name = os.environ.get("CS80_WEATHER_SHORTCUT")
    if not name:
        raise RuntimeError("未配置快捷指令")
    out = subprocess.run(["shortcuts", "run", name], capture_output=True, text=True, timeout=20)
    text = (out.stdout or "").strip()
    if not text:
        raise RuntimeError("快捷指令没有输出")
    data = json.loads(text)
    data.setdefault("city", city)
    data["source"] = "shortcut"
    return data


def weather(city):
    if os.environ.get("CS80_WEATHER_SHORTCUT"):
        try:
            return _shortcut_weather(city)
        except Exception:
            pass  # 快捷指令没配好就回退到 open-meteo
    geo = http_json("https://geocoding-api.open-meteo.com/v1/search?count=1&language=zh&name=" + urllib.parse.quote(city))
    hits = geo.get("results") or []
    if not hits:
        return {"city": city, "error": "找不到城市", "stale": True}
    g = hits[0]
    wx = http_json(
        "https://api.open-meteo.com/v1/forecast?latitude=%s&longitude=%s&current=temperature_2m,relative_humidity_2m,"
        "weather_code,wind_speed_10m&daily=temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=1"
        % (g["latitude"], g["longitude"])
    )
    cur, day = wx.get("current", {}), wx.get("daily", {})
    return {
        "city": g.get("name") or city,
        "temp": cur.get("temperature_2m"),
        "humidity": cur.get("relative_humidity_2m"),
        "wind": cur.get("wind_speed_10m"),
        "code": cur.get("weather_code"),
        "high": (day.get("temperature_2m_max") or [None])[0],
        "low": (day.get("temperature_2m_min") or [None])[0],
        "source": "open-meteo",
    }


# ---------- 行情数据源 ----------
# 优先用 efinance / akshare（装了才走，字段更全）；都没装就用东财/新浪的 HTTP 接口
# （efinance 本身也是包这两个接口，所以结果一致，只是没有 pandas 那层便利）。
def _opt(mod):
    try:
        return __import__(mod)
    except Exception:
        return None


FOREIGN = re.compile(r"^\d+\.[A-Za-z0-9$]+$")  # 东财带市场号的代码：100.SPX / 103.ES00Y


def _is_foreign(symbol):
    return bool(FOREIGN.match(str(symbol).strip()))


def _secid(symbol):
    """sh000001 -> 1.000001；100.SPX / 103.ES00Y 原样透传（东财 secid 格式）"""
    if _is_foreign(symbol):
        return str(symbol).strip()
    t = str(symbol).lower().replace(".", "")
    if t.startswith("sh"):
        return "1." + t[2:]
    if t.startswith("sz"):
        return "0." + t[2:]
    if t.startswith("bj"):
        return "0." + t[2:]
    if t.isdigit() and len(t) == 6:
        return ("1." if t[0] in "56" else "0.") + t
    raise RuntimeError("代码格式：sh000001 / sz399001 / 600519")


def _plain(symbol):
    return str(symbol).lower().replace("sh", "").replace("sz", "").replace("bj", "").replace(".", "")


def _sina_cn_code(symbol):
    """002654 -> sz002654；600519 -> sh600519（新浪只认带前缀的代码）"""
    t = str(symbol).strip().lower()
    if t.startswith(("sh", "sz", "bj")):
        return t
    if t.isdigit() and len(t) == 6:
        return ("sh" if t[0] in "569" else "sz") + t
    return t


def _sina_quote(symbol):
    symbol0 = str(symbol)
    symbol = _sina_cn_code(symbol0)
    url = "https://hq.sinajs.cn/list=" + urllib.parse.quote(symbol)
    req = urllib.request.Request(
        url, headers={"User-Agent": "Mozilla/5.0", "Referer": "https://finance.sina.com.cn/"}
    )
    with urllib.request.urlopen(req, timeout=6) as resp:
        text = resp.read().decode("gbk", "replace")
    m = re.search(r'="([^"]*)"', text)
    if not m or not m.group(1):
        raise RuntimeError("无数据（需要有效代码，如 sh000001 / sz399001 / sh600519）")
    f = m.group(1).split(",")
    is_index = symbol.startswith(("sh000", "sz399", "sh950"))
    if is_index:
        name, price, prev = f[0], float(f[1]), float(f[2])
    else:
        name, price, prev = f[0], float(f[3]), float(f[2])
    premarket = False
    if price <= 0:
        # 9:15–9:25 集合竞价阶段新浪 f[3] 是 0，但买一/卖一已有报价；
        # 用买一/卖一或昨收兜底，避免组件显示 0.00 / -100%。
        if not is_index:
            try:
                bid = float(f[6]) if len(f) > 6 else 0.0
                ask = float(f[7]) if len(f) > 7 else 0.0
            except ValueError:
                bid = ask = 0.0
            price = bid or ask or prev
        else:
            price = prev
        premarket = True
    return {
        "symbol": symbol0,
        "name": name,
        "price": price,
        "prev": prev,
        "change": round(price - prev, 3),
        "pct": round((price - prev) / prev * 100, 2) if prev else 0.0,
        "source": "sina-auction" if premarket else "sina",
        "premarket": premarket,
    }


SINA_HEADERS = {"User-Agent": "Mozilla/5.0", "Referer": "https://finance.sina.com.cn/"}


def _sina_http(url, timeout=10, retries=1):
    """新浪的行情接口偶发读超时（尤其美股分时），失败重试一次再往上抛。"""
    last = None
    for _ in range(retries + 1):
        try:
            req = urllib.request.Request(url, headers=SINA_HEADERS)
            with urllib.request.urlopen(req, timeout=timeout) as resp:
                return resp.read().decode("gbk", "replace")
        except Exception as exc:
            last = exc
            time.sleep(0.4)
    raise last


def _sina_foreign_id(symbol):
    """东财外盘代码 → 新浪代码。
    103.ES00Y（标普期货）→ hf_ES / 分时 ES；100.SPX/NDX/DJIA → gb_$inx/ndx/dji。
    返回 (行情代码, 期货分钟代码或 None)。"""
    s = str(symbol).strip()
    m = re.match(r"^103\.([A-Za-z0-9]+?)00Y$", s)
    if m:
        code = m.group(1).upper()
        return "hf_" + code, code
    m = re.match(r"^100\.(SPX|NDX|DJI|DJIA)$", s, re.I)
    if m:
        idx = {"SPX": "gb_$inx", "NDX": "gb_$ndx", "DJI": "gb_$dji", "DJIA": "gb_$dji"}[m.group(1).upper()]
        return idx, None
    return None, None


def _sina_foreign_quote(symbol):
    """外盘（指数/期货）快照：东财挂掉时用新浪的公开行情顶一下。"""
    qid, _fut = _sina_foreign_id(symbol)
    if not qid:
        raise RuntimeError("新浪不支持这个代码")
    # 注意：gb_$inx 里的 $ 不能编码（%24 会被新浪当成另一个不存在的代码，返回空串）
    text = _sina_http("https://hq.sinajs.cn/list=" + urllib.parse.quote(qid, safe="$"))
    m = re.search(r'="([^"]*)"', text)
    if not m or not m.group(1):
        raise RuntimeError("新浪无数据")
    f = m.group(1).split(",")
    if qid.startswith("hf_"):
        price, prev, name = float(f[0]), float(f[7]), (f[13] if len(f) > 13 else symbol)
    else:  # gb_$xxx：name,price,pct,time,change,...
        name, price, change = f[0], float(f[1]), float(f[4])
        prev = price - change
    return {
        "symbol": symbol,
        "name": name or symbol,
        "price": price,
        "prev": prev,
        "change": round(price - prev, 3),
        "pct": round((price - prev) / prev * 100, 2) if prev else 0.0,
        "source": "sina",
    }


def _sina_foreign_minute(symbol):
    """外盘期货分时：新浪 GlobalFuturesMinLine（指数没有可用分时接口）。"""
    _qid, fut = _sina_foreign_id(symbol)
    if not fut:
        raise RuntimeError("这个外盘代码没有分时源")
    text = _sina_http(
        "https://stock2.finance.sina.com.cn/futures/api/jsonp.php/var%20t=/"
        "GlobalFuturesService.getGlobalFuturesMinLine?symbol=" + urllib.parse.quote(fut)
    )
    a, b = text.find("("), text.rfind(")")
    if a < 0 or b <= a:
        raise RuntimeError("新浪分时格式异常")
    obj = json.loads(text[a + 1:b])
    rows = obj.get("minLine_1d") or []
    if len(rows) < 2:
        raise RuntimeError("没有分时数据")
    pre = float(rows[0][1])
    points = []
    # 第一行是交易日/昨收/开盘分钟/开盘价；后面是 ["07:01", 价格, ...]
    points.append([str(rows[0][4])[-5:] if len(rows[0]) > 4 else "07:00", float(rows[0][5] if len(rows[0]) > 5 else pre)])
    for row in rows[1:]:
        if len(row) >= 2:
            points.append([str(row[0])[-5:], float(row[1])])
    return {"symbol": symbol, "preClose": pre, "points": points, "source": "sina"}


def _sina_us_minute(symbol):
    """美股指数分时兜底：新浪 US_MinlineService（东财 trends2 挂掉时用）。
    minline_1[0].first_min = [日期, 时间, 昨收, 开盘, ...]，other_min = [[价格, 量], ...]"""
    m = re.match(r"^100\.(SPX|NDX|DJI|DJIA)$", str(symbol).strip(), re.I)
    if not m:
        raise RuntimeError("这个外盘代码没有分时源")
    code = {"SPX": ".INX", "NDX": ".NDX", "DJI": ".DJI", "DJIA": ".DJI"}[m.group(1).upper()]
    text = _sina_http(
        "https://stock.finance.sina.com.cn/usstock/api/jsonp.php/var%20t=/"
        "US_MinlineService.getMinline?symbol=" + code
    )
    a, b = text.find("("), text.rfind(")")
    if a < 0 or b <= a:
        raise RuntimeError("新浪分时格式异常")
    obj = json.loads(text[a + 1:b])
    block = (obj.get("minline_1") or [{}])[0] if isinstance(obj.get("minline_1"), list) else {}
    first = block.get("first_min") or []
    rows = block.get("other_min") or []
    if len(rows) < 2:
        raise RuntimeError("没有分时数据")
    pre = float(first[2]) if len(first) > 2 else float(rows[0][0])
    points = []
    for i, row in enumerate(rows):
        mm = 570 + i  # 美东 9:30 开盘，按分钟推时间（只用来排序/对齐，不画刻度）
        points.append(["%02d:%02d" % (mm // 60, mm % 60), float(row[0])])
    return {"symbol": symbol, "preClose": pre, "points": points, "source": "sina"}


def _sina_cn_minute(symbol):
    """A 股分时兜底：新浪 CN_MarketDataService（东财 trends2 挂掉时用）。"""
    sym = str(symbol).lower().replace(".", "")
    if not sym.startswith(("sh", "sz", "bj")):
        if not sym.isdigit():
            raise RuntimeError("代码格式：sh000001 / sz399001")
        sym = ("sh" if sym[0] in "56" else "sz") + sym
    # 注意：URL 里有 %20，不能用 % 格式化（会被当成格式符），用字符串拼接
    text = _sina_http(
        "https://quotes.sina.cn/cn/api/jsonp_v2.php/var%20t=/CN_MarketDataService.getKLineData"
        "?symbol=" + urllib.parse.quote(sym) + "&scale=1&ma=no&datalen=241"
    )
    a, b = text.find("("), text.rfind(")")
    if a < 0 or b <= a:
        raise RuntimeError("新浪分时格式异常")
    arr = json.loads(text[a + 1:b])
    if not arr:
        raise RuntimeError("没有分时数据")
    day = str(arr[-1].get("day"))[:10]
    today = datetime.datetime.now(
        datetime.timezone(datetime.timedelta(hours=8))
    ).strftime("%Y-%m-%d")
    if day != today:
        raise RuntimeError("没有今日分时数据")
    rows = [r for r in arr if str(r.get("day"))[:10] == day]
    if not rows:
        raise RuntimeError("没有分时数据")
    points = [[str(r["day"])[11:16], float(r["close"])] for r in rows]
    prev = [r for r in arr if str(r.get("day"))[:10] != day]
    pre = float(prev[-1]["close"]) if prev else float(rows[0]["open"])
    return {"symbol": symbol, "preClose": pre, "points": points, "source": "sina"}


_EM_DOWN = {"until": 0.0}


def _em_json(url):
    """东财接口统一入口：失败一次就熔断 60 秒（这台机器上 push2* 经常直接 Empty reply，
    每次重试都要卡几秒 TLS 握手，连带把 portfolio 拖慢）。"""
    left = _EM_DOWN["until"] - time.time()
    if left > 0:
        raise RuntimeError("东财暂不可用（%.0fs 后重试）" % left)
    try:
        return http_json(url, headers={"User-Agent": "Mozilla/5.0", "Referer": "https://quote.eastmoney.com/"})
    except Exception:
        _EM_DOWN["until"] = time.time() + 60
        raise


def _em_quote(symbol):
    """东财快照（A 股走 push2，外盘/期货走 push2delay，后者没有 302 拦截）"""
    fields = "f43,f44,f45,f46,f57,f58,f60,f169,f170"
    host = "push2delay" if _is_foreign(symbol) else "push2"
    url = "https://%s.eastmoney.com/api/qt/stock/get?invt=2&secid=%s" \
          "&ut=fa5fd1943c7b386f172d6893dbfba10b&fields=%s" % (host, _secid(symbol), fields)
    d = _em_json(url)
    data = d.get("data") or {}
    if not data:
        raise RuntimeError("东财无数据")
    price = data.get("f43", 0) / 100.0
    prev = data.get("f60", 0) / 100.0
    high = data.get("f44", 0) / 100.0
    low = data.get("f45", 0) / 100.0
    premarket = False
    if price <= 0 and prev > 0:
        price = prev
        premarket = True
    return {
        "symbol": symbol,
        "name": data.get("f58") or symbol,
        "price": price,
        "prev": prev,
        "high": high or None,
        "low": low or None,
        "change": round(price - prev, 3),
        "pct": round((price - prev) / prev * 100, 2) if prev else 0.0,
        "source": "eastmoney",
        "premarket": premarket,
    }


def stock(symbol):
    """现价快照：A 股 efinance → akshare → 新浪 → 东财；外盘东财 → 新浪
    （2026-11 实测：东财 push2* 从这台机器上直接 Empty reply，新浪外盘行情可用）"""
    if _is_foreign(symbol):
        try:
            return _em_quote(symbol)
        except Exception:
            return _sina_foreign_quote(symbol)
    ef = _opt("efinance")
    if ef is not None:
        try:
            row = ef.stock.get_latest_quote([_plain(symbol)]).iloc[0]
            price, prev = float(row["最新价"]), float(row["昨日收盘"])
            if price <= 0:
                raise RuntimeError("盘前无成交价")
            return {
                "symbol": symbol,
                "name": str(row.get("股票名称") or row.get("名称") or symbol),
                "price": price,
                "prev": prev,
                "high": float(row.get("最高") or 0) or None,
                "low": float(row.get("最低") or 0) or None,
                "change": round(price - prev, 3),
                "pct": round((price - prev) / prev * 100, 2) if prev else 0.0,
                "source": "efinance",
            }
        except Exception:
            pass
    ak = _opt("akshare")
    if ak is not None:
        try:
            df = ak.stock_bid_ask_em(symbol=_plain(symbol))
            row = dict(zip(df["item"], df["value"]))
            price, prev = float(row["最新"]), float(row["昨收"])
            if price <= 0:
                raise RuntimeError("盘前无成交价")
            return {
                "symbol": symbol,
                "name": symbol,
                "price": price,
                "prev": prev,
                "high": float(row.get("最高") or 0) or None,
                "low": float(row.get("最低") or 0) or None,
                "change": round(price - prev, 3),
                "pct": round((price - prev) / prev * 100, 2) if prev else 0.0,
                "source": "akshare",
            }
        except Exception:
            pass
    try:
        return _sina_quote(symbol)
    except Exception:
        return _em_quote(symbol)


def kline(symbol, limit=60):
    """日 K：efinance → akshare → 东财"""
    ef = None if _is_foreign(symbol) else _opt("efinance")
    if ef is not None:
        try:
            df = ef.stock.get_quote_history(_plain(symbol), klt=101)
            df = df.tail(limit)
            candles = [
                {"d": str(r["日期"]), "o": float(r["开盘"]), "c": float(r["收盘"]), "h": float(r["最高"]), "l": float(r["最低"])}
                for _, r in df.iterrows()
            ]
            name = str(df.iloc[-1].get("股票名称") or symbol)
            return _kline_payload(symbol, name, candles, "efinance")
        except Exception:
            pass
    ak = _opt("akshare")
    if ak is not None:
        try:
            df = ak.stock_zh_a_hist(symbol=_plain(symbol), period="daily", adjust="qfq").tail(limit)
            candles = [
                {"d": str(r["日期"]), "o": float(r["开盘"]), "c": float(r["收盘"]), "h": float(r["最高"]), "l": float(r["最低"])}
                for _, r in df.iterrows()
            ]
            return _kline_payload(symbol, symbol, candles, "akshare")
        except Exception:
            pass
    url = (
        "https://push2his.eastmoney.com/api/qt/stock/kline/get?secid=%s"
        "&ut=fa5fd1943c7b386f172d6893dbfba10b&fields1=f1,f2,f3,f4,f5,f6"
        "&fields2=f51,f52,f53,f54,f55,f56,f57&klt=101&fqt=1&end=20500101&lmt=%d"
    ) % (_secid(symbol), limit)
    d = _em_json(url)
    data = d.get("data") or {}
    rows = data.get("klines") or []
    if not rows:
        raise RuntimeError("没有 K 线数据")
    candles = []
    for row in rows:
        f = row.split(",")
        candles.append({"d": f[0], "o": float(f[1]), "c": float(f[2]), "h": float(f[3]), "l": float(f[4])})
    return _kline_payload(symbol, data.get("name") or symbol, candles, "eastmoney")


def _kline_payload(symbol, name, candles, source):
    last, prev = candles[-1]["c"], (candles[-2]["c"] if len(candles) > 1 else candles[-1]["o"])
    return {
        "symbol": symbol,
        "name": name,
        "candles": candles,
        "price": last,
        "pct": round((last - prev) / prev * 100, 2) if prev else 0.0,
        "source": source,
    }


def minute(symbol):
    """分时（当日 1 分钟）：A 股 efinance → akshare → 东财 trends2；外盘直接东财"""
    ef = None if _is_foreign(symbol) else _opt("efinance")
    if ef is not None:
        try:
            df = ef.stock.get_quote_history(_plain(symbol), klt=1)
            points = [[str(r["日期"])[-5:], float(r["收盘"])] for _, r in df.iterrows()]
            return {"symbol": symbol, "preClose": float(df.iloc[0]["开盘"]), "points": points, "source": "efinance"}
        except Exception:
            pass
    ak = None if _is_foreign(symbol) else _opt("akshare")
    if ak is not None:
        try:
            df = ak.stock_zh_a_hist_min_em(symbol=_plain(symbol), period="1")
            points = [[str(r["时间"])[-5:], float(r["收盘"])] for _, r in df.tail(241).iterrows()]
            return {"symbol": symbol, "preClose": points and points[0][1], "points": points, "source": "akshare"}
        except Exception:
            pass
    try:
        url = (
            "https://push2his.eastmoney.com/api/qt/stock/trends2/get?secid=%s"
            "&ut=fa5fd1943c7b386f172d6893dbfba10b&fields1=f1,f2,f3,f4,f5,f6,f7,f8,f9,f10,f11,f12,f13"
            "&fields2=f51,f53&iscr=0&ndays=1"
        ) % _secid(symbol)
        d = _em_json(url)
        data = d.get("data") or {}
        trends = data.get("trends") or []
        if not trends:
            raise RuntimeError("没有分时数据")
        points = []
        for row in trends:
            ts, price = row.split(",")[:2]
            points.append([ts[-5:], float(price)])
        return {
            "symbol": symbol,
            "name": data.get("name") or symbol,
            "preClose": float(data.get("preClose") or points[0][1]),
            "points": points,
            "source": "eastmoney",
        }
    except Exception:
        # 东财被墙/限流时用新浪：外盘走 GlobalFuturesMinLine，A 股走 CN_MarketDataService
        if _is_foreign(symbol):
            try:
                return _sina_foreign_minute(symbol)
            except Exception:
                return _sina_us_minute(symbol)
        return _sina_cn_minute(symbol)


def _pf_range(q):
    """分时 + 现价 → 本条行情的高/低（盘中休息时不轮询，屏上仍显示上午的高低点）。"""
    vals = []
    for p in q.get("points") or []:
        try:
            vals.append(float(p[1]))
        except (TypeError, ValueError, IndexError):
            pass
    for key in ("price", "high", "low"):
        try:
            v = float(q.get(key))
            if v:
                vals.append(v)
        except (TypeError, ValueError):
            pass
    if vals:
        q["high"] = max(vals)
        q["low"] = min(vals)
    return q


# 每只股票最后一次成功的结果：(时间, item)。行情接口偶发空数据/超时时用它顶一下，
# 免得组件上一秒还有分时、下一秒就变成「无分时」。
_PF_LAST = {}
PF_KEEP = 1800  # 秒：超过这个时间的旧分时不再复用（避免拿昨天的曲线当今天）


def _pf_item(sym):
    prev = _PF_LAST.get(sym)
    try:
        q = stock(sym)
        try:
            price = float(q.get("price") or 0)
        except (TypeError, ValueError):
            price = 0.0
        if price <= 0:  # 空快照/盘前无报价（接口抽风）当失败处理
            raise RuntimeError("空快照")
    except Exception:
        if prev:
            return dict(prev[1], stale=True)
        return {"symbol": sym, "error": "行情获取失败", "stale": True}
    try:
        m = minute(sym)
        q["points"] = m["points"]
        q["preClose"] = m.get("preClose")
    except Exception:
        # 分时拿不到就沿用上一份（有 & 不太旧），至少别让曲线整块消失
        stale = False
        if prev and prev[1].get("points") and time.time() - prev[0] < PF_KEEP:
            q["points"] = prev[1]["points"]
            q["preClose"] = prev[1].get("preClose", q.get("prev"))
            stale = True
        else:
            q["points"] = []
            q["preClose"] = q.get("preClose") or q.get("prev")
        q["stale"] = stale
    _pf_range(q)
    _PF_LAST[sym] = (time.time(), q)
    return q


def portfolio(symbols):
    """自选股：一次给「快照 + 分时」，给自定义页面和股票组件用"""
    items = [_pf_item(sym) for sym in [x.strip() for x in str(symbols).split(",") if x.strip()][:8]]
    return {"items": items, "source": "mixed",
            "stale": any(it.get("stale") for it in items)}


# ---------- 行情补充源（加密 / GitHub）----------
COIN_SYMBOL = {"bitcoin": "BTC", "ethereum": "ETH", "solana": "SOL", "binancecoin": "BNB",
               "ripple": "XRP", "dogecoin": "DOGE", "cardano": "ADA", "tron": "TRX",
               "toncoin": "TON", "avalanche-2": "AVAX", "polkadot": "DOT", "chainlink": "LINK"}


def crypto(ids):
    """coingecko 免 key 接口：/api/data/crypto?ids=bitcoin,ethereum"""
    want = [x.strip() for x in str(ids).split(",") if x.strip()][:8]
    d = http_json("https://api.coingecko.com/api/v3/simple/price?ids=%s&vs_currencies=usd&include_24hr_change=true"
                  % ",".join(want))
    coins = []
    for cid in want:
        row = d.get(cid) or {}
        if row.get("usd") is None:
            continue
        coins.append({"id": cid, "symbol": COIN_SYMBOL.get(cid, cid.upper()[:5]), "price": row["usd"],
                      "pct": round(row.get("usd_24h_change") or 0, 2)})
    return {"coins": coins, "source": "coingecko"}


def github(repo):
    """GitHub 仓库快照：/api/data/github?repo=owner/name（未授权 60 次/时）"""
    repo = str(repo).strip("/")
    d = http_json("https://api.github.com/repos/" + repo,
                  {"User-Agent": "cs80-panel/1.0", "Accept": "application/vnd.github+json"})
    return {"repo": d.get("full_name") or repo, "stars": d.get("stargazers_count"),
            "lang": d.get("language"), "issues": d.get("open_issues_count"),
            "pushed": d.get("pushed_at"), "source": "github"}


# ---------- 热榜资讯 ----------
# 接口照 newsnow（github.com/newsnext/newsnow）的公开端点：
# 每源单独 CACHE，一个源抓不到只让那个 Tab 空着，不影响别的 Tab；抓到过一次就沿用上次结果。
NEWS_UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
           "(KHTML, like Gecko) Chrome/120 Safari/537.36")


def _hot_number(text):
    """"1234 万热度" → 12340000；纯数字原样返回"""
    m = re.search(r"([\d.]+)\s*(万|亿)?", str(text or ""))
    if not m:
        return None
    scale = {"万": 10000, "亿": 100000000}.get(m.group(2), 1)
    return int(float(m.group(1)) * scale)


def _news_toutiao():
    d = http_json("https://www.toutiao.com/hot-event/hot-board/?origin=toutiao_pc", {"User-Agent": NEWS_UA})
    return [{"title": x.get("Title"), "hot": _hot_number(x.get("HotValue"))} for x in d.get("data") or []]


def _news_baidu():
    d = http_json("https://top.baidu.com/api/board?platform=pc&tab=realtime", {"User-Agent": NEWS_UA})
    cards = (d.get("data") or {}).get("cards") or []
    rows = (cards[0].get("content") if cards else []) or []
    return [{"title": x.get("query") or x.get("word"), "hot": _hot_number(x.get("hotScore"))} for x in rows]


def _news_zhihu():
    # 知乎对数据中心 IP 会要求登录/过验证；被拒就靠 CACHE 的旧值和其它源兜底
    d = http_json("https://www.zhihu.com/api/v3/feed/topstory/hot-lists/total?limit=50&desktop=true",
                  {"User-Agent": NEWS_UA, "Referer": "https://www.zhihu.com/hot"})
    return [{"title": (x.get("target") or {}).get("title"), "hot": _hot_number(x.get("detail_text"))}
            for x in d.get("data") or []]


def _news_weibo():
    d = http_json("https://weibo.com/ajax/side/hotSearch", {"User-Agent": NEWS_UA, "Referer": "https://weibo.com/"})
    rows = (d.get("data") or {}).get("realtime") or []
    return [{"title": x.get("word"), "hot": _hot_number(x.get("num"))}
            for x in rows if x.get("word") and not x.get("is_ad")]


def _news_v2ex():
    d = http_json("https://www.v2ex.com/api/topics/hot.json", {"User-Agent": NEWS_UA})
    return [{"title": x.get("title"), "hot": _hot_number(x.get("replies"))} for x in d or []]


def _news_juejin():
    d = http_json("https://api.juejin.cn/content_api/v1/content/article_rank?category_id=1&type=hot",
                  {"User-Agent": NEWS_UA})
    return [{"title": (x.get("content") or {}).get("title"),
             "hot": _hot_number((x.get("content_counter") or {}).get("hot_rank"))}
            for x in d.get("data") or []]


def _news_sspai():
    d = http_json("https://sspai.com/api/v1/article/tag/page/get?limit=40&offset=0&tag=%E7%83%AD%E9%97%A8%E6%96%87%E7%AB%A0",
                  {"User-Agent": NEWS_UA, "Referer": "https://sspai.com/"})
    return [{"title": x.get("title")} for x in d.get("data") or []]


def _news_ithome():
    d = http_json("https://api.ithome.com/json/newslist/news", {"User-Agent": NEWS_UA})
    return [{"title": x.get("title"), "hot": _hot_number(x.get("commentcount"))}
            for x in d.get("newslist") or []]


def _news_36kr():
    body = json.dumps({"partner_id": "wap", "param": {"siteId": 1, "platformId": 2}, "timestamp": 0}).encode()
    req = urllib.request.Request(
        "https://gateway.36kr.com/api/mis/nav/home/nav/rank/hot", data=body,
        headers={"User-Agent": NEWS_UA, "Content-Type": "application/json",
                 "Referer": "https://36kr.com/", "Connection": "close"})
    with urllib.request.urlopen(req, timeout=10) as resp:
        d = json.loads(resp.read().decode("utf-8", "replace"))
    rows = (d.get("data") or {}).get("hotRankList") or []
    return [{"title": (x.get("templateMaterial") or {}).get("widgetTitle")} for x in rows]


def _news_wscn():
    # 华尔街见闻快讯：标题大多为空，取正文首句
    d = http_json("https://api-one-wscn.awtmt.com/apiv1/content/lives?channel=global-channel&client=pc&limit=40",
                  {"User-Agent": NEWS_UA, "Referer": "https://wallstreetcn.com/"})
    out = []
    for x in (d.get("data") or {}).get("items") or []:
        title = (x.get("title") or x.get("content_text") or "").strip()
        if title:
            out.append({"title": title.split("\n")[0], "hot": _hot_number(x.get("score"))})
    return out


def _news_hn():
    # Hacker News 首页（用 Algolia 的公开接口，省得一条条取 item）
    d = http_json("https://hn.algolia.com/api/v1/search?tags=front_page&hitsPerPage=40",
                  {"User-Agent": NEWS_UA})
    return [{"title": x.get("title"), "hot": _hot_number(x.get("points"))} for x in d.get("hits") or []]


def _news_tieba():
    req = urllib.request.Request("https://tieba.baidu.com/hottopic/browse/topicList?res_type=1",
                                 headers={"User-Agent": NEWS_UA, "Referer": "https://tieba.baidu.com/"})
    with urllib.request.urlopen(req, timeout=10) as resp:
        html = resp.read().decode("utf-8", "replace")
    return [{"title": t} for t in re.findall(r'class="topic-text"[^>]*>([^<]+)<', html)]


# ---------- 假期 ----------
# 数据照 holiday-cn（github.com/NateScarlet/holiday-cn）：每年一份 JSON，国务院公布后更新。
# 只取 isOffDay 的日子（调休上班日丢掉），再把同名连续日期拼成一个假期区间。
HOLIDAY_URL = "https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/%d.json"


def _holiday_year(year):
    d = http_json(HOLIDAY_URL % year, {"User-Agent": NEWS_UA}, timeout=10)
    return sorted((x["date"], x.get("name") or "假期")
                  for x in d.get("days") or [] if x.get("isOffDay") and x.get("date"))


def _date(s):
    return datetime.date(*[int(v) for v in str(s).split("-")])


def holidays(today=None):
    today = today or datetime.date.today()
    days = []
    for y in (today.year, today.year + 1):
        val = CACHE.get("hol:%d" % y, 43200, lambda y=y: _holiday_year(y))
        if isinstance(val, list):
            days += val

    blocks = []
    for date_s, name in sorted(days):
        b = blocks[-1] if blocks else None
        if b and b["name"] == name and _date(b["end"]) + datetime.timedelta(days=1) == _date(date_s):
            b["end"] = date_s
        else:
            blocks.append({"name": name, "start": date_s, "end": date_s})

    out = []
    for b in blocks:
        start, end = _date(b["start"]), _date(b["end"])
        if end < today:
            continue
        out.append(dict(b, length=(end - start).days + 1,
                        until=max(0, (start - today).days),
                        left=max(0, (end - today).days + 1),
                        ongoing=start <= today <= end))
    out.sort(key=lambda x: x["start"])
    return {"next": out[0] if out else None, "upcoming": out[:8], "today": today.isoformat()}


NEWS = {
    "toutiao": _news_toutiao,
    "baidu": _news_baidu,
    "zhihu": _news_zhihu,
    "weibo": _news_weibo,
    "v2ex": _news_v2ex,
    "juejin": _news_juejin,
    "sspai": _news_sspai,
    "ithome": _news_ithome,
    "36kr": _news_36kr,
    "wscn": _news_wscn,
    "hackernews": _news_hn,
    "tieba": _news_tieba,
}


def _news_one(sid):
    fn = NEWS.get(sid)
    if not fn:
        return None
    return CACHE.get("news:" + sid, 180, fn)


def news(sources):
    """{sourceId: [{title, hot}]}，只抓请求里点名的几个源；有源回退到旧值就标 stale。
    多个源并发抓（最慢一个决定总耗时），别让前端为 12 个源等半分钟。"""
    ids = [s.strip() for s in str(sources).split(",") if s.strip()][:16]
    ids = [sid for sid in ids if sid in NEWS]
    out, failed = {}, 0
    if ids:
        with ThreadPoolExecutor(max_workers=min(8, len(ids))) as pool:
            results = list(pool.map(_news_one, ids))
        for sid, val in zip(ids, results):
            if isinstance(val, list):
                out[sid] = [x for x in val if isinstance(x, dict) and x.get("title")]
            else:
                out[sid] = []
                failed += 1
    # 只有「全挂了」才整体标 stale：单个源被墙（比如知乎要过验证）不该让整个组件报警
    if ids and failed == len(ids):
        out["stale"] = True
    return out



# ---------- 本机 Coding Agent 会话扫描 ----------
# 思路：会话都在本地落 jsonl（Pi / Claude Code / Codex），用文件的 mtime 当"最近活动"，
# 用进程是否存活 + 会话最后一条记录区分「运行中 / 等待确认 / 已结束」。
# procs：进程名/命令行 token。macOS 上是可执行文件名；Windows 上 npm 全局装的 CLI
# 实际跑成 node.exe + 一段带包名的命令行，所以把包名也列进来（两端通用、不会误伤）。
AGENT_SPECS = [
    {"id": "pi", "name": "Pi", "short": "Pi", "roots": ["~/.pi/agent/sessions"],
     "procs": ["pi", "pi-coding-agent"]},
    {"id": "claude", "name": "Claude Code", "short": "Claude", "roots": ["~/.claude/projects"],
     "procs": ["claude", "claude-code"]},
    {"id": "codex", "name": "Codex", "short": "Codex", "roots": ["~/.codex/sessions"],
     "procs": ["codex", "codex-cli"]},
    # DeepSeek Harness 是 Electron 应用，会话存在 IndexedDB 里，只能靠进程判断
    {"id": "deepseek", "name": "DeepSeek Harness", "short": "DeepSeek", "roots": [],
     "procs": ["DeepSeek Harness", "deepseek-harness"]},
]

_TITLE_CACHE = {}


_AGENT_WRAPPERS = {
    "node", "nodejs", "bun", "deno", "python", "python3", "ruby",
    "perl", "env", "npx", "pnpm", "yarn", "sh", "bash", "zsh",
}


def _agent_from_cmd(cmd):
    """从进程命令行识别 Agent；只检查可执行名和包装器的第一个路径参数。"""
    parts = str(cmd or "").split()
    if not parts:
        return None
    exe = os.path.basename(parts[0].strip("\"'")).lower()
    names = {exe}
    if exe in _AGENT_WRAPPERS:
        for arg in parts[1:5]:
            arg = arg.strip("\"'")
            if arg in ("-c", "-e"):
                break
            if arg.startswith("-"):
                continue
            names.update(x.lower() for x in re.split(r"[/\\]+", arg) if x)
            break
    for spec in AGENT_SPECS:
        for needle in spec["procs"]:
            n = needle.lower()
            if " " in n:
                if n in cmd.lower():
                    return spec["id"]
            elif exe == n or exe == n + ".exe" or n in names:
                return spec["id"]
    return None


def _etime_secs(text):
    """ps 的 [[dd-]hh:]mm:ss → 秒；解析失败返回 None。"""
    try:
        days = 0
        text = str(text).strip()
        if "-" in text:
            d, text = text.split("-", 1)
            days = int(d)
        parts = [int(x) for x in text.split(":")]
        if len(parts) == 2:
            h, m, s = 0, parts[0], parts[1]
        elif len(parts) == 3:
            h, m, s = parts
        else:
            return None
        return days * 86400 + h * 3600 + m * 60 + s
    except (TypeError, ValueError):
        return None


_AGENT_CPU = {}


def _proc_cpu_ticks(pid):
    """Linux /proc/<pid>/stat 的 utime+stime（clock ticks）；其他平台返回 None。"""
    try:
        with open("/proc/%d/stat" % pid, "r") as f:
            data = f.read()
        rest = data[data.rfind(")") + 2:].split()
        return int(rest[11]) + int(rest[12])
    except (OSError, ValueError, IndexError):
        return None


def _agent_process_info():
    """{agent_id: {pids, cwds, children:[(pid, age)]}}。

    children 带上子进程启动时长，用来区分“工具在跑”和“常驻 MCP/helper”：
    工具子进程一般和最后一次 toolCall 差不多同时启动；常驻子进程会更老。
    """
    rows = []
    if os.name == "nt":
        try:
            out = subprocess.run(
                ["powershell", "-NoProfile", "-Command",
                 "Get-CimInstance Win32_Process | ForEach-Object { "
                 "\"$($_.ProcessId)`t$($_.ParentProcessId)`t$($_.Name)`t$($_.CommandLine)\" }"],
                capture_output=True, text=True, timeout=12)
            for line in (out.stdout or "").splitlines():
                fields = line.split("\t", 3)
                if len(fields) < 4:
                    continue
                try:
                    pid, ppid = int(fields[0]), int(fields[1])
                except ValueError:
                    continue
                rows.append((pid, ppid, None, (fields[2] + " " + fields[3]).strip()))
        except Exception:
            return {}
    else:
        try:
            out = subprocess.check_output(
                ["ps", "-Ao", "pid=,ppid=,etime=,command="], text=True, timeout=5)
        except Exception:
            return {}
        for line in out.splitlines():
            m = re.match(r"\s*(\d+)\s+(\d+)\s+(\S+)\s+(.*)", line)
            if m:
                rows.append((int(m.group(1)), int(m.group(2)),
                             _etime_secs(m.group(3)), m.group(4).strip()))
    info = {}
    for pid, _ppid, _age, cmd in rows:
        sid = _agent_from_cmd(cmd)
        if sid:
            info.setdefault(sid, {"pids": set(), "cwds": set(), "children": []})["pids"].add(pid)
    for pid, ppid, age, _cmd in rows:
        for val in info.values():
            if ppid in val["pids"]:
                val["children"].append((pid, age))
    for val in info.values():
        for pid in val["pids"]:
            try:
                cwd = os.readlink("/proc/%d/cwd" % pid)
            except OSError:
                continue
            try:
                val["cwds"].add(os.path.realpath(cwd))
            except OSError:
                pass
        val["busy"] = bool(val["children"])  # 兼容旧字段
        # 工具完全在 Agent 进程内执行时没有子进程；用采样 CPU 作为补充信号。
        now_mono = time.monotonic()
        hz = os.sysconf("SC_CLK_TCK") if hasattr(os, "sysconf") else 100
        active = False
        for pid in val["pids"]:
            ticks = _proc_cpu_ticks(pid)
            prev = _AGENT_CPU.get(pid)
            if ticks is not None:
                _AGENT_CPU[pid] = (ticks, now_mono)
            if ticks is not None and prev is not None:
                dt = now_mono - prev[1]
                if dt > 0.4:
                    frac = (ticks - prev[0]) / (dt * max(1, hz))
                    if frac > 0.03:
                        active = True
        val["cpu_active"] = active
    # 清理已经退出的 pid，避免长期运行后字典膨胀
    current = {pid for val in info.values() for pid in val["pids"]}
    for pid in list(_AGENT_CPU):
        if pid not in current:
            _AGENT_CPU.pop(pid, None)
    return info


def _alive_agents():
    """兼容旧调用：返回活着的 Agent id 集合。"""
    return set(_agent_process_info())


def music():
    """当前正在播放的音乐。macOS 走 osascript；Windows 暂不实现（WinRT 媒体会话接口较重），
    直接返回未在播放，组件显示「未在播放」而不是报错。"""
    if os.name == "nt":
        return {"playing": False}
    script = '''
    tell application "System Events"
      set runningApps to name of every process
    end tell
    if runningApps contains "Music" then
      tell application "Music"
        if player state is playing then
          return (name of current track) & "\\t" & (artist of current track) & "\\t" & "Music"
        end if
      end tell
    end if
    return ""
    '''
    try:
        out = subprocess.check_output(["osascript", "-e", script], text=True, timeout=5).strip()
    except Exception:
        return {"playing": False}
    if not out:
        return {"playing": False}
    parts = out.split("\t")
    return {"playing": True, "title": parts[0], "artist": parts[1] if len(parts) > 1 else "",
            "app": parts[2] if len(parts) > 2 else ""}


def _text_of(content):
    """从 message.content 里取纯文本（list/str 两种写法都兼容）"""
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        for part in content:
            if isinstance(part, dict) and part.get("type") in ("text", "input_text") and part.get("text"):
                return str(part["text"]).strip()
            if isinstance(part, str) and part.strip():
                return part.strip()
    return ""


def _session_meta(path):
    """取会话标题 + 工作目录，按 (路径, mtime) 缓存，不重复解析"""
    st = os.stat(path)
    key = (path, st.st_mtime, st.st_size)
    if key in _TITLE_CACHE:
        return _TITLE_CACHE[key]
    title, cwd = "", ""
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for _ in range(400):
                line = f.readline()
                if not line:
                    break
                if len(line) > 200_000:
                    continue
                try:
                    d = json.loads(line)
                except Exception:
                    continue
                if not cwd:
                    cwd = d.get("cwd") or (d.get("payload") or {}).get("cwd") or ""
                if not title:
                    if d.get("type") == "message" and (d.get("message") or {}).get("role") == "user":
                        title = _text_of((d["message"] or {}).get("content"))
                    elif d.get("type") == "user":
                        title = _text_of((d.get("message") or {}).get("content"))
                    elif (d.get("payload") or {}).get("type") in ("user_message", "message"):
                        payload = d["payload"]
                        title = _text_of(payload.get("message") or payload.get("content"))
                if title and cwd:
                    break
    except Exception:
        pass
    # 有些会话的第一条"用户消息"其实是系统注入的上下文/临时路径，换成项目名更好读
    if title.startswith("<") or title.startswith("/") or "/var/folders/" in title:
        title = ""
    if not title:
        title = os.path.basename(cwd) if cwd else os.path.splitext(os.path.basename(path))[0]
    title = " ".join(title.split())[:120]
    _TITLE_CACHE[key] = (title, cwd)
    return title, cwd


ERR_MARKERS = (
    '"isapierrormessage":true',
    '"type":"turn_aborted"',
    '"type":"stream_error"',
    '"type":"error"',
    '"stopreason":"error"',
)


def _session_failed(path, tail_bytes=20000, tail_lines=25):
    """看会话尾部有没有硬错误标记（用户取消/流错误/API 报错）。不解析全文件。"""
    try:
        size = os.path.getsize(path)
        with open(path, "rb") as f:
            f.seek(max(0, size - tail_bytes))
            tail = f.read().decode("utf-8", "replace")
    except OSError:
        return False
    for line in tail.splitlines()[-tail_lines:]:
        low = line.lower()
        if any(m in low for m in ERR_MARKERS):
            return True
    return False


def _session_activity(path):
    """最后一条有效 jsonl 记录里，助手是不是刚结束/是否停在工具调用上。"""
    try:
        size = os.path.getsize(path)
        start = max(0, size - 1_000_000)
        with open(path, "rb") as f:
            f.seek(start)
            tail = f.read().decode("utf-8", "replace")
    except OSError:
        return None
    lines = tail.splitlines()
    if start > 0 and lines:
        lines = lines[1:]  # 第一行通常是被截断的
    for line in reversed(lines):
        line = line.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except Exception:
            continue
        if not isinstance(d, dict):
            continue
        msg = d.get("message") if isinstance(d.get("message"), dict) else {}
        payload = d.get("payload") if isinstance(d.get("payload"), dict) else {}
        role = str(msg.get("role") or d.get("role") or d.get("type") or "").lower()
        stop = str(
            msg.get("stopReason") or msg.get("stop_reason")
            or d.get("stopReason") or d.get("stop_reason")
            or payload.get("stop_reason") or ""
        ).lower()
        content = msg.get("content")
        if content is None:
            content = d.get("content")
        if content is None:
            content = payload.get("content")
        tool_call = False
        if isinstance(content, list):
            for part in content:
                if not isinstance(part, dict):
                    continue
                kind = str(part.get("type") or "").lower()
                if kind in ("toolcall", "tool_call", "tool_use", "function_call", "local_shell_call", "custom_tool_call"):
                    tool_call = True
        return {"role": role, "stop": stop, "tool_call": tool_call}
    return None


def _session_state(path, age, live, children=None, cpu_active=False):
    """live + 最后一条是工具调用且长时间无结果 → 等待确认；
    助手已经 stop（回合结束）就不再算等待，避免空闲进程一直黄闪。"""
    if not live:
        return "error" if _session_failed(path) else ("done" if age < 86400 else "idle")
    act = _session_activity(path)
    if not act:
        return "running" if age < 120 else ("done" if age < 86400 else "idle")
    role, stop, tool_call = act["role"], act["stop"], act["tool_call"]
    if stop in ("error", "aborted", "cancelled", "canceled"):
        return "error"
    pending = tool_call or stop in ("tooluse", "tool_calls", "tool_use", "function_call")
    if role == "assistant":
        if pending:
            if age >= 86400:
                return "idle"
            # 只把「和这次 toolCall 差不多同时启动」的子进程当成工具；
            # 早于它启动的常驻 MCP/helper 不阻止 waiting。
            tool_running = cpu_active or any(
                ca is None or ca <= age + 30
                for _pid, ca in (children or [])
            )
            return "waiting" if age >= 120 and not tool_running else "running"
        return "done" if age < 86400 else "idle"
    # 最后一条是 toolResult / user / system：仍在一轮任务里，给个上限防止
    # 进程挂着但永远显示运行中。
    return "running" if age < 3600 else ("done" if age < 86400 else "idle")


def _parse_ts(value):
    """JSONL 里的时间戳：Unix ms/s 或 ISO 8601 → 秒。"""
    if isinstance(value, (int, float)):
        v = float(value)
        return v / 1000.0 if v > 1_000_000_000_000 else v
    if not isinstance(value, str) or not value.strip():
        return None
    try:
        return datetime.datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()
    except ValueError:
        return None


def _session_run_start(path, tail_bytes=1_000_000):
    """当前一轮开始时间（最后一次 user message）；找不到返回 None。"""
    try:
        size = os.path.getsize(path)
        start = max(0, size - tail_bytes)
        with open(path, "rb") as f:
            f.seek(start)
            tail = f.read().decode("utf-8", "replace")
    except OSError:
        return None
    lines = tail.splitlines()
    if start > 0 and lines:
        lines = lines[1:]
    last_user = None
    last_stop = None
    done_stops = {"stop", "end_turn", "endturn", "complete", "completed", "length", "max_tokens"}
    for line in lines:
        try:
            d = json.loads(line)
        except Exception:
            continue
        if not isinstance(d, dict):
            continue
        msg = d.get("message") if isinstance(d.get("message"), dict) else {}
        payload = d.get("payload") if isinstance(d.get("payload"), dict) else {}
        role = str(msg.get("role") or d.get("role") or d.get("type") or "").lower()
        stop = str(
            msg.get("stopReason") or msg.get("stop_reason")
            or d.get("stopReason") or d.get("stop_reason")
            or payload.get("stop_reason") or ""
        ).lower()
        ts = _parse_ts(msg.get("timestamp") or d.get("timestamp") or payload.get("timestamp"))
        if ts is None:
            continue
        if role == "user":
            last_user = ts if last_user is None else max(last_user, ts)
        elif role == "assistant" and stop in done_stops:
            last_stop = ts if last_stop is None else max(last_stop, ts)
    if last_user is None:
        return None
    if last_stop is not None and last_user <= last_stop:
        return None
    return last_user


def _session_files(roots, limit=4):
    files = []
    for root in roots:
        root = os.path.expanduser(root)
        if not os.path.isdir(root):
            continue
        for dirpath, _dirs, names in os.walk(root):
            for n in names:
                if not n.endswith(".jsonl"):
                    continue
                path = os.path.join(dirpath, n)
                try:
                    st = os.stat(path)
                except OSError:
                    continue
                if st.st_size > 60 * 1024 * 1024:  # 太夸张的会话跳过
                    continue
                files.append((st.st_mtime, path))
    files.sort(reverse=True)
    return files[:limit]


_AGENT_STATUS_MAP = {
    "running": "running",
    "waiting_confirmation": "waiting",
    "done": "done",
    "error": "error",
}


def _pid_alive(pid):
    try:
        pid = int(pid)
    except (TypeError, ValueError):
        return False
    if pid <= 0:
        return False
    if os.name == "nt":
        # Windows 上不做 kill(0) 探测；状态文件的时间/会话仍会兜底。
        return True
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    except OSError:
        return False


def _load_agent_statuses():
    """读 ~/.cs80_panel/agent-status/<agent>/*.json，按 sessionFile/ID 建索引。"""
    by_path = {}
    by_id = {}
    if not os.path.isdir(AGENT_STATUS_DIR):
        return by_path, by_id
    for agent in os.listdir(AGENT_STATUS_DIR):
        adir = os.path.join(AGENT_STATUS_DIR, agent)
        if not os.path.isdir(adir):
            continue
        for name in os.listdir(adir):
            if not name.endswith(".json"):
                continue
            path = os.path.join(adir, name)
            try:
                with open(path, encoding="utf-8") as f:
                    d = json.load(f)
            except Exception:
                continue
            if not isinstance(d, dict):
                continue
            d.setdefault("agent", agent)
            sid = str(d.get("sessionId") or "").strip()
            sf = str(d.get("sessionFile") or "").strip()
            if sid:
                by_id[(agent, sid)] = d
            if sf:
                by_path[(agent, os.path.realpath(sf))] = d
    return by_path, by_id


def _agent_status_for(by_path, by_id, agent, session_path):
    real = os.path.realpath(session_path)
    st = by_path.get((agent, real))
    if st:
        return st
    base = os.path.basename(session_path)
    for (a, sid), candidate in by_id.items():
        if a == agent and sid and sid in base:
            return candidate
    return None


def _status_state(st, now, agent_alive=True):
    """把 sidecar 状态映射到 UI 状态；过期/进程已退出则返回 None。"""
    if not st:
        return None
    state = _AGENT_STATUS_MAP.get(str(st.get("state") or ""))
    if not state:
        return None
    try:
        updated = float(st.get("updatedAt") or 0)
    except (TypeError, ValueError):
        return None
    if not updated:
        return None
    if updated < 1_000_000_000_000:  # 兼容秒级时间戳
        updated *= 1000.0
    age = now - updated / 1000.0
    if state == "waiting":
        ttl = 6 * 3600
    elif state == "running":
        ttl = 30 * 60
    else:
        ttl = 24 * 3600
    if age > ttl:
        return None
    if state in ("running", "waiting") and not agent_alive:
        return None
    pid = st.get("pid")
    if pid is not None and not _pid_alive(pid):
        return None
    return state


def agents():
    """给 Agent 状态组件：按「运行中 → 等待确认 → 已完成」+ 最近活动倒序"""
    now = time.time()
    proc_info = _agent_process_info()
    alive = set(proc_info)
    status_by_path, status_by_id = _load_agent_statuses()
    items = []
    for spec in AGENT_SPECS:
        is_alive = spec["id"] in alive
        proc = proc_info.get(spec["id"], {})
        children = proc.get("children", [])
        proc_cwds = proc.get("cwds", set())
        cpu_active = bool(proc.get("cpu_active"))
        sessions = _session_files(spec["roots"])
        if not sessions:
            if is_alive:  # 没有会话日志的 agent（比如 DeepSeek Harness）
                items.append(
                    {
                        "id": spec["id"] + ":app",
                        "agent": spec["id"],
                        "short": spec["short"],
                        "name": spec["name"],
                        "state": "running",
                        "title": "运行中（会话数据未落盘）",
                        "cwd": "",
                        "age": 0,
                        "updated": now,
                    }
                )
            continue
        for idx, (mtime, path) in enumerate(sessions):
            title, cwd = _session_meta(path)
            age = max(0, now - mtime)
            # 精确映射优先：Agent 进程的 cwd 与 session header 的 cwd 相同，
            # 就算这个 session 不是最新，也可能是并行会话。
            cwd_real = os.path.realpath(cwd) if cwd else ""
            cwd_live = bool(cwd_real and cwd_real in proc_cwds)
            live = is_alive and (cwd_live or idx == 0 or age < 300)
            state = _session_state(path, age, live, children, cpu_active)
            # extension/hook 写出的精确状态优先；过期或进程退出才回退推断。
            sidecar = _agent_status_for(status_by_path, status_by_id, spec["id"], path)
            sidecar_state = _status_state(sidecar, now, is_alive)
            if sidecar_state:
                state = sidecar_state
            if sidecar:
                side_title = str(sidecar.get("title") or "").strip()
                if side_title:
                    # Pi extension 已把标题压缩成 "project · 规则摘要"（或手动 /name）。
                    title = side_title[:80]
            running_since = None
            if state == "running":
                if sidecar:
                    try:
                        running_since = float(sidecar.get("runningSince") or 0)
                    except (TypeError, ValueError):
                        running_since = None
                    if running_since and running_since < 1_000_000_000_000:
                        running_since *= 1000.0
                if not running_since:
                    start = _session_run_start(path)
                    running_since = start * 1000.0 if start else None
            running_for = None
            if state == "running" and running_since:
                running_for = max(0, int(now - running_since / 1000.0))
            if state != "running" and age > 3 * 86400:  # 三天前的旧会话不上屏
                continue
            items.append(
                {
                    "id": hashlib.sha1(path.encode("utf-8")).hexdigest()[:12],
                    "agent": spec["id"],
                    "short": spec["short"],
                    "name": spec["name"],
                    "state": state,
                    "title": title,
                    "cwd": os.path.basename(cwd) if cwd else "",
                    "age": int(age),
                    "runningFor": running_for,
                    "updated": mtime,
                }
            )
    order = {"running": 0, "waiting": 1, "error": 2, "done": 3, "idle": 4}
    items.sort(key=lambda it: (order.get(it["state"], 9), it["age"]))
    items = items[:12]  # 屏幕只看最靠前的几条
    running = sum(1 for it in items if it["state"] == "running")
    return {"items": items, "running": running, "alive": sorted(alive)}


_NET_PREV = {"at": 0.0, "rx": 0, "tx": 0}
_NET_WIN = {"at": 0.0, "val": None}
_WIN_CPU = {"ticks": None, "pct": 0.0}


def _net_counters():
    """累计收/发字节数；取不到返回 None。"""
    try:
        if os.name == "nt":
            # 只看默认路由那个网卡（Measure-Object 求和会被虚拟网卡异常计数带飞）
            now = time.time()
            if now - _NET_WIN["at"] < 1.0 and _NET_WIN["val"] is not None:
                return _NET_WIN["val"]
            ps = ("$i=(Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric | "
                  "Select-Object -First 1).ifIndex; if(!$i){exit}; "
                  "$n=(Get-NetAdapter -InterfaceIndex $i).Name; "
                  "$s=Get-NetAdapterStatistics -Name $n; \"$($s.ReceivedBytes) $($s.SentBytes)\"")
            out = subprocess.run(["powershell", "-NoProfile", "-Command", ps],
                                 capture_output=True, text=True, timeout=8)
            parts = (out.stdout or "").split()
            val = (int(parts[0]), int(parts[1])) if len(parts) >= 2 else None
            _NET_WIN.update(at=now, val=val)
            return val
        if os.path.exists("/proc/net/dev"):
            rx = tx = 0
            with open("/proc/net/dev") as f:
                for line in f.readlines()[2:]:
                    name, _, rest = line.partition(":")
                    if name.strip() == "lo":
                        continue
                    cols = rest.split()
                    rx += int(cols[0])
                    tx += int(cols[8])
            return rx, tx
        # macOS：<Link#N> 那行的 Ibytes/Obytes；Address 列可能为空，所以按标记列定位
        out = subprocess.check_output(["netstat", "-ib"], text=True, timeout=5)
        rx = tx = 0
        for line in out.splitlines()[1:]:
            cols = line.split()
            i = next((k for k, v in enumerate(cols) if v.startswith("<Link#")), None)
            if i is None or cols[0].startswith("lo"):
                continue
            if ":" in cols[i + 1]:
                rx += int(cols[i + 4])
                tx += int(cols[i + 7])
            else:
                rx += int(cols[i + 3])
                tx += int(cols[i + 6])
        return rx, tx
    except Exception:
        return None


def net_rates():
    """(上传, 下载) 字节/秒，按上一次采样差分。"""
    now = time.time()
    cur = _net_counters()
    prev = dict(_NET_PREV)
    _NET_PREV.update(at=now, rx=(cur[0] if cur else 0), tx=(cur[1] if cur else 0))
    dt = now - prev["at"]
    if cur is None or dt <= 0.2 or cur[0] < prev["rx"] or cur[1] < prev["tx"]:
        return 0, 0
    return round((cur[1] - prev["tx"]) / dt), round((cur[0] - prev["rx"]) / dt)



# macOS 真实 CPU 占用率（host_statistics64 / HOST_CPU_LOAD_INFO）。
# 不用 loadavg/ncpu —— 那是负载均值，跟活动监视器/RunCat 的口径不一样，会明显偏高。
_CPU_SAMPLE = {"ticks": None, "at": 0.0, "pct": 0.0}


def _mac_cpu_percent():
    import ctypes
    import ctypes.util

    global _CPU_SAMPLE
    now = time.time()
    if now - _CPU_SAMPLE["at"] < 0.5 and _CPU_SAMPLE["ticks"]:
        return _CPU_SAMPLE["pct"]

    class CpuLoad(ctypes.Structure):
        _fields_ = [("cpu_ticks", ctypes.c_uint32 * 4)]  # user, system, idle, nice

    try:
        lib = ctypes.CDLL(ctypes.util.find_library("System"))
        lib.mach_host_self.restype = ctypes.c_uint
        host = lib.mach_host_self()
        info = CpuLoad()
        count = ctypes.c_uint32(4)
        if lib.host_statistics64(host, 3, ctypes.byref(info), ctypes.byref(count)) != 0:
            return _CPU_SAMPLE["pct"]
        ticks = list(info.cpu_ticks)
    except Exception:
        return _CPU_SAMPLE["pct"]

    prev = _CPU_SAMPLE["ticks"]
    _CPU_SAMPLE["ticks"] = ticks
    _CPU_SAMPLE["at"] = now
    if prev:
        delta = [b - a for a, b in zip(prev, ticks)]
        total = sum(delta)
        if total > 0:
            _CPU_SAMPLE["pct"] = max(0.0, min(1.0, 1.0 - delta[2] / total))
    return _CPU_SAMPLE["pct"]



_LINUX_CPU = {"ticks": None}


def _linux_cpu_percent():
    """Linux：/proc/stat 的 jiffies 差分（idle+iowait 之外都算忙）。
    和 top/htop 口径一致；loadavg/ncpu 是负载均值，不是占用率（见 docs/dead-ends.md）。
    第一次调用没有基准，返回 None，由调用方退回 loadavg。"""
    try:
        with open("/proc/stat") as f:
            parts = f.readline().split()
        vals = [int(x) for x in parts[1:]]
        if len(vals) < 4:
            return None
        idle = vals[3] + (vals[4] if len(vals) > 4 else 0)  # idle + iowait
        total = sum(vals)
    except Exception:
        return None
    prev = _LINUX_CPU["ticks"]
    _LINUX_CPU["ticks"] = (total, idle)
    if not prev or total <= prev[0]:
        return None
    dt = total - prev[0]
    di = idle - prev[1]
    if dt <= 0:
        return None
    return max(0.0, min(1.0, 1.0 - di / dt))


def _windows_metrics():
    """Windows 的 CPU/内存（GetSystemTimes + GlobalMemoryStatusEx，不装第三方包）。"""
    import ctypes
    from ctypes import wintypes

    class FILETIME(ctypes.Structure):
        _fields_ = [("lo", wintypes.DWORD), ("hi", wintypes.DWORD)]

    class MEMORYSTATUSEX(ctypes.Structure):
        _fields_ = [("dwLength", wintypes.DWORD), ("dwMemoryLoad", wintypes.DWORD),
                    ("ullTotalPhys", ctypes.c_ulonglong), ("ullAvailPhys", ctypes.c_ulonglong),
                    ("ullTotalPageFile", ctypes.c_ulonglong), ("ullAvailPageFile", ctypes.c_ulonglong),
                    ("ullTotalVirtual", ctypes.c_ulonglong), ("ullAvailVirtual", ctypes.c_ulonglong),
                    ("ullAvailExtendedVirtual", ctypes.c_ulonglong)]

    k32 = ctypes.windll.kernel32
    idle, kern, user = FILETIME(), FILETIME(), FILETIME()
    now = time.time()
    if not k32.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kern), ctypes.byref(user)):
        raise OSError("GetSystemTimes failed")
    ticks = [(x.hi << 32) | x.lo for x in (idle, kern, user)]
    prev = _WIN_CPU["ticks"]
    _WIN_CPU["ticks"] = ticks
    if prev:
        d_idle, d_kern, d_user = (a - b for a, b in zip(ticks, prev))
        total = d_kern + d_user  # kernel 已含 idle
        if total > 0:
            _WIN_CPU["pct"] = max(0.0, min(1.0, 1.0 - d_idle / total))

    ms = MEMORYSTATUSEX()
    ms.dwLength = ctypes.sizeof(ms)
    k32.GlobalMemoryStatusEx(ctypes.byref(ms))
    total_gb = ms.ullTotalPhys / 2**30
    used_gb = (ms.ullTotalPhys - ms.ullAvailPhys) / 2**30
    return {
        "cpu": round(_WIN_CPU["pct"], 3),
        "mem": round(used_gb, 2),
        "memTotal": round(total_gb, 1),
        "host": os.environ.get("COMPUTERNAME", ""),
        "load": 0,
    }


_IP_CACHE = {"at": 0.0, "ip": ""}


_TUN_HINTS = ("tunnel", "tap", "tun", "vpn", "meta", "wireguard", "zerotier", "tailscale", "clash")


def _rank_ip(ip, has_gw, desc):
    """越小越像“真正的局域网 IP”；None = 不要。"""
    if not ip or ip.startswith("127.") or ip.startswith("169.254.") or ip == "0.0.0.0":
        return None
    if ip.startswith("198.18.") or ip.startswith("198.19."):
        return None  # 代理 TUN / 基准测试保留段（Clash、FlClash 的 Meta Tunnel 就用这个）
    parts = ip.split(".")
    private = (ip.startswith("10.") or ip.startswith("192.168.")
               or (len(parts) == 4 and parts[0] == "172" and parts[1].isdigit() and 16 <= int(parts[1]) <= 31))
    if any(h in (desc or "").lower() for h in _TUN_HINTS):
        return 4
    if private and has_gw:
        return 0
    if private:
        return 1
    if has_gw:
        return 2
    return 3


def _lan_ip_win():
    """Get-NetIPConfiguration → [(alias, ip, 有无默认网关, 网卡描述)]"""
    ps = ("Get-NetIPConfiguration | ForEach-Object { "
          "\"$($_.InterfaceAlias)`t$($_.IPv4Address.IPAddress -join ',')\t"
          "$($_.IPv4DefaultGateway.NextHop)`t$($_.InterfaceDescription)\" }")
    out = subprocess.run(["powershell", "-NoProfile", "-Command", ps],
                         capture_output=True, text=True, timeout=10)
    rows = []
    for line in (out.stdout or "").splitlines():
        cols = line.split("\t")
        if len(cols) < 4:
            continue
        alias, ips, gw, desc = cols[0].strip(), cols[1].strip(), cols[2].strip(), cols[3].strip()
        for ip in ips.split(","):
            ip = ip.strip()
            if ip:
                rows.append((alias, ip, bool(gw), desc))
    return rows


def _lan_ip_posix():
    """ifconfig + 默认路由 → [(iface, ip, 有无默认网关, iface)]"""
    try:
        conf = subprocess.run(["ifconfig"], capture_output=True, text=True, timeout=6).stdout or ""
    except Exception:
        return []
    default_if = ""
    try:
        rn = subprocess.run(["netstat", "-rn"], capture_output=True, text=True, timeout=6).stdout or ""
        for line in rn.splitlines():
            cols = line.split()
            if len(cols) >= 4 and cols[0] == "default":
                default_if = cols[-1]
                break
    except Exception:
        pass
    rows, cur = [], ""
    for line in conf.splitlines():
        if line and not line.startswith(("\t", " ")):
            cur = line.split(":")[0]
        m = re.search(r"inet (\d+\.\d+\.\d+\.\d+)", line)
        if m and cur:
            rows.append((cur, m.group(1), cur == default_if, cur))
    return rows


def lan_ip():
    """本机**局域网** IPv4（拿不到就返回空字符串，绝不抛异常）。

    不能简单地用“默认路由那张网卡”：开了代理（Clash/FlClash 的 Meta Tunnel）时默认路由是指向
    TUN 的，会拿到 198.18.0.1；VMware 的 VMnet 又是 192.168.x.x。所以枚举所有 IPv4 再打分：

    1. 优先「私网段 + 有默认网关」的物理网卡（WLAN / 以太网）
    2. 其次私网但没网关的；再次有网关的公网地址
    3. 直接丢弃：回环、169.254 链路本地、198.18/19（TUN）、以及描述里带 tunnel/vpn/meta/… 的网卡

    10 秒缓存，换 WiFi / 插网线能自动跟上。
    """
    now = time.time()
    if _IP_CACHE["ip"] and now - _IP_CACHE["at"] < 10:
        return _IP_CACHE["ip"]
    rows = []
    try:
        rows = _lan_ip_win() if os.name == "nt" else _lan_ip_posix()
    except Exception:
        rows = []
    ranked = []
    for _alias, ip, has_gw, desc in rows:
        r = _rank_ip(ip, has_gw, desc)
        if r is not None:
            ranked.append((r, ip))
    ip = ""
    if ranked:
        ip = sorted(ranked)[0][1]
    if not ip:  # 全都不合适（例如机器上只有代理 TUN）→ 回退到“默认路由地址”
        try:
            import socket
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            try:
                s.connect(("8.8.8.8", 80))
                cand = s.getsockname()[0]
            finally:
                s.close()
            if _rank_ip(cand, True, "") is not None:
                ip = cand
        except Exception:
            ip = ""
    _IP_CACHE.update(at=now, ip=ip)
    return ip


def system_metrics():
    if os.name == "nt":
        try:
            m = _windows_metrics()
        except Exception:
            m = {"cpu": 0, "mem": 0, "memTotal": 0, "host": os.environ.get("COMPUTERNAME", ""), "load": 0}
        m["netUp"], m["netDown"] = net_rates()
        m["ip"] = lan_ip()
        return m
    if sys.platform == "darwin":
        cpu = _mac_cpu_percent()
    else:
        cpu = _linux_cpu_percent()
        if cpu is None:  # 第一次采样/取不到 /proc/stat 时退回 loadavg
            try:
                cpu = min(1.0, os.getloadavg()[0] / max(1, os.cpu_count() or 1))
            except Exception:
                cpu = 0.0
    mem, total = 0.0, 0.0
    try:
        if os.path.exists("/usr/bin/vm_stat"):
            # macOS
            out = subprocess.check_output(["vm_stat"], text=True)
            m = re.search(r"page size of (\d+) bytes", out)
            page = int(m.group(1)) if m else 4096
            used = 0
            for line in out.splitlines():
                key, _, val = line.partition(":")
                if key.strip() in ("Pages active", "Pages wired down", "Pages occupied by compressor"):
                    used += int(val.strip().rstrip(".") or 0)
            total = int(subprocess.check_output(["sysctl", "-n", "hw.memsize"]).strip()) / 2**30
            mem = used * page / 2**30
        elif os.path.exists("/proc/meminfo"):
            # Linux：used = MemTotal - MemAvailable，和 free/htop 的 used 一致
            info = {}
            with open("/proc/meminfo") as f:
                for line in f:
                    k, _, v = line.partition(":")
                    try:
                        info[k.strip()] = int(v.strip().split()[0])
                    except (ValueError, IndexError):
                        pass
            total = info.get("MemTotal", 0) / 1048576.0
            avail = info.get("MemAvailable")
            if avail is None:
                avail = info.get("MemFree", 0) + info.get("Buffers", 0) + info.get("Cached", 0)
            mem = (info.get("MemTotal", 0) - avail) / 1048576.0
    except Exception:
        pass
    up, down = net_rates()
    return {
        "cpu": round(cpu, 3),
        "mem": round(mem, 2),
        "memTotal": round(total, 1),
        "netUp": up,
        "netDown": down,
        "ip": lan_ip(),
        "host": os.uname().nodename if hasattr(os, "uname") else os.environ.get("COMPUTERNAME", ""),
        "load": round(os.getloadavg()[0], 2) if hasattr(os, "getloadavg") else 0,
    }


def agent_state():
    try:
        with open(STATE_FILE, encoding="utf-8") as f:
            st = json.load(f)
    except Exception:
        return {"state": "idle", "task": "", "since": time.time()}
    st.setdefault("state", "idle")
    st.setdefault("task", "")
    st.setdefault("since", time.time())
    return st


# ---------------- Home Assistant ----------------
# HA 只在家里局域网/本机跑：token 放在 ~/.cs80_panel/config.json 的 ha.token，
# 或环境变量 HA_URL / HA_TOKEN。浏览器只拿到白名单实体的 state，拿不到 token。
HA_ATTRS = (
    "friendly_name", "unit_of_measurement", "device_class", "state_class",
    "hvac_action", "current_temperature", "temperature", "fan_mode",
    "brightness", "media_title", "media_artist", "volume_level", "is_volume_muted",
    "battery_level", "entity_id", "humidity", "wind_speed", "cloud_coverage",
)
HA_DOMAINS = {
    "sensor", "binary_sensor", "light", "switch", "climate", "lock", "media_player",
    "weather", "fan", "vacuum", "cover", "alarm_control_panel", "person", "input_boolean",
}


def ha_states(ids):
    """按 entity_id 读 HA 状态；只返回组件需要的字段，token 不出 host。"""
    cfg = load_config().get("ha") or {}
    url = (os.environ.get("HA_URL") or cfg.get("url") or "").strip().rstrip("/")
    token = (os.environ.get("HA_TOKEN") or cfg.get("token") or "").strip()
    if not url or not token:
        raise RuntimeError("HA 未配置：~/.cs80_panel/config.json 的 ha.url / ha.token")

    wanted = []
    for eid in ids:
        eid = eid.strip()
        if not re.match(r"^[a-z_]+\.[a-z0-9_]+$", eid):
            continue
        if eid.split(".", 1)[0] not in HA_DOMAINS:
            continue
        if eid not in wanted:
            wanted.append(eid)
    if not wanted:
        raise RuntimeError("HA 实体为空")

    headers = {"Authorization": "Bearer " + token, "User-Agent": "cs80-panel/1.0"}

    def one(eid):
        try:
            return eid, http_json(url + "/api/states/" + urllib.parse.quote(eid), headers=headers, timeout=4), None
        except Exception as exc:
            return eid, None, str(exc)

    # 并发拉：HA 某一台设备/一条实体超时不能把整个组件拖住（顺序拉 8 条最坏 32 s）。
    with ThreadPoolExecutor(max_workers=min(8, len(wanted))) as pool:
        results = list(pool.map(one, wanted[:32]))

    items = {}
    errors = []
    for eid, st, err in results:
        if st is None:
            items[eid] = {"id": eid, "state": "unavailable", "domain": eid.split(".", 1)[0],
                          "error": (err or "timeout")[:120]}
            errors.append(err or "timeout")
            continue
        attrs = st.get("attributes") or {}
        items[eid] = {
            "id": eid,
            "name": attrs.get("friendly_name") or eid,
            "state": st.get("state"),
            "unit": attrs.get("unit_of_measurement") or "",
            "domain": eid.split(".", 1)[0],
            "device_class": attrs.get("device_class") or "",
            "attributes": {k: attrs[k] for k in HA_ATTRS if k in attrs},
            "changed": st.get("last_changed"),
        }
    if not any(v.get("state") != "unavailable" for v in items.values()):
        raise RuntimeError(errors[0] if errors else "HA 无响应")
    return {"online": True, "items": items, "updated": time.time()}


# ---------------- HTTP ----------------
def load_layout():
    try:
        with open(LAYOUT, encoding="utf-8") as f:
            return migrate_layout(json.load(f))
    except Exception:
        return migrate_layout(None)


def save_layout(layout):
    os.makedirs(os.path.dirname(LAYOUT), exist_ok=True)
    tmp = LAYOUT + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(layout, f, ensure_ascii=False, indent=1)
    os.replace(tmp, LAYOUT)


# ---- 纯色测试图案（判断「屏闪」到底是背光/面板还是我们推的帧）----
PATTERN_COLORS = {
    "white": (255, 255, 255),
    "black": (0, 0, 0),
    "gray": (128, 128, 128),
    "dark": (24, 26, 32),
    "red": (255, 0, 0),
    "green": (0, 255, 0),
    "blue": (0, 0, 255),
}


def parse_color(spec):
    s = str(spec).strip().lower()
    if s in PATTERN_COLORS:
        return PATTERN_COLORS[s]
    if len(s) == 7 and s[0] == "#":
        return tuple(int(s[i:i + 2], 16) for i in (1, 3, 5))
    raise ValueError(spec)


def solid_bgr565(rgb):
    """整屏单色 BGR565 小端整帧（2 MB）：sidecar 的 0x04 消息，不用再过一遍 RGB 转换。"""
    r, g, b = rgb
    value = ((b & 0xF8) << 8) | ((g & 0xFC) << 3) | (r >> 3)
    return value.to_bytes(2, "little") * (800 * 1280)


def usb_link_info():
    """Linux：读 sysfs 拿 QDC1 设备的 USB 速率（480=高速，12=全速）。
    整帧 2 MB 在 12 Mbps 上要 ~1.5 s，是「推整帧时屏上冻结两秒」的直接原因。"""
    now = time.time()
    if now - _USB_INFO["at"] < 10 and _USB_INFO["value"] is not None:
        return _USB_INFO["value"]
    out = None
    if sys.platform.startswith("linux"):
        try:
            for name in os.listdir("/sys/bus/usb/devices"):
                base = os.path.join("/sys/bus/usb/devices", name)

                def rd(key):
                    try:
                        with open(os.path.join(base, key)) as f:
                            return f.read().strip()
                    except OSError:
                        return ""

                if rd("idVendor") == "2c7c" and rd("idProduct") == "0901":
                    try:
                        speed = float(rd("speed"))
                    except ValueError:
                        speed = 0.0
                    out = {"speed": speed, "path": name}
                    break
        except OSError:
            pass
    _USB_INFO.update(at=now, value=out)
    return out


class Handler(BaseHTTPRequestHandler):
    device = None
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        pass

    def _send(self, code, body, ctype="application/json; charset=utf-8"):
        if isinstance(body, (dict, list)):
            body = json.dumps(body, ensure_ascii=False).encode()
        elif isinstance(body, str):
            body = body.encode()
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        url = urllib.parse.urlparse(self.path)
        path, qs = url.path, urllib.parse.parse_qs(url.query)
        if path == "/api/layout":
            return self._send(200, load_layout())
        if path == "/api/status":
            return self._send(200, dict(self.device.status, session=self.device.session, layout=load_layout(),
                                       sidecar=self.device.last, build=web_build(),
                                       screen=dict(SCREEN_STATE), usb=usb_link_info(),
                                       beat_age=(round(time.time() - BEAT["at"], 1) if BEAT["at"] else None),
                                       logs={"sidecar": SIDECAR_LOG, "web": WEB_LOG}))
        if path == "/api/health":
            now = time.time()
            return self._send(200, {
                "beat_age": round(now - BEAT["at"], 1) if BEAT["at"] else None,
                "push_age": round(now - PUSH["at"], 1) if PUSH["at"] else None,
                "device": self.device.status.get("device"),
            })
        if path == "/api/data/weather":
            city = (qs.get("city") or ["北京"])[0]
            return self._send(200, CACHE.get("wx:" + city, 900, lambda: weather(city)))
        if path == "/api/data/stock":
            sym = (qs.get("symbol") or ["sh000001"])[0]
            return self._send(200, CACHE.get("st:" + sym, 15, lambda: stock(sym)))
        if path == "/api/data/kline":
            sym = (qs.get("symbol") or ["sh000001"])[0]
            return self._send(200, CACHE.get("kl:" + sym, 120, lambda: kline(sym)))
        if path == "/api/data/minute":
            sym = (qs.get("symbol") or ["sh000001"])[0]
            return self._send(200, CACHE.get("mi:" + sym, 30, lambda: minute(sym)))
        if path == "/api/data/portfolio":
            syms = (qs.get("symbols") or ["sh000001,sz399001"])[0]
            return self._send(200, CACHE.get("pf:" + syms, 10, lambda: portfolio(syms)))
        if path == "/api/data/crypto":
            ids = (qs.get("ids") or ["bitcoin,ethereum"])[0]
            return self._send(200, CACHE.get("cr:" + ids, 60, lambda: crypto(ids)))
        if path == "/api/data/github":
            repo = (qs.get("repo") or ["tauri-apps/tauri"])[0]
            return self._send(200, CACHE.get("gh:" + repo, 300, lambda: github(repo)))
        if path == "/api/data/news":
            return self._send(200, news((qs.get("sources") or ["toutiao,baidu,zhihu,weibo"])[0]))
        if path == "/api/data/holiday":
            return self._send(200, CACHE.get("holiday", 3600, holidays))
        if path == "/api/data/music":
            return self._send(200, CACHE.get("mu", 3, music))
        if path == "/api/screensaver":
            return self._send(200, dict(load_saver_cfg(), files=screensaver_files(), screen=dict(SCREEN_STATE),
                                        saver=SAVER.status() if SAVER else None))
        if path == "/api/data/agents":
            return self._send(200, CACHE.get("agents", 1, agents))
        if path == "/api/data/system":
            # 0.9s 缓存：多个组件共用一次采样，网速差分才有固定 dt
            return self._send(200, CACHE.get("sys", 0.9, system_metrics))
        if path == "/api/data/ha":
            ids = [x.strip() for x in (qs.get("entities") or [""])[0].split(",") if x.strip()]
            if not ids:
                return self._send(200, {"online": False, "error": "未配置 HA 实体", "items": {}})
            key = "ha:" + ",".join(sorted(set(ids)))
            return self._send(200, CACHE.get(key, 2.5, lambda: ha_states(ids)))
        if path == "/api/data/agent":
            return self._send(200, agent_state())
        # 静态文件
        rel = "index.html" if path in ("/", "") else path.lstrip("/")
        full = os.path.normpath(os.path.join(WEB, rel))
        if not full.startswith(WEB) or not os.path.isfile(full):
            return self._send(404, {"error": "not found"})
        ctype = {
            ".html": "text/html; charset=utf-8",
            ".js": "text/javascript; charset=utf-8",
            ".css": "text/css; charset=utf-8",
            ".svg": "image/svg+xml",
            ".mp4": "video/mp4",
            ".webm": "video/webm",
            ".mov": "video/quicktime",
            ".png": "image/png",
            ".json": "application/json; charset=utf-8",
        }.get(os.path.splitext(full)[1], "application/octet-stream")
        # 视频要支持 Range，否则 <video> 播不了
        size = os.path.getsize(full)
        ranged = False
        rng = self.headers.get("Range")
        start, end = 0, size - 1
        if rng and rng.startswith("bytes="):
            try:
                part = rng[6:].split(",")[0]
                a, _, b = part.partition("-")
                start = int(a) if a else max(0, size - int(b))
                end = min(size - 1, int(b)) if b else size - 1
                ranged = True
            except Exception:
                ranged = False
        with open(full, "rb") as f:
            f.seek(start)
            body = f.read(end - start + 1)
        ext = os.path.splitext(full)[1]
        if ext in (".html", ".js"):
            text = body.decode("utf-8", "replace")
            v = web_build()
            if ext == ".html":
                text = re.sub(r'(src="[^"?]+\.js)(")', r'\1?v=' + v + r'\2', text)
                text = re.sub(r'(href="[^"?]+\.css)(")', r'\1?v=' + v + r'\2', text)
                if sys.platform.startswith("linux"):
                    # 无桌面主机：默认页面只做编辑器，推帧交给后台 kiosk（?kiosk=1）
                    text = text.replace("</head>", "<script>window.__CS80_HEADLESS__=true;</script></head>", 1)
            else:
                text = re.sub(r"(from\s*['\"]\./[^'\"]+?)(['\"])", r"\1?v=" + v + r"\2", text)
            body = text.encode()
        if ranged:
            self.send_response(206)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Content-Range", "bytes %d-%d/%d" % (start, end, size))
            self.send_header("Accept-Ranges", "bytes")
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(body)
        else:
            self._send(200, body, ctype)

    def do_PUT(self):
        url = urllib.parse.urlparse(self.path)
        n = int(self.headers.get("Content-Length") or 0)
        if url.path == "/api/layout":
            try:
                layout = json.loads(self.rfile.read(n).decode())
            except Exception as exc:
                return self._send(400, {"error": str(exc)})
            if layout.get("reset"):
                layout = json.loads(json.dumps(DEFAULT_LAYOUT))
            save_layout(migrate_layout(layout))
            return self._send(200, {"ok": True})
        return self._send(404, {"error": "not found"})

    def do_POST(self):
        url = urllib.parse.urlparse(self.path)
        n = int(self.headers.get("Content-Length") or 0)
        if url.path == "/api/screensaver":
            body = json.loads(self.rfile.read(n) or b"{}")
            cfg = load_saver_cfg()
            if "file" in body:
                cfg["file"] = body["file"]
            if "autoLock" in body:
                cfg["autoLock"] = bool(body["autoLock"])
            action = body.get("action")
            if action == "play" and SAVER:
                ok = SAVER.start(cfg["file"])
                return self._send(200 if ok else 500, dict(cfg, ok=ok, error=SAVER.error))
            if action == "stop" and SAVER:
                SAVER.stop()
                return self._send(200, dict(cfg, ok=True))
            save_saver_cfg(cfg)
            return self._send(200, dict(cfg, ok=True))
        if url.path == "/api/heartbeat":
            self.rfile.read(n)  # keep-alive：body 必须读掉
            BEAT["at"] = time.time()
            return self._send(200, {"ok": True})
        if url.path == "/api/frame":
            body = self.rfile.read(n)
            if len(body) != n or len(body) != 800 * 1280 * 3:
                return self._send(400, {"error": "frame must be %d bytes, got %d" % (800 * 1280 * 3, len(body))})
            self.device.flush_pending()  # 整帧覆盖全屏，旧 patch 没有意义
            ok = self.device.send(b"\x01" + body)
            PUSH["at"] = time.time()
            if ok:
                PUSH["n"] += 1
            return self._send(200, {"ok": ok, "queued": ok})
        if url.path == "/api/patch":
            body = self.rfile.read(n)
            # 浏览器 reload / fetch 超时会在上传中途断开，rfile.read(n) 只拿到半截 body。
            # 把半截 patch 转给 sidecar 会永久错位它的 stdin 消息流（下一帧读到 0xe3 之类
            # 的像素字节当消息类型 → exit → 反复重启）。这里只放行结构完整的 patch。
            bad = len(body) != n or len(body) < 4
            if not bad:
                cnt = struct.unpack_from("<I", body, 0)[0]
                if cnt == 0 or cnt > 495 or 4 + cnt * 1028 != len(body):
                    bad = True
            if bad:
                return self._send(400, {"error": "bad patch body"})
            ok = self.device.send(b"\x02" + body)
            PUSH["at"] = time.time()
            if ok:
                PUSH["n"] += 1
            return self._send(200, {"ok": ok, "queued": ok})
        if url.path == "/api/device/pattern":
            # 把整屏刷成一种纯色：闪不闪、闪哪种颜色，就能把固件重绘 / 背光 PWM / 面板分开
            body = json.loads(self.rfile.read(n) or b"{}")
            spec = body.get("color") or "gray"
            try:
                rgb = parse_color(spec)
            except ValueError:
                return self._send(400, {"error": "color 需要 #RRGGBB 或 %s" % "/".join(PATTERN_COLORS)})
            ok = self.device.send(b"\x04" + solid_bgr565(rgb))
            return self._send(200, {"ok": ok, "color": spec})
        if url.path == "/api/pages/active":
            body = json.loads(self.rfile.read(n) or b"{}")
            layout = load_layout()
            if any(p["id"] == body.get("id") for p in layout["pages"]):
                layout["activePage"] = body["id"]
                save_layout(layout)
                return self._send(200, {"ok": True, "activePage": layout["activePage"]})
            return self._send(400, {"error": "no such page"})
        if url.path == "/api/device/reconnect":
            self.device.reconnect()
            return self._send(200, {"ok": True})
        if url.path == "/api/debug":
            text = self.rfile.read(n).decode("utf-8", "replace")
            os.makedirs(CONF_DIR, exist_ok=True)
            with open(WEB_LOG, "a", encoding="utf-8") as f:
                f.write(time.strftime("%H:%M:%S ") + text + "\n")
            return self._send(200, {"ok": True})
        if url.path == "/api/audio/file":
            # 浏览器 POST 原始文件字节；?name=foo.wav 给扩展名，?play=1 传完就播。
            # 非 MP3 先转码（固件只认 MP3）。
            data = self.rfile.read(n)
            if len(data) != n:
                return self._send(400, {"error": "truncated upload"})
            if not data:
                return self._send(400, {"error": "empty body"})
            if len(data) > AUDIO_IN_MAX:
                return self._send(413, {"error": "文件太大（上限 64 MiB）"})
            name = (urllib.parse.parse_qs(url.query).get("name") or [""])[0]
            try:
                data, info = audio_to_mp3(data, name)
            except Exception as exc:
                return self._send(400, {"error": str(exc)})
            if len(data) > AUDIO_MAX:
                return self._send(413, {"error": "转换后 %.1f MiB，超过固件 4 MiB 上限"
                                        % (len(data) / 1048576.0), "convert": info})
            r = dict(self.device.upload_mp3(data), convert=info)
            if r.get("ok") and "play" in urllib.parse.parse_qs(url.query):
                p = self.device.play_uploaded()
                return self._send(200 if p.get("ok") else 500, {"ok": bool(p.get("ok")), "upload": r, "play": p})
            return self._send(200 if r.get("ok") else 500, {"ok": bool(r.get("ok")), "upload": r})
        if url.path == "/api/audio":
            body = json.loads(self.rfile.read(n) or b"{}")
            action = str(body.get("action") or "query")
            if action == "query":
                ev = self.device.audio(AUDIO_QUERY)
            elif action == "stop":
                ev = self.device.audio(AUDIO_STOP)
            elif action == "volume":
                v = max(0, min(100, int(body.get("value") or 0)))
                ev = self.device.audio(AUDIO_VOLUME, offset=v)
            elif action == "play":
                ev = self.device.play_file(body["file"]) if body.get("file") else self.device.play_uploaded()
            elif action == "verify":
                ev = self.device.audio(AUDIO_VERIFY)
            elif action == "diag":
                # 固件诊断：路由/档位/PA/增益（sub 8，只读）
                ev = self.device.audio(8)
            elif action == "route":
                # 停止后改音频路由：0=speaker（正常），1/2 诊断用
                ev = self.device.audio(9, offset=max(0, min(2, int(body.get("value") or 0))))
            elif action == "upload":
                if not body.get("file"):
                    return self._send(400, {"error": "upload 需要 file 路径；浏览器请用 POST /api/audio/file"})
                try:
                    with open(body["file"], "rb") as f:
                        data = f.read()
                except OSError as exc:
                    return self._send(400, {"error": str(exc)})
                ev = self.device.upload_mp3(data)
            elif action == "say":
                try:
                    data = tts_mp3(str(body.get("text") or ""))
                except Exception as exc:
                    return self._send(400, {"error": str(exc)})
                r = self.device.upload_mp3(data)
                ev = dict(self.device.audio(AUDIO_PLAY, timeout=20), upload=r) if r.get("ok") else r
            else:
                return self._send(400, {"error": "unknown action"})
            return self._send(200 if ev.get("ok") else 500, {"ok": bool(ev.get("ok")), "audio": ev})
        if url.path == "/api/control":
            body = json.loads(self.rfile.read(n) or b"{}")
            cmd = {"brightness": 1, "sleep": 2, "wake": 3, "state": 4, "hello": 5}.get(body.get("cmd"))
            if cmd is None:
                return self._send(400, {"error": "unknown cmd"})
            arg = int(body.get("value") or 0)
            if cmd == 1 and (arg < 5 or arg > 100 or arg % 5):
                return self._send(400, {"error": "brightness 需要 5..100 且为 5 的倍数"})
            if cmd == 1:
                self.device.user_brightness = arg
                if hasattr(self.device, "_set_bright"):
                    self.device._set_bright(arg)   # 记住偏好 + 下发（不再几十秒后又刷回 100）
                else:
                    self.device.control(cmd, arg)
            else:
                self.device.control(cmd, arg)
            time.sleep(0.15)
            return self._send(200, {"ok": True, "status": self.device.status})
        return self._send(404, {"error": "not found"})


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default=os.environ.get("CS80_HOST", "127.0.0.1"),
                    help="监听地址；局域网访问用 0.0.0.0（无鉴权，注意网络环境）")
    ap.add_argument("--port", type=int, default=8787)
    ap.add_argument("--no-device", action="store_true", help="只跑编辑器，不连屏（设计布局用）")
    ap.add_argument("--com", help="Windows：指定设备数据串口，如 COM15")
    args = ap.parse_args()
    if args.com:
        os.environ["CS80_COM"] = args.com

    if sys.platform == "darwin":  # 预热 CPU 采样，避免第一次请求报 0%
        _mac_cpu_percent()
        time.sleep(0.12)
        _mac_cpu_percent()
    elif os.name == "nt":
        _windows_metrics()
        time.sleep(0.12)
        _windows_metrics()
    else:  # Linux：/proc/stat 差分也需要两次采样
        _linux_cpu_percent()
        time.sleep(0.12)
        _linux_cpu_percent()
    threading.Thread(target=_screen_watch, daemon=True).start()
    device = (
        Device()
        if not args.no_device
        else type("D", (), {"status": {"device": "none"}, "last": None, "session": 0,
                            "user_brightness": None, "status_note": "no-device",
                            "send": lambda *a: False, "control": lambda *a: False,
                            "audio": lambda *a, **k: {"ok": False, "err": "no-device"},
                            "upload_mp3": lambda *a, **k: {"ok": False, "err": "no-device"},
                            "play_file": lambda *a, **k: {"ok": False, "err": "no-device"}})()
    )
    Handler.device = device
    global SAVER
    SAVER = Saver(device)
    srv = ThreadingHTTPServer((args.host, args.port), Handler)
    print("CS80 面板编辑器: http://%s:%d  (layout: %s)" % (args.host, args.port, LAYOUT))
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
