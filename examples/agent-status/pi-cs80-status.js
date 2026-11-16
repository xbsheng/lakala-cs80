// Pi extension: write a normalized Agent status for the CS80 panel.
//
// Install:
//   mkdir -p ~/.pi/agent/extensions
//   cp pi-cs80-status.js ~/.pi/agent/extensions/
//   # then run /reload inside Pi
//
// It only writes IDs/paths/state/tool names and a short rule-based title.
// No prompts, tool output, or secrets are written.
// See docs/agent-status.md for the protocol and host integration notes.

import { mkdirSync, writeFileSync, renameSync, unlinkSync } from "node:fs";
import { basename, join } from "node:path";

const DIR = join(
  process.env.HOME || process.env.USERPROFILE || ".",
  ".cs80_panel",
  "agent-status",
  "pi",
);

const PROJECT_MAX = 12;
const SUMMARY_MAX = 14;
const SKIP_PROMPTS = new Set([
  "继续", "继续吧", "好的", "好", "可以", "行", "嗯", "对",
  "ok", "okay", "yes", "y", "go", "go on", "continue",
]);

/** @typedef {"running" | "waiting_confirmation" | "done" | "error"} AgentState */

/** @param {string} text */
function safeName(text) {
  return String(text || "unknown").replace(/[^A-Za-z0-9._-]/g, "_");
}

/** @param {string} text @param {number} max */
function clip(text, max) {
  const chars = Array.from(String(text || "").trim());
  if (!chars.length) return "";
  return chars.length > max ? chars.slice(0, max).join("") + "…" : chars.join("");
}

/** @param {unknown} content */
function contentText(content) {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .map((part) => {
      if (typeof part === "string") return part;
      if (!part || typeof part !== "object") return "";
      return part.type === "text" || part.type === "input_text" ? String(part.text || "") : "";
    })
    .filter(Boolean)
    .join(" ");
}

/**
 * Very small, offline extractive summary. It is intentionally conservative:
 * code, URLs, paths, emails, IPs and common secret shapes are removed.
 * @param {string} text
 */
function cleanPrompt(text) {
  let s = String(text || "");
  s = s.replace(/```[\s\S]*?```/g, " ").replace(/`[^`]*`/g, " ");
  s = s.replace(/<system-reminder>[\s\S]*?<\/system-reminder>/gi, " ");
  s = s.replace(/<[^>]+>/g, " ");
  s = s.replace(/https?:\/\/[^\s]+/gi, " ");
  s = s.replace(/\b[\w.+-]+@[\w-]+\.[\w.-]+\b/g, " ");
  s = s.replace(/\b(?:\d{1,3}\.){3}\d{1,3}\b/g, " ");
  s = s.replace(/\b(?:sk-[A-Za-z0-9_-]{8,}|gh[pousr]_[A-Za-z0-9]{10,}|AKIA[0-9A-Z]{16})\b/g, " ");
  s = s.replace(/\b(?:token|password|passwd|secret|api[_-]?key|authorization)\s*[:=]\s*\S+/gi, " ");
  s = s.replace(/(?:[A-Za-z]:\\|\/)(?:[^\s/\\]+\/)+[^\s/\\]+/g, " ");
  s = s.replace(/\b[0-9a-f]{24,}\b/gi, " ");
  s = s.replace(/\b[0-9a-f]{8}-[0-9a-f-]{27,}\b/gi, " ");

  let first = s
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean) || "";
  first = first
    .replace(/^[#>*\-\d.\s]+/, "")
    .replace(/^(?:(?:请|麻烦|帮我|帮忙|please)\s*)+/i, "")
    .replace(/\s+(?:和|以及|跟|与)\s+/g, " ")
    .replace(/\s*(?:这个|那个|这些|那些)\s*/g, " ")
    .replace(/\s+(?:的|了|地|得)\s*/g, "")
    .replace(/\s+/g, " ")
    .trim();

  const sentence = first.match(/^.{1,80}?[。！？!?]/);
  if (sentence) first = sentence[0];
  if (!first || first.length < 2) return "";
  if (/^[/!]/.test(first) || SKIP_PROMPTS.has(first.toLowerCase())) return "";
  return clip(first, SUMMARY_MAX);
}

/** @param {string} project @param {string} summary */
function makeTitle(project, summary) {
  const p = clip(project || "Pi", PROJECT_MAX) || "Pi";
  return summary ? `${p} · ${clip(summary, SUMMARY_MAX)}` : p;
}

/** @param {string} value */
function projectFromCwd(value) {
  return basename(String(value || "")) || "Pi";
}

/** @param {any} ctx */
function projectName(ctx) {
  return projectFromCwd(ctx.sessionManager?.getCwd?.() || ctx.cwd || "");
}

/** @param {any} ctx */
function lastUserPrompt(ctx) {
  try {
    const entries = ctx.sessionManager?.getBranch?.() || [];
    for (let i = entries.length - 1; i >= 0; i--) {
      const entry = entries[i];
      if (entry?.type === "message" && entry.message?.role === "user") {
        return contentText(entry.message.content);
      }
    }
  } catch {
    // ignore
  }
  return "";
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
  /** @type {"manual" | "auto" | null} */
  let titleSource = null;
  /** @type {string | null} */
  let manualTitle = null;
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
        titleSource,
        project: projectFromCwd(cwd),
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

  function applyAutoTitle(prompt) {
    if (manualTitle) return;
    const summary = cleanPrompt(prompt);
    if (!summary) return;
    title = makeTitle(projectFromCwd(cwd), summary);
    titleSource = "auto";
  }

  function refreshIdentity(ctx) {
    sessionId = ctx.sessionManager.getSessionId() || "";
    sessionFile = ctx.sessionManager.getSessionFile() || null;
    cwd = ctx.sessionManager.getCwd() || ctx.cwd || "";
    manualTitle = ctx.sessionManager.getSessionName() || null;
    title = manualTitle;
    titleSource = manualTitle ? "manual" : null;
    statusFile = sessionId ? join(DIR, `${safeName(sessionId)}.json`) : "";
  }

  pi.on("session_start", (_event, ctx) => {
    refreshIdentity(ctx);
    active = false;
    promptKind = null;
    tool = null;
    runningSince = null;
    if (!manualTitle) applyAutoTitle(lastUserPrompt(ctx));
    if (!title) title = makeTitle(projectName(ctx), "");
    publish("done");
  });

  pi.on("session_info_changed", (event, ctx) => {
    refreshIdentity(ctx);
    if (event.name) {
      manualTitle = event.name;
      title = event.name;
      titleSource = "manual";
    } else {
      manualTitle = null;
      title = null;
      titleSource = null;
      applyAutoTitle(lastUserPrompt(ctx));
      if (!title) title = makeTitle(projectName(ctx), "");
    }
    publish(promptKind ? "waiting_confirmation" : active ? "running" : "done");
  });

  pi.on("before_agent_start", (event, ctx) => {
    // Only real user prompts; custom/system-triggered runs don't change the title.
    if (event.messageType !== "user" || manualTitle) return;
    applyAutoTitle(event.prompt || "");
    if (!title) title = makeTitle(projectName(ctx), "");
    // agent_start normally follows immediately; publish here so a newly
    // generated title is visible even if the run is very short.
    publish("running");
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
