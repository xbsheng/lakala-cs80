# Agent status sidecar examples

给 CS80 Agent 组件用的「精确状态」示例。设计说明见
[`docs/agent-status.md`](../../docs/agent-status.md)。

> 这些代码是可运行的示例；host 已经会读取
> `~/.cs80_panel/agent-status/`，状态文件存在且新鲜时优先于 JSONL/进程推断。
> 合并逻辑见文档 §7。

## 文件

| 文件 | 用途 |
|---|---|
| `status.schema.json` | 统一状态文件 JSON Schema |
| `pi-cs80-status.js` | Pi 全局 extension，精确等待确认/完成 |
| `cs80-agent-status.py` | Claude Code / Codex 共用的 hook adapter |
| `claude-settings.example.json` | Claude Code hooks 配置示例 |
| `codex-hooks.example.json` | Codex lifecycle hooks 配置示例 |

状态文件写到：

```text
~/.cs80_panel/agent-status/<agent>/<session-id>.json
```

只写 session id、路径、状态、工具名和标题，不写 prompt、工具输出或 token。

## Pi

```bash
mkdir -p ~/.pi/agent/extensions
cp examples/agent-status/pi-cs80-status.js ~/.pi/agent/extensions/
```

然后在 Pi 里执行：

```text
/reload
```

Pi 会自动发现 `~/.pi/agent/extensions/*.js` / `*.ts`，不需要改 `settings.json`。

验证：

```bash
ls ~/.cs80_panel/agent-status/pi/
cat ~/.cs80_panel/agent-status/pi/*.json
```

状态含义：

- `running`：Agent 正在执行
- `waiting_confirmation`：明确在等权限/确认对话框
- `done`：一轮结束，等新输入
- `error`：异常结束

## Claude Code

把 `cs80-agent-status.py` 放到任意稳定路径，然后把
`claude-settings.example.json` 里的 `hooks` 合并到：

```text
~/.claude/settings.json
```

或者项目级：

```text
.claude/settings.json
```

把示例中的 `/ABS/PATH` 换成仓库绝对路径。不要直接覆盖已有 settings，
只合并 `hooks` 字段。

关键事件：

- `PermissionRequest` → `waiting_confirmation`
- `Notification(permission_prompt)` → 6 秒后的兜底
- `Stop` → `done`
- `SessionEnd` → 删除状态

## Codex

把示例里的 `/ABS/PATH` 换成仓库绝对路径，然后按当前 Codex 版本放到 hooks
配置位置（不同版本可能是 `~/.codex/hooks.json` 或 `config.toml` 内联 hooks）。
Codex 可能需要首次信任 hooks。

老版本只有 `notify` 的话，只能收到 turn-complete，不能做确认状态；这种情况继续用
host 的 JSONL/进程推断。

## 测试 adapter（不会碰真实状态）

```bash
export HOME=/tmp/cs80-agent-status-test
mkdir -p "$HOME"
printf '%s\n' '{"session_id":"demo","transcript_path":"/tmp/demo.jsonl","cwd":"/tmp","hook_event_name":"PermissionRequest","tool_name":"Bash"}' \
  | python3 examples/agent-status/cs80-agent-status.py claude
cat "$HOME/.cs80_panel/agent-status/claude/demo.json"

printf '%s\n' '{"session_id":"demo","hook_event_name":"Stop"}' \
  | python3 examples/agent-status/cs80-agent-status.py claude
cat "$HOME/.cs80_panel/agent-status/claude/demo.json"   # state=done

printf '%s\n' '{"session_id":"demo","hook_event_name":"SessionEnd"}' \
  | python3 examples/agent-status/cs80-agent-status.py claude
ls "$HOME/.cs80_panel/agent-status/claude/"             # demo.json 已删除
```

## Host 合并规则（待接入）

```text
sessionId / sessionFile 匹配
  → 状态文件存在且新鲜
    → waiting_confirmation 映射为 UI 的 waiting
  → 否则回退到现有 JSONL + 进程推断
```

Pi 的状态文件有 `pid`，可校验进程；Claude/Codex hook 没有 Agent PID，
以 `updatedAt`、session mtime 和进程检查共同判断。

## 安全

- Extension/hook 以当前用户权限运行，安装前审计代码。
- 目录 `0700`，状态文件 `0600`。
- Adapter 永不输出到 stdout、永不阻塞 Agent；异常直接吞掉并退出 0。
