"""Agent 显示语义（单点）自检：python3 panel/test_agent_display.py

规则以后只改 panel/host.py 的 _agent_display / _dur_txt，
渲染端（renderer/src/widgets/agent.rs、panel/web/widgets.js）只做 tone→颜色映射。
"""
import host


def col(d):
    return (d["tone"], d["bucket"], d["muted"], d["blink"], d["label"], d["right"])


def main():
    # 运行中：呼吸点 + 已运行时长；没 runningFor 时退回「运行中」
    assert col(host._agent_display("running", 5, 150)) == ("up", 0, False, True, "运行中", "运行 3m")
    assert host._agent_display("running", 5, None)["right"] == "运行中"
    # 等待确认：琥珀 + 呼吸
    assert col(host._agent_display("waiting", 300)) == ("warn", 1, False, True, "等待", "等待")
    # 异常退出：1 小时内红字，超过 1 小时变灰但保留标签
    assert col(host._agent_display("error", 600)) == ("bad", 2, False, False, "异常退出", "异常退出")
    assert col(host._agent_display("error", 7200)) == ("muted", 4, True, False, "异常退出", "异常退出")
    # 结束：1 小时内蓝色，1 分钟内显示「刚刚」
    assert col(host._agent_display("done", 30)) == ("info", 3, False, False, "", "刚刚")
    assert col(host._agent_display("done", 600)) == ("info", 3, False, False, "", "10m")
    assert col(host._agent_display("done", 7200)) == ("muted", 4, True, False, "", "2h")
    assert col(host._agent_display("idle", 90000)) == ("muted", 4, True, False, "", "1d")
    # 时长格式：半分钟进一
    assert host._dur_txt(150) == "3m" and host._dur_txt(59) == "59s"
    assert host._dur_txt(5400) == "2h" and host._dur_txt(172800) == "2d"
    # 排序档位：同一档内按 age，不同档不交叉（老 error 必须沉底）
    items = [
        {"display": host._agent_display("error", 7200), "age": 7200},
        {"display": host._agent_display("done", 600), "age": 600},
        {"display": host._agent_display("running", 1, 5), "age": 1},
    ]
    items.sort(key=lambda it: (it["display"]["bucket"], it["age"]))
    assert [it["display"]["tone"] for it in items] == ["up", "info", "muted"], items
    print("agent display: 全部通过")


if __name__ == "__main__":
    main()
