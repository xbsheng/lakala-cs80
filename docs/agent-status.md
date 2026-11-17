# Agent 精确状态协议（Sidecar）

> 状态：host 已接入 status 目录；示例代码在 `examples/agent-status/`。  
> 目标：让 Agent 组件拿到「running / waiting_confirmation / done / error」的**精确状态**，
> 不再依赖 mtime、子进程、CPU 等启发式推断。

## 1. 为什么要 Sidecar

当前 `/api/data/agents` 靠扫 JSONL + 进程推断，已经能覆盖大多数情况，但仍有三类边界：

1. 同一 Agent 多个并行会话，进程和会话文件不是一一对应。
2. `waiting` 到底是「工具在跑」还是「等用户确认」，从进程侧只能猜。
3. in-process 工具、常驻 MCP 子进程会干扰子进程判断。

最可靠的办法是让 Agent 自己在生命周期事件里写一份状态：

```text
Pi extension ─┐
Claude hooks ─┼─→ ~/.cs80_panel/agent-status/<agent>/<session>.json
Codex hooks  ─┘                 ↓
                         host.py /api/data/agents
                                 ↓
                           Agent 组件
```

- **Pi**：extension 直接用 `ui_prompt_start/end`、`agent_settled`，最精确。
- **Claude Code**：hooks 的 `PermissionRequest` / `Notification` / `Stop`。
- **Codex**：新版 lifecycle hooks 的 `PermissionRequest` / `Stop`。
- **DeepSeek Harness**：目前没有稳定公开 hook，只能继续用现有推断。

## 2. 统一状态文件

目录：

```text
~/.cs80_panel/agent-status/<agent>/<session-id>.json
```

示例：

```json
{
  "v": 1,
  "agent": "pi",
  "sessionId": "01a1dec0-c9a7-778c-977b-c3a6a84dc833",
  "sessionFile": "/root/.pi/agent/sessions/--root-lakala-cs80--/xxx.jsonl",
  "pid": 78738,
  "cwd": "/root/lakala-cs80",
  "state": "waiting_confirmation",
  "promptKind": "confirm",
  "tool": "bash",
  "runningSince": 1794810000000,
  "title": "lakala-cs80 · 检查 Agent 状态",
  "titleSource": "auto",
  "project": "lakala-cs80",
  "source": "pi-extension",
  "updatedAt": 1794820000000
}
```

字段：

| 字段 | 说明 |
|---|---|
| `v` | 协议版本，当前 `1` |
| `agent` | `pi` / `claude` / `codex` / `deepseek` |
| `sessionId` | Agent 自己的会话 ID |
| `sessionFile` | transcript/JSONL 路径，可为空 |
| `pid` | Agent 进程 PID；hook adapter 为空 |
| `cwd` | 工作目录 |
| `state` | `running` / `waiting_confirmation` / `done` / `error` |
| `promptKind` | `confirm` / `select` / `input` / `editor` / `custom` / `permission` |
| `tool` | 当前/最后一次工具名，可为空 |
| `runningSince` | 当前这轮开始运行的 Unix 毫秒；done 时清空 |
| `title` | 屏幕展示标题；Pi 扩展写 `project · 规则摘要` |
| `titleSource` | `auto` / `manual` / null |
| `project` | 项目名（cwd basename），可为空 |
| `source` | `pi-extension` / `claude-hook` / `codex-hook` |
| `updatedAt` | Unix 毫秒时间戳 |

JSON Schema 见：

[`examples/agent-status/status.schema.json`](../examples/agent-status/status.schema.json)

### 状态语义

```text
running
  Agent 正在执行（模型/Tool/子任务）

waiting_confirmation
  明确在等用户操作：权限确认、select/input 对话框、MCP elicitation

done
  一轮任务已经结束，等用户输入新任务

error
  本轮以 API 错误/中断/异常结束
```

