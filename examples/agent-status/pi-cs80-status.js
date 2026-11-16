// Pi extension: write a normalized Agent status for the CS80 panel.
//
// Install:
//   mkdir -p ~/.pi/agent/extensions
//   cp pi-cs80-status.js ~/.pi/agent/extensions/
//   # then run /reload inside Pi
//
// It only writes IDs/paths/state/tool names. No prompts, tool output, or secrets.
// See docs/agent-status.md for the protocol and host integration notes.

import { mkdirSync, writeFileSync, renameSync, unlinkSync } from "node:fs";
import { join } from "node:path";

const DIR = join(
  process.env.HOME || process.env.USERPROFILE || ".",
  ".cs80_panel",
  "agent-status",
  "pi",
);

/** @typedef {"running" | "waiting_confirmation" | "done" | "error"} AgentState */

/** @param {string} text */
function safeName(text) {
  return String(text || "unknown").replace(/[^A-Za-z0-9._-]/g, "_");
}

export default function (pi) {
  /** @type {string} */
  let sessionId = "";
  /** @type {string | null} */
  let sessionFile = null;
  /** @type {string} */
  let cwd = "";
  /** @type {string} */
  let statusFile = "";
  /** @type {string | null} */
  let title = null;
  /** @type {string | null} */
  let promptKind = null;
  /** @type {string | null} */
  let tool = null;
  /** @type {number | null} */
  let runningSince = null;
  let active = false;

  /** @param {AgentState} state @param {Record<string, unknown>} [extra] */
  function publish(state, extra = {}) {
    if (!sessionId || !statusFile) return;
    try {
      mkdirSync(DIR, { recursive: true, mode: 0o700 });
      const body = {
        v: 1,
        agent: "pi",
        sessionId,
        sessionFile,
        pid: process.pid,
        cwd,
        state,
        promptKind,
        tool,
        runningSince,
        title,
        source: "pi-extension",
        updatedAt: Date.now(),
        ...extra,
      };
      const tmp = `${statusFile}.${process.pid}.tmp`;
      writeFileSync(tmp, JSON.stringify(body), { mode: 0o600 });
      try {
        renameSync(tmp, statusFile);
      } catch {
        // Windows 上 rename 覆盖已有文件有时会失败，退化为直接写。
        writeFileSync(statusFile, JSON.stringify(body), { mode: 0o600 });
        try { unlinkSync(tmp); } catch {}
      }
    } catch {
      // 状态旁路不能影响 Pi 主流程。
    }
  }

  function refreshIdentity(ctx) {
    sessionId = ctx.sessionManager.getSessionId() || "";
    sessionFile = ctx.sessionManager.getSessionFile() || null;
    cwd = ctx.sessionManager.getCwd() || ctx.cwd || "";
    title = ctx.sessionManager.getSessionName() || null;
    statusFile = sessionId ? join(DIR, `${safeName(sessionId)}.json`) : "";
  }

  pi.on("session_start", (_event, ctx) => {
    refreshIdentity(ctx);
    active = false;
    promptKind = null;
    tool = null;
    runningSince = null;
    publish("done");
  });

  pi.on("session_info_changed", (event, ctx) => {
    refreshIdentity(ctx);
    title = event.name || null;
    publish(promptKind ? "waiting_confirmation" : active ? "running" : "done");
  });

  pi.on("agent_start", () => {
    active = true;
    if (runningSince == null) runningSince = Date.now();
    publish("running");
  });

  pi.on("ui_prompt_start", (event) => {
    promptKind = event.kind || "confirm";
    publish("waiting_confirmation", { promptKind });
  });

  pi.on("ui_prompt_end", () => {
    promptKind = null;
    publish(active ? "running" : "done");
  });

  pi.on("tool_execution_start", (event) => {
    tool = event.toolName || null;
    if (!promptKind) publish("running", { tool });
  });

  pi.on("tool_execution_end", () => {
    tool = null;
    if (!promptKind) publish("running");
  });

  // agent_end 之后仍可能重试/压缩/继续；agent_settled 才是最终空闲。
  pi.on("agent_settled", () => {
    active = false;
    tool = null;
    if (!promptKind) runningSince = null;
    publish(promptKind ? "waiting_confirmation" : "done");
  });

  pi.on("session_shutdown", () => {
    if (!statusFile) return;
    try { unlinkSync(statusFile); } catch {}
  });
}
