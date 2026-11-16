#!/usr/bin/env python3
"""CS80 Agent status adapter for Claude Code / Codex lifecycle hooks.

Usage in a hook command (JSON arrives on stdin):

    python3 /ABS/PATH/examples/agent-status/cs80-agent-status.py claude
    python3 /ABS/PATH/examples/agent-status/cs80-agent-status.py codex

It writes:

    ~/.cs80_panel/agent-status/<agent>/<session-id>.json

The script is intentionally side-effect only: it never prints to stdout, never
blocks the Agent, and always exits 0. See docs/agent-status.md.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import sys
import time
from pathlib import Path

STATE_DIR = Path.home() / ".cs80_panel" / "agent-status"


def safe_name(text: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]", "_", text or "unknown")


def load_previous(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {}


def atomic_write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        os.chmod(path.parent, 0o700)
    except OSError:
        pass
    tmp = path.with_name(path.name + f".{os.getpid()}.tmp")
    tmp.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
    try:
        os.chmod(tmp, 0o600)
    except OSError:
        pass
    os.replace(tmp, path)


def event_state(agent: str, event: str, notification: str) -> tuple[str | None, str | None]:
    """Return (state, prompt_kind). None means "leave the previous state alone"."""
    ev = event.lower().replace("_", "").replace("-", "")
    nt = notification.lower().replace("_", "").replace("-", "")

    if agent == "claude":
        if ev == "sessionstart":
            return "done", None
        if ev in ("userpromptsubmit", "pretooluse", "posttooluse", "posttoolusefailure", "permissiondenied"):
            return "running", None
        if ev == "permissionrequest":
            return "waiting_confirmation", "permission"
        if ev == "stop":
            return "done", None
        if ev == "stopfailure":
            return "error", None
        if ev == "sessionend":
            return "remove", None
        if ev == "notification":
            if nt in ("permissionprompt", "elicitationdialog", "elicitationurldialog", "agentneedsinput"):
                return "waiting_confirmation", "permission"
            if nt in ("idleprompt", "agentcompleted"):
                return "done", None
        return None, None

    if agent == "codex":
        if ev == "sessionstart":
            return "done", None
        if ev in ("userpromptsubmit", "pretooluse", "posttooluse"):
            return "running", None
        if ev == "permissionrequest":
            return "waiting_confirmation", "permission"
        if ev == "stop":
            return "done", None
        if ev == "interrupt":
            return "done", None
        if ev == "sessionend":
            return "remove", None
        return None, None

    return None, None


def main() -> None:
    agent = (sys.argv[1] if len(sys.argv) > 1 else "claude").strip().lower()
    if agent not in ("claude", "codex"):
        return

    try:
        payload = json.loads(sys.stdin.read() or "{}")
    except Exception:
        return
    if not isinstance(payload, dict):
        return

    event = str(payload.get("hook_event_name") or payload.get("event") or "")
    notification = str(payload.get("notification_type") or "")
    session_id = str(payload.get("session_id") or payload.get("thread_id") or "").strip()
    session_file = str(
        payload.get("transcript_path") or payload.get("session_file") or ""
    ).strip()
    cwd = str(payload.get("cwd") or "")

    if not session_id:
        # transcript_path 通常比 session_id 更稳定；没有 ID 时用路径哈希兜底。
        session_id = (
            hashlib.sha1(session_file.encode("utf-8")).hexdigest()[:16]
            if session_file
            else ""
        )
    if not session_id:
        return

    path = STATE_DIR / agent / (safe_name(session_id) + ".json")
    state, prompt_kind = event_state(agent, event, notification)
    if state == "remove":
        try:
            path.unlink()
        except OSError:
            pass
        return
    if state is None:
        return

    previous = load_previous(path)
    if previous.get("sessionFile") and not session_file:
        session_file = previous["sessionFile"]
    if previous.get("title") and not payload.get("title"):
        title = previous["title"]
    else:
        title = payload.get("title") or None
    if previous.get("tool") and state == "waiting_confirmation" and not payload.get("tool_name"):
        tool = previous["tool"]
    else:
        tool = payload.get("tool_name") or None

    if state != "waiting_confirmation":
        prompt_kind = None
    if state in ("done", "error"):
        tool = None

    now_ms = int(time.time() * 1000)
    previous_state = previous.get("state")
    try:
        running_since = int(previous.get("runningSince") or 0) or None
    except (TypeError, ValueError):
        running_since = None
    event_key = event.lower().replace("_", "").replace("-", "")
    if state == "running":
        if event_key == "userpromptsubmit" or previous_state not in ("running", "waiting_confirmation") or not running_since:
            running_since = now_ms
    elif state == "waiting_confirmation":
        if not running_since:
            running_since = now_ms
    else:
        running_since = None

    atomic_write(path, {
        "v": 1,
        "agent": agent,
        "sessionId": session_id,
        "sessionFile": session_file or None,
        "cwd": cwd or previous.get("cwd") or "",
        "state": state,
        "promptKind": prompt_kind,
        "tool": tool,
        "runningSince": running_since,
        "title": title,
        "source": f"{agent}-hook",
        "updatedAt": now_ms,
    })


if __name__ == "__main__":
    try:
        main()
    except Exception:
        # Hooks must never break the Agent.
        pass
    raise SystemExit(0)