> `waiting_confirmation` 不要和 done 混淆：  
> 一句回答结束后等用户继续输入，是 `done`；  
> 工具调用被权限/确认框拦住，才是 `waiting_confirmation`。

组件显示：

- `running`：显示 `运行 5m`（从 `runningSince` 计算；无 sidecar 时从最后一次 user message 推断）
- `waiting_confirmation`：显示 `等待`
- `done`：1 小时内蓝色；不足 1 分钟显示 `刚刚`，之后显示 `5m` / `2h`
- 灰色：`done` 超过 1 小时，或 `idle`（超过 1 天）；logo、标题、状态点一起弱化
- `error`：显示 `异常退出`（红色；超过 1 小时和 `done` 一样变灰，不再抢眼）

## 3. Pi extension

示例：

[`examples/agent-status/pi-cs80-status.js`](../examples/agent-status/pi-cs80-status.js)

安装：

```bash
mkdir -p ~/.pi/agent/extensions
cp examples/agent-status/pi-cs80-status.js ~/.pi/agent/extensions/
```

然后在 Pi 里执行 `/reload`。Pi 会自动发现 `~/.pi/agent/extensions/*.js` / `*.ts`；不需要改 `settings.json`。

验证（`./cs80 status` 确认 host 在跑）：

```bash
ls ~/.cs80_panel/agent-status/pi/          # 每个活着的 Pi 会话一个 <sessionId>.json
cat ~/.cs80_panel/agent-status/pi/*.json | head -c 400
curl -s 127.0.0.1:8787/api/data/agents | python3 -m json.tool | head -30
```

状态文件只在 Pi 进程活着时可信（host 会核对 `pid`）；退出会话会自动删。
没看到文件 = 没 `reload` 或扩展报错，在 Pi 里 `/extensions` 看一眼。

### 事件映射

| Pi 事件 | 状态 |
|---|---|
| `session_start` | `done` |
| `agent_start` | `running` |
| `ui_prompt_start` | `waiting_confirmation` |
| `ui_prompt_end` | `running` 或 `done` |
| `tool_execution_start` | `running`（记录 `tool`） |
| `tool_execution_end` | 保持 `running`（模型还会继续） |
| `agent_end` | 记下这一轮消息的 `stopReason`；有 `error` / `aborted` 就标记失败 |
| `agent_settled` | `error`（这一轮失败过）/ `done` |

> 失败必须由扩展自己写 `error`：sidecar 状态优先级高于 host 的 JSONL 推断，
> 之前扩展在 `agent_settled` 里一律写 `done`，网络中断/取消的会话就会显示成**蓝色**而不是红色。
> 自动重试会再走一次 `agent_start`，失败标记在那时清掉，重试成功仍然算 `done`。
| `session_shutdown` | 删除状态文件 |

关键点：

- 用 **`agent_settled`** 而不是 `agent_end`。`agent_end` 之后可能自动重试、压缩、继续跑。
- 用 **`ui_prompt_start/end`**，不要从工具卡住时间推断确认框。
- `sessionManager.getSessionId()` / `getSessionFile()` 提供精确会话，不依赖 cwd。
- 状态文件原子写入（临时文件 + rename），权限 `0600`。

### 自动标题

Pi 扩展会在 `before_agent_start` 收到 `event.prompt` 时生成：

```text
<项目名> · <最后一条用户提问的规则摘要>
```

例如：

```text
lakala-cs80 · 检查接口，为什么设备一直断开
```

规则摘要不是模型总结，只做本地提取：

- 取第一行/第一句
- 去掉代码块、URL、绝对路径、邮箱、IP、常见 token 形状
- 过滤 `/command`、`继续`、`ok` 这类无意义输入
- 项目名最多 12 字符，摘要最多 14 字符
- `/name` 手动标题优先，不会被自动标题覆盖

因此它能“简短”，但不能保证 100% 去敏感；屏幕公开场景建议再加手动别名或中性标题。

