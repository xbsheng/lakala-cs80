#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""改 QDisplay 固件的 DSI 像素时钟 `dsi_pclk_rate`（中间灰闪烁的开关）。

为什么打这个补丁（issue #4，社区实测）：

    QDisplay 在 `mipi_init` 里按下面这串常量调面板：

        文件偏移   字节        指令
        0x1678     14 21      movs r1, #20      HSync
        0x167a     28 22      movs r2, #40      HBP
        0x167e     03 23      movs r3, #3       VSync
        0x1680     0b 20      movs r0, #11      VBP
        0x1686     10 21      movs r1, #16      VFP
        0x168e     3c 22      movs r2, #60      FrameRate
        0x1690     64 23      movs r3, #100     dsi_pclk_rate   ← 本脚本只改这一字节

    前 6 个值和 QuecPython 官方 `mipi_init(HSync=20, HBP=40, …, VFP=16, FrameRate=60)`
    完全一致（见 archive/quecpython/gen_panel.py），所以第 7 个常量不进 QuecPython API、
    只有 QDisplay 自己的 native 实现才用得到 —— 就是 `dsi_pclk_rate`。

    100 → 200 后社区三台机器上「白/黑不闪、中间灰闪」的现象基本消失。
    注意不是越大越好：issue #4 的扫描显示 195 以下更差、220/240 又变差，
    200~210 是舒适区，`movs #imm8` 上限 255。

改了什么字节：

    `movs r3, #imm8` 是 16 位 Thumb 指令 0x23xx，内存里低字节在前：
        [0x1690] = imm8（要改的那一字节）      [0x1691] = 0x23（opcode，不动）
    改完后 APP2 容器头 u32@8 的整文件 CRC32 必须重算（appimg.fix），
    否则 loader 不启动 APP —— 现象是「刷写/读回都成功，但屏黑、QDC1 无应答」。

用法（Windows 刷机机）：

    py\\python.exe patch_pclk.py qdisplay_native.bin            # 默认 200 → *_pclk200.bin
    py\\python.exe patch_pclk.py qdisplay_native.bin 220        # 换档
    py\\python.exe qdisplay_app.py qdisplay_native_pclk200.bin --auto
    # 回退：拿原来的 qdisplay_native.bin 再刷一次即可

0.3.0（154400 B）和 0.3.1（155456 B）都适用：两个版本的 `mipi_init` 常量块逐字节相同。
"""
import hashlib
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # embed python 的 ._pth 不含脚本目录
import appimg

try:  # Windows 控制台编码不是 UTF-8 时不要让中文 print 直接抛异常
    sys.stdout.reconfigure(errors="replace")
except Exception:
    pass

DEFAULT_RATE = 200
# mipi_init 常量块指纹：`str r3,[r5]` + `movs r2,#60`(FrameRate)，紧跟的就是 pclk
FINGERPRINT = bytes.fromhex("2b603c22")
OPCODE_MOVS_R3 = 0x23


def patch(data, rate=DEFAULT_RATE):
    """把 `dsi_pclk_rate` 改成 rate，返回 (新 bytes, 字节偏移)。改不了就 raise ValueError。

    已经是 rate 时原样返回（不重算 CRC）。flash_oneclick.py 也走这个函数。
    """
    if not 1 <= rate <= 255:
        raise ValueError("pclk 只能是 1..255（`movs #imm8` 的上限就是 255）")
    data = bytearray(data)
    # 容器 magic 是 u32 0x41505032，落在文件里就是 ASCII "2PPA"
    if data[:4] not in (b"2PPA", b"APP2"):
        raise ValueError("不是 APP2 容器（magic 不是 0x41505032）")
    declared = int.from_bytes(data[4:8], "little")
    if declared != len(data):
        raise ValueError("容器长度字段 %d != 文件长度 %d" % (declared, len(data)))
    hits = []
    pos = data.find(FINGERPRINT)
    while pos >= 0:
        hits.append(pos)
        pos = data.find(FINGERPRINT, pos + 1)
    if len(hits) != 1:
        raise ValueError("mipi_init 常量块指纹命中 %d 处（%s），不敢猜"
                         % (len(hits), [hex(h) for h in hits]))
    site = hits[0] + len(FINGERPRINT)          # → movs r3,#imm8 的 imm8 字节
    if data[site + 1] != OPCODE_MOVS_R3:
        raise ValueError("补丁点后面不是 movs r3,#imm8（%s），固件版本不匹配"
                         % data[site:site + 2].hex())
    if data[site] == rate:
        return bytes(data), site
    data[site] = rate
    out = bytes(appimg.fix(data))      # ★ APP2 头部 u32@8 = 整文件 CRC32，必须重算
    assert appimg.check(out)
    return out, site


def main():
    args = sys.argv[1:]
    if not args or args[0] in ("-h", "--help"):
        print(__doc__)
        return 2
    src = args[0]
    if not os.path.exists(src):
        print("找不到文件：%s" % src)
        return 2
    rate = DEFAULT_RATE
    if len(args) > 1:
        try:
            rate = int(args[1], 0)
        except ValueError:
            print("pclk 必须是整数：%s" % args[1])
            return 2
    data = open(src, "rb").read()
    if not appimg.check(data):
        print("注意：输入 bin 自己的 crc32@8 就不对（可能是别人改过又没重算），先确认来源。")
    print("输入：%s  %d B  sha256=%s" % (src, len(data), hashlib.sha256(data).hexdigest()))

    try:
        out, site = patch(data, rate)
    except ValueError as exc:
        print("%s，放弃" % exc)
        return 1
    old = data[site]
    print("补丁点：文件偏移 0x%x（虚拟地址 0x%x）  dsi_pclk_rate = %d → %d"
          % (site, 0x60260000 + site, old, rate))
    if old == rate:
        print("已经是 %d，无需修改。" % rate)
        return 0
    if old != 100:
        print("注意：原值不是 100（是 %d），可能是别人改过的固件，按你的要求继续。" % old)

    dst = os.path.splitext(src)[0] + "_pclk%d.bin" % rate
    open(dst, "wb").write(out)

    diff = [i for i, (a, b) in enumerate(zip(out, data)) if a != b]
    print("差异字节：%s（应为 0x%x + crc32@0x8..0xb，共 5 字节）"
          % (["0x%x" % i for i in diff], site))
    print("输出：%s  %d B  crc32@8=%08x(ok)  sha256=%s"
          % (dst, len(out), appimg.stored(out), hashlib.sha256(out).hexdigest()))
    print("刷写：py\\python.exe qdisplay_app.py %s --auto" % os.path.basename(dst))
    print("回退：py\\python.exe qdisplay_app.py %s --auto" % os.path.basename(src))
    return 0


if __name__ == "__main__":
    sys.exit(main())
