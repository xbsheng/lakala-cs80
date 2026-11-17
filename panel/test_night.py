"""夜间调光时间窗自检：python3 panel/test_night.py"""
import host


def at(hh, mm=0):
    return hh * 60 + mm


def main():
    # 00:00–07:00（不跨零点）
    assert host._in_window(at(0), at(0), at(7)) is True
    assert host._in_window(at(3), at(0), at(7)) is True
    assert host._in_window(at(6, 59), at(0), at(7)) is True
    assert host._in_window(at(7), at(0), at(7)) is False      # 右开：7:00 恢复白天
    assert host._in_window(at(12), at(0), at(7)) is False
    assert host._in_window(at(23, 59), at(0), at(7)) is False
    # 23:00–07:00（跨零点）
    assert host._in_window(at(23), at(23), at(7)) is True
    assert host._in_window(at(2), at(23), at(7)) is True
    assert host._in_window(at(12), at(23), at(7)) is False
    assert host._in_window(at(22, 59), at(23), at(7)) is False
    # 非法/空窗口
    assert host._in_window(at(1), None, at(7)) is False
    assert host._in_window(at(1), at(7), at(7)) is False
    # 时间解析
    assert host._hm_minutes("07:30") == 450
    assert host._hm_minutes("7:30") == 450
    assert host._hm_minutes("bad") is None
    # 默认值：00:00–07:00 / 亮度 5，enabled 默认开
    real = host.load_config
    host.load_config = lambda: {}
    try:
        assert host._night_cfg() == {"from": "00:00", "to": "07:00", "brightness": 5}
        host.load_config = lambda: {"night": {"enabled": False, "from": "23:00"}}
        cfg = host._night_cfg()
        assert cfg["enabled"] is False and cfg["from"] == "23:00" and cfg["brightness"] == 5
    finally:
        host.load_config = real
    print("night: 全部通过")


if __name__ == "__main__":
    main()