## 4. Claude Code hooks

示例脚本：

[`examples/agent-status/cs80-agent-status.py`](../examples/agent-status/cs80-agent-status.py)

示例配置：

[`examples/agent-status/claude-settings.example.json`](../examples/agent-status/claude-settings.example.json)

把配置合并到：

```text
~/.claude/settings.json
```

或项目级：

```text
.claude/settings.json
```

### 事件映射

| Claude hook | 状态 |
|---|---|
| `SessionStart` | `done` |
| `UserPromptSubmit` | `running` |
| `PreToolUse` | `running` |
| `PostToolUse` / `PostToolUseFailure` | `running` |
| `PermissionRequest` | `waiting_confirmation`（立即） |
| `Notification(permission_prompt)` | `waiting_confirmation`（约 6s 后兜底） |
| `Notification(agent_needs_input)` | `waiting_confirmation` |
| `Notification(idle_prompt)` / `agent_completed` | `done` |
| `Stop` | `done` |
| `StopFailure` | `error` |
| `SessionEnd` | 删除状态文件 |

注意：

- `PermissionRequest` 是最准的权限确认信号。
- 沙箱网络请求可能不触发 `PermissionRequest`，此时靠 `Notification(permission_prompt)` 兜底。
- `Notification` 在终端里可能只在“用户看起来离开终端”时触发；所以两者都要接。
- Hook 脚本从 stdin 读 JSON，退出码 0，不输出，不阻塞 Agent。

## 5. Codex hooks

示例脚本同上；配置示例：

[`examples/agent-status/codex-hooks.example.json`](../examples/agent-status/codex-hooks.example.json)

Codex 当前版本支持 lifecycle hooks，事件模型和 Claude/Pi 接近：

| Codex hook | 状态 |
|---|---|
| `SessionStart` | `done` |
| `UserPromptSubmit` | `running` |
| `PreToolUse` | `running` |
| `PostToolUse` | `running` |
| `PermissionRequest` | `waiting_confirmation` |
| `Stop` | `done` |
| `Interrupt` | `done` |
| `SessionEnd` | 删除状态文件 |

老版本 Codex 只有：

```toml
notify = ["python3", "/path/to/notify.py"]
```

它只会在 `agent-turn-complete` 时触发，只能做 done 通知，**拿不到权限确认状态**。  
如果当前 Codex 版本不支持 hooks，只能继续用 host 的进程/JSONL 推断。

> Codex hooks 的配置位置和 JSON/TOML 形态随版本演进；示例文件按当前 hooks schema 写，
> 安装前先确认当前版本的 hooks 说明，并注意 Codex 可能要求首次信任 hooks。

## 6. DeepSeek Harness

DeepSeek Harness 是 Electron 应用，当前没有可用的：

- CLI 生命周期 hook
- 稳定的本地 RPC
- 权限确认事件
- 文本会话 transcript 路径

会话数据在 IndexedDB 里，host 只能看到进程。  
所以 DeepSeek 保持现有 fallback：

- 进程活着 → running
- 进程退出 → done
- 窗口标题/CPU/长时间无变化 → 只能推断，不保证准确

除非应用提供插件/API，否则不建议为它做不可靠的 UI 自动化。

## 7. host 接入方式（已实现）

`panel/host.py` 的 `agents()` 已经按以下顺序合并：

```python
statuses = _load_agent_statuses()
for session in scan_sessions():
    sidecar = _agent_status_for(statuses, agent, session.path)
    state = _status_state(sidecar, now) or infer_from_jsonl_and_process(session)
```

当前实现的 TTL/校验：

- `waiting_confirmation`：最多可信 6 小时
- `running`：最多可信 30 分钟；超时回退 JSONL/进程推断
- `done` / `error`：最多可信 24 小时
- 带 `pid` 的状态（Pi extension）会检查进程是否还活着
- 没有 `pid` 的 hook 状态靠 `updatedAt` + session 文件/进程兜底
- running 时输出 `runningFor` 秒数；UI 显示 `运行 5m`
- 数据刷新：有 `running` 会话时 1 秒一次；没有 running 时保持 3 秒
- **显示语义单点**：`/api/data/agents` 的每个 item 带一个 `display` 对象，
  由 `host._agent_display()` 一次算好；排序用的是同一个 `bucket`。
  渲染端（`renderer/src/widgets/agent.rs`、`panel/web/widgets.js`）**只做 `tone` → 颜色映射**，
  不许再各自推导"多少秒算旧""什么时候变灰""右侧写什么"，之前这类多实现已经出过不一致。

  | 字段 | 含义 |
  |---|---|
  | `bucket` | 排序档：`0` running / `1` waiting / `2` error<1h / `3` done<1h / `4` 其余 |
  | `tone` | 色调：`up` / `warn` / `bad` / `info` / `muted` |
  | `muted` | 是否弱化（灰） |
  | `blink` | 状态点呼吸 |
  | `label` | 右侧短标签（空 = 用 `right`） |
  | `right` | 右侧最终文案：`运行 3m` / `刚刚` / `10m` / `2h` / `异常退出` |

  时长格式也在这里统一（`150s → 3m`，半分钟进一，避免各家舍入不一致）。
  自检：`python3 panel/test_agent_display.py`。
- 排序跟屏幕分档一致：直接按 `display.bucket`，再按 `age`。早先的
  `order = {error: 2, done: 3}` 会让两天前的失败压在新会话上面，已废弃。

匹配规则：

1. `sessionFile` 完全相等优先。
2. 否则 `sessionId` 相等。
3. 找不到就回退到现有推断。
4. 标题优先使用 sidecar `title`，否则使用会话首条 user message。

新鲜度：

- Pi extension 带 `pid`：PID 还在、文件存在 → 可信；进程退出 → 删除/回退。
- Claude/Codex hook 没有 Agent PID：以 `updatedAt`、session mtime、进程存活共同判断；
  `Stop`/`SessionEnd` 会及时写 done/删除。
- 状态文件超过合理时间未更新（例如 30 分钟）且进程已不在 → 回退推断。

清理：

- `SessionEnd` / `session_shutdown` 删除对应文件。
- host 目前不主动清理孤儿状态文件；异常退出留下的文件可按需手动删除。

## 8. 通用性判断

| Agent | 精确等待确认 | 会话 ID | 工具事件 | 精确程度 |
|---|---|---|---|---|
| Pi | ✅ `ui_prompt_start` | ✅ | ✅ | 最高 |
| Claude Code | ✅ `PermissionRequest` | ✅ | ✅ | 高 |
| Codex | ✅ `PermissionRequest` | ✅ | ✅ | 高 |
| DeepSeek Harness | ❌ | ❌ | ❌ | 仅推断 |

**结论：事件模型有共性，配置协议没有统一标准。**

可以复用的部分是：

- 统一状态 schema
- 统一写入脚本（Claude/Codex 共用一个 Python adapter）
- host 的合并/回退逻辑
- UI 的四个状态

不能复用的是：

- Pi 的 `ui_prompt_start/end` 语义
- Claude 的 `Notification` 类型和 hook 配置
- Codex 的 hooks 版本/配置
- DeepSeek 缺少公开事件

## 9. 安全与限制

- Extension/hook 以当前用户权限运行，能读文件、执行命令；只安装审计过的代码。
- 状态文件只写 session id、路径、状态、工具名，不写 prompt/工具输出/token。
- 目录 `0700`，文件 `0600`。
- Hook 应该快速退出（<5s），不阻塞 Agent 主流程。
- 这是**状态旁路**，不修改 Agent 行为；不要在 hook 里做权限决策。
- Claude/Codex 的 hook 输入字段可能随版本变化，升级后需要回归测试。
