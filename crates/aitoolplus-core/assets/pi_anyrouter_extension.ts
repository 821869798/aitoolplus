// @aitoolplus-extension: anyrouter
// @aitoolplus-version: 1.0.0
/**
 * AIToolPlus Self-Maintained AnyRouter Provider Extension for Pi Coding Agent
 *
 * Adapted from xifan2333/pi-anyrouter and phy-zhangzl/pi-anyrouter-cc (MIT License).
 * Maintained directly by AIToolPlus so users do not depend on external repositories.
 *
 * Protocol support:
 * 1. Claude Code protocol (/v1/messages?beta=true) with Claude Agent SDK fingerprint,
 *    system prompt blocks, metadata.user_id, and PascalCase tool name mapping.
 * 2. Codex Responses Lite protocol (/v1/responses) for OpenAI/GPT/Codex models.
 */

export const AITOOLPLUS_ANYROUTER_EXT_VERSION = "1.0.0";

import { AsyncLocalStorage } from "node:async_hooks";
import { randomBytes, randomUUID } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { ProxyAgent, fetch as undiciFetch } from "undici";
import * as piAi from "@earendil-works/pi-ai";
import {
  calculateCost,
  collapseSystemMessages,
  createAssistantMessageEventStream,
  getCurrentSystemPrompt,
  getCurrentTools,
  type Api,
  type AssistantMessage,
  type AssistantMessageEventStream,
  type Context,
  type ImageContent,
  type Message,
  type Model,
  type SimpleStreamOptions,
  type StopReason,
  type TextContent,
  type ThinkingContent,
  type Tool,
  type ToolResultMessage,
} from "@earendil-works/pi-ai";
import type { ExtensionAPI, ExtensionCommandContext } from "@earendil-works/pi-coding-agent";

// =============================================================================
// Constants & Types
// =============================================================================

type Json = Record<string, any>;

export type ProviderModelConfig = {
  id: string;
  name?: string;
  api?: string;
  reasoning?: boolean;
  input?: ("text" | "image")[];
  cost?: {
    input?: number;
    output?: number;
    cacheRead?: number;
    cacheWrite?: number;
  };
  contextWindow?: number;
  maxTokens?: number;
  compat?: Record<string, any>;
  thinkingLevelMap?: Record<string, string | null>;
};

export type DiagnosticsSnapshot = {
  status?: number;
  endpoint?: string;
  modelId?: string;
  protocol?: "claude-code" | "codex-responses";
  toolsInjected?: string[];
  systemPromptLength?: number;
  error?: string;
  requestId?: string;
  latencyMs?: number;
  retryAttempt?: number;
};

export const API_CLAUDE_ID = "anyrouter-claude" as Api;
export const API_CODEX_ID = "anyrouter-codex" as Api;
export const DEFAULT_BASE_URL = "https://anyrouter.top";

export const CLAUDE_CODE_VERSION = "2.1.322";
export const CLAUDE_CODE_VERSION_BUILD = "2.1.322.b94";
export const STAINLESS_PACKAGE_VERSION = "0.94.0";
export const STAINLESS_OS = "Linux";
export const STAINLESS_ARCH = "x64";
export const STAINLESS_RUNTIME = "node";
export const STAINLESS_RUNTIME_VERSION = "v26.3.0";
export const ANTHROPIC_BETA =
  "claude-code-20250219,context-1m-2025-08-07,interleaved-thinking-2025-05-14,thinking-token-count-2026-05-13,context-management-2025-06-27,prompt-caching-scope-2026-01-05,mid-conversation-system-2026-04-07,advisor-tool-2026-03-01,effort-2025-11-24";

export const CODEX_VERSION = "0.167.0";
export const DEFAULT_MAX_RETRIES = 3;

// =============================================================================
// Diagnostics
// =============================================================================

const diagnosticsStorage = new AsyncLocalStorage<DiagnosticsSnapshot>();
let lastDiagnosticsSnapshot: DiagnosticsSnapshot = {};

function currentDiagnosticsBag(): DiagnosticsSnapshot {
  return diagnosticsStorage.getStore() ?? lastDiagnosticsSnapshot;
}

async function runWithDiagnostics<T>(fn: () => Promise<T>): Promise<T> {
  const bag: DiagnosticsSnapshot = {};
  return diagnosticsStorage.run(bag, async () => {
    try {
      return await fn();
    } finally {
      lastDiagnosticsSnapshot = { ...bag };
    }
  });
}

function getLastDiagnostics(): Readonly<DiagnosticsSnapshot> {
  return lastDiagnosticsSnapshot;
}

function setDiagnosticsField<K extends keyof DiagnosticsSnapshot>(
  key: K,
  value: DiagnosticsSnapshot[K],
): void {
  currentDiagnosticsBag()[key] = value;
}

function recordDiagnosticsError(error: unknown): void {
  const msg = error instanceof Error ? error.message : String(error);
  setDiagnosticsField("error", redactSecrets(msg).slice(0, 1000));
}

// =============================================================================
// Utilities (Text, Naming, Security, SSE)
// =============================================================================

function sanitizeText(text: string): string {
  if (typeof text !== "string") return "";
  return text.replace(/[\uD800-\uDFFF]/g, "\uFFFD");
}

function redactSecrets(text: string): string {
  if (typeof text !== "string") return "";
  return text
    .replace(/sk-[a-zA-Z0-9_-]{20,}/g, "sk-***")
    .replace(/Bearer\s+[a-zA-Z0-9._-]+/gi, "Bearer ***");
}

function extractRequestId(parsed: any, headers?: Headers): string | undefined {
  return (
    parsed?.error?.message?.match(/request id:\s*([^)]+)/i)?.[1] ||
    headers?.get("x-oneapi-request-id") ||
    headers?.get("x-request-id") ||
    undefined
  );
}

function tryParseJson(text: string): any {
  try {
    return text ? JSON.parse(text) : {};
  } catch {
    return undefined;
  }
}

const NAME_MAP: Record<string, string> = {
  read: "Read",
  write: "Write",
  edit: "Edit",
  bash: "Bash",
  grep: "Grep",
  find: "Glob",
  glob: "Glob",
  ls: "LS",
  todowrite: "TodoWrite",
  webfetch: "WebFetch",
  websearch: "WebSearch",
  google_search: "Google_Search",
};

const REVERSE_NAME_MAP: Record<string, string> = {
  read: "read",
  write: "write",
  edit: "edit",
  bash: "bash",
  grep: "grep",
  glob: "find",
  ls: "ls",
  todowrite: "todowrite",
  webfetch: "webfetch",
  websearch: "websearch",
  google_search: "google_search",
};

export function toClaudeCodeName(name?: string | null): string {
  if (!name || typeof name !== "string") return name ?? "";
  const mapped = NAME_MAP[name.toLowerCase()];
  if (mapped) return mapped;
  return name.charAt(0).toUpperCase() + name.slice(1);
}

export function fromClaudeCodeName(name?: string | null): string {
  if (!name || typeof name !== "string") return name ?? "";
  const mapped = REVERSE_NAME_MAP[name.toLowerCase()];
  if (mapped) return mapped;
  return name.charAt(0).toLowerCase() + name.slice(1);
}

export function toClaudeCodeRequestModel(modelId: string): string {
  return modelId.replace(/\[1m\]$/i, "");
}

type SseEvent = {
  event: string;
  data: string;
};

function parseSseEvent(chunk: string): SseEvent {
  let event = "message";
  const data: string[] = [];
  for (const line of chunk.split(/\r?\n/)) {
    if (!line || line.startsWith(":")) continue;
    if (line.startsWith("event:")) {
      event = line.slice(6).trim();
      continue;
    }
    if (line.startsWith("data:")) {
      data.push(line.slice(5).trimStart());
    }
  }
  return { event, data: data.join("\n") };
}

function nextSseChunk(buffer: string): { chunk: string; rest: string } | undefined {
  const unix = buffer.indexOf("\n\n");
  const dos = buffer.indexOf("\r\n\r\n");
  if (unix === -1 && dos === -1) return undefined;
  if (dos !== -1 && (unix === -1 || dos < unix)) {
    return { chunk: buffer.slice(0, dos), rest: buffer.slice(dos + 4) };
  }
  return { chunk: buffer.slice(0, unix), rest: buffer.slice(unix + 2) };
}

async function* iterateSseEvents(stream: ReadableStream<Uint8Array>): AsyncGenerator<SseEvent> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { value, done } = await reader.read();
    buffer += decoder.decode(value || new Uint8Array(), { stream: !done });

    let parsedChunk = nextSseChunk(buffer);
    while (parsedChunk) {
      buffer = parsedChunk.rest;
      const event = parseSseEvent(parsedChunk.chunk);
      if (event.data) yield event;
      parsedChunk = nextSseChunk(buffer);
    }

    if (done) break;
  }

  const tail = buffer.trim();
  if (tail) {
    const event = parseSseEvent(tail);
    if (event.data) yield event;
  }
}

// =============================================================================
// Proxy & Retry HTTP Client
// =============================================================================

const PROXY_AGENTS = new Map<string, ProxyAgent>();

function hostMatchesNoProxy(hostname: string, pattern: string): boolean {
  const item = pattern.trim().toLowerCase();
  if (!item) return false;
  if (item === "*") return true;
  const host = hostname.toLowerCase();
  if (item.startsWith(".")) return host === item.slice(1) || host.endsWith(item);
  return host === item || host.endsWith(`.${item}`);
}

function getProxyUrl(url: string): string | undefined {
  const parsed = new URL(url);
  const noProxy = process.env.NO_PROXY || process.env.no_proxy || "";
  if (noProxy.split(",").some((item) => hostMatchesNoProxy(parsed.hostname, item))) {
    return undefined;
  }
  if (parsed.protocol === "https:") {
    return (
      process.env.HTTPS_PROXY ||
      process.env.https_proxy ||
      process.env.HTTP_PROXY ||
      process.env.http_proxy
    );
  }
  return process.env.HTTP_PROXY || process.env.http_proxy;
}

function getProxyAgent(proxyUrl: string): ProxyAgent {
  let agent = PROXY_AGENTS.get(proxyUrl);
  if (!agent) {
    agent = new ProxyAgent(proxyUrl);
    PROXY_AGENTS.set(proxyUrl, agent);
  }
  return agent;
}

async function fetchWithProxy(url: string, init: RequestInit): Promise<Response> {
  const proxyUrl = getProxyUrl(url);
  if (!proxyUrl) {
    return fetch(url, init);
  }
  return undiciFetch(url, {
    ...init,
    dispatcher: getProxyAgent(proxyUrl),
  } as any) as unknown as Promise<Response>;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function isRetryableStatus(status: number): boolean {
  return [408, 409, 429, 500, 502, 503, 504, 520, 522, 524].includes(status);
}

function parseRetryAfterMs(value: string | null): number | undefined {
  if (!value) return undefined;
  const seconds = Number(value);
  if (Number.isFinite(seconds) && seconds >= 0) return seconds * 1000;
  const at = Date.parse(value);
  if (Number.isFinite(at)) {
    const delta = at - Date.now();
    return delta > 0 ? delta : 0;
  }
  return undefined;
}

function getRetryDelayMs(attempt: number, retryAfterMs?: number): number {
  if (typeof retryAfterMs === "number") {
    return Math.max(0, Math.min(retryAfterMs, 30_000));
  }
  const base = Math.min(1000 * 2 ** attempt, 15_000);
  const jitter = Math.floor(Math.random() * 250);
  return base + jitter;
}

interface SseRequestOptions {
  url: string;
  headers: Record<string, string>;
  body: string;
  signal?: AbortSignal;
  onResponse?: (response: Response) => void;
  onRetry?: (status: number, attempt: number, maxRetries: number) => void;
}

async function postSseWithRetry(options: SseRequestOptions): Promise<Response> {
  const maxRetries = DEFAULT_MAX_RETRIES;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    let response: Response;
    try {
      response = await fetchWithProxy(options.url, {
        method: "POST",
        signal: options.signal,
        headers: options.headers,
        body: options.body,
      });
    } catch (error) {
      if (attempt < maxRetries && !options.signal?.aborted) {
        await delay(getRetryDelayMs(attempt));
        continue;
      }
      throw error;
    }

    setDiagnosticsField("status", response.status);
    setDiagnosticsField("endpoint", options.url);
    setDiagnosticsField("retryAttempt", attempt);

    const contentType = response.headers.get("content-type") || "";
    if (
      response.ok &&
      (contentType.includes("text/event-stream") || contentType.includes("application/json"))
    ) {
      const reqId = response.headers.get("x-oneapi-request-id") || undefined;
      setDiagnosticsField("requestId", reqId);
      if (options.onResponse) {
        try {
          options.onResponse(response);
        } catch {
          // Best-effort hook
        }
      }
      return response;
    }

    const raw = await response.text();
    const parsed = tryParseJson(raw) || { raw };
    const requestId = extractRequestId(parsed, response.headers);
    setDiagnosticsField("requestId", requestId);

    if (!response.ok && attempt < maxRetries && isRetryableStatus(response.status)) {
      if (options.onRetry) {
        options.onRetry(response.status, attempt + 1, maxRetries);
      }
      await delay(
        getRetryDelayMs(attempt, parseRetryAfterMs(response.headers.get("retry-after"))),
      );
      continue;
    }

    if (response.ok) {
      throw new Error(`Stream response was not SSE (content-type=${contentType || "<missing>"})`);
    }
    throw new Error(raw || `HTTP ${response.status}`);
  }

  throw new Error("HTTP request failed after retries");
}

// =============================================================================
// Configuration & Credential Resolution
// =============================================================================

function getAgentDir(): string {
  return process.env.PI_CODING_AGENT_DIR || join(homedir(), ".pi", "agent");
}

function readJsonSafe(filePath: string): Json | undefined {
  try {
    if (!existsSync(filePath)) return undefined;
    return JSON.parse(readFileSync(filePath, "utf8"));
  } catch {
    return undefined;
  }
}

export function resolveEnvOrLiteral(value?: string): string {
  if (!value) return "";
  const trimmed = value.trim();
  if (!trimmed) return "";
  if (trimmed.startsWith("!")) {
    throw new Error("Shell command apiKey values ('!...') are disabled for security.");
  }
  const bracedMatch = trimmed.match(/^\$\{([A-Za-z_][A-Za-z0-9_]*)\}$/);
  if (bracedMatch) {
    return process.env[bracedMatch[1]] || "";
  }
  if (trimmed.startsWith("$") && !trimmed.startsWith("$$")) {
    return process.env[trimmed.slice(1)] || "";
  }
  if (trimmed === "ANYROUTER_API_KEY") {
    return process.env.ANYROUTER_API_KEY || "";
  }
  return process.env[trimmed] || trimmed;
}

function extractAuthEntryKey(entry: any): string {
  if (!entry) return "";
  if (typeof entry === "string") return resolveEnvOrLiteral(entry);
  if (entry.type === "api_key" && typeof entry.key === "string") {
    return resolveEnvOrLiteral(entry.key);
  }
  if (typeof entry.apiKey === "string") {
    return resolveEnvOrLiteral(entry.apiKey);
  }
  if (typeof entry.access === "string") {
    return entry.access;
  }
  return "";
}

export function resolveAnyRouterApi(providerCfg: any): Api | undefined {
  const api = String(providerCfg?.api || "").trim();
  if (api === API_CLAUDE_ID) return API_CLAUDE_ID;
  if (api === API_CODEX_ID) return API_CODEX_ID;
  return undefined;
}

export function buildProviderModels(providerCfg: any): ProviderModelConfig[] {
  if (!Array.isArray(providerCfg?.models) || providerCfg.models.length === 0) {
    return [];
  }

  const baseModels: ProviderModelConfig[] = [];
  for (const userModel of providerCfg.models) {
    if (userModel?.id) {
      baseModels.push({
        ...userModel,
        id: String(userModel.id),
        cost: { ...(userModel.cost || {}) },
      });
    }
  }

  if (providerCfg?.modelOverrides && typeof providerCfg.modelOverrides === "object") {
    for (let i = 0; i < baseModels.length; i++) {
      const m = baseModels[i];
      const override = providerCfg.modelOverrides[m.id];
      if (override && typeof override === "object") {
        baseModels[i] = {
          ...m,
          ...override,
          cost: {
            ...(m.cost || {}),
            ...(override.cost || {}),
          },
        };
      }
    }
  }

  return baseModels;
}

export function resolveProviderCredentials(
  model: Model<Api>,
  options?: SimpleStreamOptions,
): { baseUrl: string; apiKey: string } {
  const agentDir = getAgentDir();
  const modelsJson = readJsonSafe(join(agentDir, "models.json"));
  const authJson = readJsonSafe(join(agentDir, "auth.json"));

  const providerKey = model.provider || "anyrouter-claude";
  const providerCfg = modelsJson?.providers?.[providerKey];

  const baseUrl = (
    model.baseUrl ||
    process.env.ANYROUTER_BASE_URL ||
    providerCfg?.baseUrl ||
    DEFAULT_BASE_URL
  )
    .trim()
    .replace(/\/+$/, "");

  const rawKey =
    options?.apiKey ||
    extractAuthEntryKey(authJson?.[providerKey]) ||
    resolveEnvOrLiteral(providerCfg?.apiKey) ||
    process.env.ANYROUTER_API_KEY ||
    "";

  const apiKey = resolveEnvOrLiteral(rawKey);
  if (!apiKey) {
    throw new Error(
      `Missing AnyRouter API key for provider "${providerKey}". Configure it in AIToolPlus, ~/.pi/agent/auth.json, ~/.pi/agent/models.json, or ANYROUTER_API_KEY.`,
    );
  }

  return { baseUrl, apiKey };
}

// =============================================================================
// Context Helpers
// =============================================================================

function resolveCurrentTools(context: Context): Tool[] {
  try {
    const transcript = collapseSystemMessages(context);
    const tools = getCurrentTools(transcript.messages);
    if (tools && tools.length > 0) return tools;
  } catch {
    // Fallback for older context shapes
  }
  return (context as any).tools ?? [];
}

function resolveCurrentSystemPrompt(context: Context): string {
  try {
    const transcript = collapseSystemMessages(context);
    const prompt = getCurrentSystemPrompt(transcript.messages);
    if (prompt) return prompt;
  } catch {
    // Fallback for older context shapes
  }
  return (context as any).systemPrompt ?? "";
}

// =============================================================================
// Claude Code Protocol Implementation
// =============================================================================

function loadClaudeDeviceId(): string {
  const configured = process.env.PI_ANYROUTER_CC_DEVICE_ID?.trim();
  if (configured && /^[0-9a-f]{64}$/i.test(configured)) return configured;

  try {
    const claudeConfigPath = join(homedir(), ".claude.json");
    if (existsSync(claudeConfigPath)) {
      const parsed = JSON.parse(readFileSync(claudeConfigPath, "utf8"));
      if (typeof parsed.userID === "string" && /^[0-9a-f]{64}$/i.test(parsed.userID)) {
        return parsed.userID;
      }
    }
  } catch {
    // Ignore if ~/.claude.json is not present
  }

  return randomBytes(32).toString("hex");
}

const CLAUDE_DEVICE_ID = loadClaudeDeviceId();

const REASONING_EFFORT_MAP: Record<string, string> = {
  minimal: "low",
  low: "low",
  medium: "medium",
  high: "high",
  xhigh: "xhigh",
};

export function mapReasoningEffort(level?: SimpleStreamOptions["reasoning"]): string {
  return (level && REASONING_EFFORT_MAP[level]) || "medium";
}

const STOP_REASON_MAP: Record<string, StopReason> = {
  end_turn: "stop",
  pause_turn: "stop",
  stop_sequence: "stop",
  max_tokens: "length",
  tool_use: "toolUse",
};

export function mapStopReason(reason: string): StopReason {
  return STOP_REASON_MAP[reason] || "error";
}

export function getClaudeCodeHeaders(
  apiKey: string,
  retryCount = 0,
  sessionId: string,
): Record<string, string> {
  return {
    "content-type": "application/json",
    accept: "application/json",
    authorization: `Bearer ${apiKey}`,
    "x-api-key": apiKey,
    "anthropic-version": "2023-06-01",
    "anthropic-dangerous-direct-browser-access": "true",
    "anthropic-beta": ANTHROPIC_BETA,
    "user-agent": `claude-cli/${CLAUDE_CODE_VERSION} (external, sdk-cli)`,
    "x-app": "cli",
    "x-claude-code-session-id": sessionId,
    "x-stainless-retry-count": String(retryCount),
    "x-stainless-timeout": "600",
    "x-stainless-lang": "js",
    "x-stainless-package-version": STAINLESS_PACKAGE_VERSION,
    "x-stainless-os": STAINLESS_OS,
    "x-stainless-arch": STAINLESS_ARCH,
    "x-stainless-runtime": STAINLESS_RUNTIME,
    "x-stainless-runtime-version": STAINLESS_RUNTIME_VERSION,
  };
}

export function createClaudeCodeMetadata(sessionId: string): { user_id: string } {
  return {
    user_id: JSON.stringify({
      device_id: CLAUDE_DEVICE_ID,
      account_uuid: "",
      session_id: sessionId,
    }),
  };
}

export function createClaudeCodeSystem(systemPrompt: string): Array<{
  type: string;
  text: string;
  cache_control?: { type: string };
}> {
  return [
    {
      type: "text",
      text: `x-anthropic-billing-header: cc_version=${CLAUDE_CODE_VERSION_BUILD}; cc_entrypoint=sdk-cli;`,
    },
    {
      type: "text",
      text: "You are a Claude agent, built on Anthropic's Claude Agent SDK.",
      cache_control: { type: "ephemeral" },
    },
    {
      type: "text",
      text: sanitizeText(systemPrompt),
      cache_control: { type: "ephemeral" },
    },
  ];
}

function convertContentBlocks(content: (TextContent | ImageContent)[]): any {
  const hasImages = content.some((c) => c.type === "image");
  if (!hasImages) {
    return sanitizeText(content.map((c) => (c as TextContent).text).join("\n"));
  }

  const blocks = content.map((block) => {
    if (block.type === "text") return { type: "text", text: sanitizeText(block.text) };
    return {
      type: "image",
      source: {
        type: "base64",
        media_type: block.mimeType,
        data: block.data,
      },
    };
  });
  if (!blocks.some((b) => b.type === "text")) {
    blocks.unshift({ type: "text", text: "(see attached image)" });
  }
  return blocks;
}

function convertUserMessage(msg: Message): any | undefined {
  if (typeof msg.content === "string") {
    const text = sanitizeText(msg.content);
    return text.trim() ? { role: "user", content: [{ type: "text", text }] } : undefined;
  }
  const blocks = msg.content.map((item) =>
    item.type === "text"
      ? { type: "text", text: sanitizeText(item.text) }
      : { type: "image", source: { type: "base64", media_type: item.mimeType, data: item.data } },
  );
  return blocks.length > 0 ? { role: "user", content: blocks } : undefined;
}

function convertAssistantBlock(block: any): any | undefined {
  if (block.type === "text" && block.text.trim()) {
    return { type: "text", text: sanitizeText(block.text) };
  }
  if (block.type === "thinking" && block.thinking.trim()) {
    return (block as ThinkingContent).thinkingSignature
      ? {
          type: "thinking",
          thinking: sanitizeText(block.thinking),
          signature: (block as ThinkingContent).thinkingSignature,
        }
      : { type: "text", text: sanitizeText(block.thinking) };
  }
  if (block.type === "toolCall") {
    return {
      type: "tool_use",
      id: block.id,
      name: toClaudeCodeName(block.name),
      input: block.arguments,
    };
  }
  return undefined;
}

export function convertMessages(messages: Message[]): any[] {
  const params: any[] = [];
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];

    if (msg.role === "user") {
      const converted = convertUserMessage(msg);
      if (converted) params.push(converted);
      continue;
    }

    if (msg.role === "assistant") {
      const blocks = msg.content.map(convertAssistantBlock).filter(Boolean);
      if (blocks.length > 0) params.push({ role: "assistant", content: blocks });
      continue;
    }

    if (msg.role === "toolResult") {
      const toolResults: any[] = [];
      const pushToolResult = (toolMsg: ToolResultMessage) => {
        toolResults.push({
          type: "tool_result",
          tool_use_id: toolMsg.toolCallId,
          content: convertContentBlocks(toolMsg.content),
          is_error: toolMsg.isError,
        });
      };
      pushToolResult(msg as ToolResultMessage);
      let j = i + 1;
      while (j < messages.length && messages[j].role === "toolResult") {
        pushToolResult(messages[j] as ToolResultMessage);
        j++;
      }
      i = j - 1;
      params.push({ role: "user", content: toolResults });
    }
  }

  if (params.length > 0) {
    const last = params[params.length - 1];
    if (last.role === "user" && Array.isArray(last.content) && last.content.length > 0) {
      last.content[last.content.length - 1].cache_control = { type: "ephemeral" };
    }
  }
  return params;
}

export function convertTools(tools: Tool[]): any[] {
  return tools.map((tool) => ({
    name: toClaudeCodeName(tool.name),
    description: tool.description,
    input_schema: {
      type: "object",
      properties: (tool.parameters as any).properties || {},
      required: (tool.parameters as any).required || [],
    },
  }));
}

function updateUsageFromAnthropic(
  output: AssistantMessage,
  usage: any,
  model: Model<Api>,
): void {
  if (usage?.input_tokens != null) output.usage.input = usage.input_tokens;
  if (usage?.output_tokens != null) output.usage.output = usage.output_tokens;
  if (usage?.cache_read_input_tokens != null) output.usage.cacheRead = usage.cache_read_input_tokens;
  if (usage?.cache_creation_input_tokens != null) {
    output.usage.cacheWrite = usage.cache_creation_input_tokens;
  }
  output.usage.totalTokens =
    output.usage.input + output.usage.output + output.usage.cacheRead + output.usage.cacheWrite;
  calculateCost(model, output.usage);
}

function handleClaudeBlockStart(
  payload: any,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  blockIndexByEventIndex: Map<number, number>,
): void {
  const block = payload.content_block;
  if (!block?.type) return;

  const contentIndex = output.content.length;
  blockIndexByEventIndex.set(payload.index, contentIndex);

  if (block.type === "text") {
    output.content.push({ type: "text", text: "", eventIndex: payload.index } as any);
    stream.push({ type: "text_start", contentIndex, partial: output });
    return;
  }

  if (block.type === "thinking" || block.type === "redacted_thinking") {
    const isRedacted = block.type === "redacted_thinking";
    output.content.push({
      type: "thinking",
      thinking: isRedacted ? "[Reasoning redacted]" : "",
      thinkingSignature: isRedacted ? String(block.data || "") : "",
      redacted: isRedacted || undefined,
      eventIndex: payload.index,
    } as any);
    stream.push({ type: "thinking_start", contentIndex, partial: output });
    return;
  }

  if (block.type === "tool_use") {
    const toolCall = {
      type: "toolCall" as const,
      id: block.id,
      name: fromClaudeCodeName(block.name),
      arguments: (block.input as Json) || {},
      partialJson: "",
      eventIndex: payload.index,
    };
    output.content.push(toolCall as any);
    stream.push({ type: "toolcall_start", contentIndex, partial: output });
  }
}

function handleClaudeBlockDelta(
  payload: any,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  blockIndexByEventIndex: Map<number, number>,
): void {
  const contentIndex = blockIndexByEventIndex.get(payload.index);
  if (contentIndex == null) return;
  const block = output.content[contentIndex] as any;
  if (!block || !payload.delta?.type) return;

  const deltaType = payload.delta.type;

  if (deltaType === "text_delta" && block.type === "text") {
    const delta = String(payload.delta.text || "");
    block.text += delta;
    stream.push({ type: "text_delta", contentIndex, delta, partial: output });
    return;
  }

  if (deltaType === "thinking_delta" && block.type === "thinking") {
    const delta = String(payload.delta.thinking || "");
    block.thinking += delta;
    stream.push({ type: "thinking_delta", contentIndex, delta, partial: output });
    return;
  }

  if (deltaType === "input_json_delta" && block.type === "toolCall") {
    const delta = String(payload.delta.partial_json || "");
    block.partialJson += delta;
    try {
      block.arguments = JSON.parse(block.partialJson);
    } catch {
      // Partial JSON is expected during streaming
    }
    stream.push({ type: "toolcall_delta", contentIndex, delta, partial: output });
    return;
  }

  if (deltaType === "signature_delta" && block.type === "thinking") {
    block.thinkingSignature = `${block.thinkingSignature || ""}${String(payload.delta.signature || "")}`;
  }
}

function handleClaudeBlockStop(
  payload: any,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  blockIndexByEventIndex: Map<number, number>,
): void {
  const contentIndex = blockIndexByEventIndex.get(payload.index);
  if (contentIndex == null) return;
  const block = output.content[contentIndex] as any;
  if (!block) return;

  delete block.eventIndex;
  blockIndexByEventIndex.delete(payload.index);

  if (block.type === "text") {
    stream.push({ type: "text_end", contentIndex, content: block.text, partial: output });
    return;
  }

  if (block.type === "thinking") {
    stream.push({ type: "thinking_end", contentIndex, content: block.thinking, partial: output });
    return;
  }

  if (block.type === "toolCall") {
    if (block.partialJson) {
      try {
        block.arguments = JSON.parse(block.partialJson);
      } catch {
        block.arguments = block.arguments || {};
      }
    }
    delete block.partialJson;
    stream.push({ type: "toolcall_end", contentIndex, toolCall: block, partial: output });
  }
}

function applySsePayloadEvent(
  payload: any,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  model: Model<Api>,
  blockIndexByEventIndex: Map<number, number>,
): void {
  const type = payload?.type;
  if (!type || type === "ping" || type === "message_stop") return;

  if (type === "error") {
    const errorText =
      payload?.error?.message || payload?.error || payload?.message || JSON.stringify(payload);
    throw new Error(String(errorText));
  }

  if (type === "message_start") {
    output.responseId = payload.message?.id || output.responseId;
    updateUsageFromAnthropic(output, payload.message?.usage || {}, model);
    return;
  }

  if (type === "content_block_start") {
    handleClaudeBlockStart(payload, output, stream, blockIndexByEventIndex);
    return;
  }

  if (type === "content_block_delta") {
    handleClaudeBlockDelta(payload, output, stream, blockIndexByEventIndex);
    return;
  }

  if (type === "content_block_stop") {
    handleClaudeBlockStop(payload, output, stream, blockIndexByEventIndex);
    return;
  }

  if (type === "message_delta") {
    if (payload.delta?.stop_reason) output.stopReason = mapStopReason(payload.delta.stop_reason);
    updateUsageFromAnthropic(output, payload.usage || {}, model);
  }
}

function applyJsonResponseToOutput(
  response: any,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  model: Model<Api>,
): void {
  output.responseId = response?.id || output.responseId;
  updateUsageFromAnthropic(output, response?.usage || {}, model);
  output.stopReason = mapStopReason(response?.stop_reason || "end_turn");

  const content = Array.isArray(response?.content) ? response.content : [];
  for (const block of content) {
    if (block?.type === "text") {
      output.content.push({ type: "text", text: "" });
      const contentIndex = output.content.length - 1;
      stream.push({ type: "text_start", contentIndex, partial: output });
      const text = String(block.text || "");
      (output.content[contentIndex] as any).text = text;
      if (text) stream.push({ type: "text_delta", contentIndex, delta: text, partial: output });
      stream.push({ type: "text_end", contentIndex, content: text, partial: output });
    } else if (block?.type === "thinking") {
      output.content.push({
        type: "thinking",
        thinking: String(block.thinking || ""),
        thinkingSignature: block.signature || "",
      } as any);
      const contentIndex = output.content.length - 1;
      stream.push({ type: "thinking_start", contentIndex, partial: output });
      if (block.thinking) {
        stream.push({
          type: "thinking_delta",
          contentIndex,
          delta: String(block.thinking),
          partial: output,
        });
      }
      stream.push({
        type: "thinking_end",
        contentIndex,
        content: String(block.thinking || ""),
        partial: output,
      });
    } else if (block?.type === "tool_use") {
      const toolCall = {
        type: "toolCall" as const,
        id: block.id,
        name: fromClaudeCodeName(block.name),
        arguments: block.input || {},
      };
      output.content.push(toolCall as any);
      const contentIndex = output.content.length - 1;
      stream.push({ type: "toolcall_start", contentIndex, partial: output });
      stream.push({
        type: "toolcall_delta",
        contentIndex,
        delta: JSON.stringify(toolCall.arguments),
        partial: output,
      });
      stream.push({ type: "toolcall_end", contentIndex, toolCall, partial: output });
    }
  }
}

async function tryStreamAnyRouterCc(
  url: string,
  body: Json,
  apiKey: string,
  model: Model<Api>,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  sessionId: string,
  options?: SimpleStreamOptions,
): Promise<void> {
  const headers = getClaudeCodeHeaders(apiKey, 0, sessionId);
  const response = await postSseWithRetry({
    url,
    headers,
    body: JSON.stringify({ ...body, stream: true }),
    signal: options?.signal,
    onResponse: options?.onResponse,
    onRetry: (status, attempt, maxRetries) => {
      const contentIndex = output.content.length;
      const retryText = `⏳ ${status} — retrying (${attempt}/${maxRetries})…`;
      output.content.push({ type: "text", text: retryText } as any);
      stream.push({ type: "text_start", contentIndex, partial: output });
      stream.push({ type: "text_delta", contentIndex, delta: retryText, partial: output });
      stream.push({ type: "text_end", contentIndex, content: retryText, partial: output });
    },
  });

  const contentType = response.headers.get("content-type") || "";
  if (contentType.includes("application/json")) {
    const jsonPayload = await response.json();
    applyJsonResponseToOutput(jsonPayload, output, stream, model);
    return;
  }

  if (!response.body) throw new Error("stream response body missing");

  const blockIndexByEventIndex = new Map<number, number>();

  for await (const event of iterateSseEvents(response.body)) {
    if (event.data === "[DONE]") continue;
    const payload = tryParseJson(event.data);
    if (!payload) {
      throw new Error(`invalid SSE payload: ${event.data.slice(0, 200)}`);
    }
    if (options?.onProviderStreamEvent) {
      await options.onProviderStreamEvent(payload, model);
    }
    applySsePayloadEvent(payload, output, stream, model, blockIndexByEventIndex);
  }
}

export function buildClaudeCodeRequestBody(
  model: Model<Api>,
  context: Context,
  options: SimpleStreamOptions | undefined,
  sessionId: string,
): { requestBody: Json; tools: Tool[]; systemPrompt: string } {
  const tools = resolveCurrentTools(context);
  const systemPrompt = resolveCurrentSystemPrompt(context);

  setDiagnosticsField("toolsInjected", tools.map((t) => t.name));
  setDiagnosticsField("systemPromptLength", systemPrompt.length);

  const requestBody: Json = {
    model: toClaudeCodeRequestModel(model.id),
    messages: convertMessages(context.messages),
    max_tokens: options?.maxTokens || model.maxTokens || 32000,
    stream: false,
    metadata: createClaudeCodeMetadata(sessionId),
    system: createClaudeCodeSystem(systemPrompt),
    context_management: {
      edits: [{ type: "clear_thinking_20251015", keep: "all" }],
    },
  };

  if (tools.length > 0) {
    requestBody.tools = convertTools(tools);
  }

  if (options?.reasoning && model.reasoning) {
    requestBody.thinking = { type: "adaptive", display: "omitted" };
    requestBody.output_config = { effort: mapReasoningEffort(options.reasoning) };
  }

  return { requestBody, tools, systemPrompt };
}

// =============================================================================
// Codex Responses Lite Protocol Implementation
// =============================================================================

const CODEX_INSTALLATION_ID = randomUUID();

export function getCodexResponsesUrl(baseUrl: string): string {
  const normalized = baseUrl.replace(/\/+$/, "");
  if (normalized.endsWith("/responses")) return normalized;
  if (normalized.endsWith("/v1")) return `${normalized}/responses`;
  return `${normalized}/v1/responses`;
}

function convertCodexUserMessage(msg: any): any | undefined {
  if (typeof msg.content === "string") {
    const text = sanitizeText(msg.content);
    return text.trim()
      ? { type: "message", role: "user", content: [{ type: "input_text", text }] }
      : undefined;
  }
  const content = msg.content.map((item: any) =>
    item.type === "text"
      ? { type: "input_text", text: sanitizeText(item.text) }
      : {
          type: "input_image",
          detail: "auto",
          image_url: `data:${item.mimeType};base64,${item.data}`,
        },
  );
  return content.length ? { type: "message", role: "user", content } : undefined;
}

function convertCodexAssistantBlock(block: any): any | undefined {
  if (block.type === "thinking" && block.thinkingSignature) {
    return tryParseJson(block.thinkingSignature);
  }
  if (block.type === "text" && block.text.trim()) {
    return {
      type: "message",
      role: "assistant",
      status: "completed",
      content: [{ type: "output_text", text: sanitizeText(block.text), annotations: [] }],
    };
  }
  if (block.type === "toolCall") {
    const [callId, itemId] = block.id.split("|");
    return {
      type: "function_call",
      ...(itemId ? { id: itemId } : {}),
      call_id: callId,
      name: block.name,
      arguments: JSON.stringify(block.arguments),
    };
  }
  return undefined;
}

function convertCodexToolResult(msg: ToolResultMessage): any {
  const text = msg.content
    .filter((item) => item.type === "text")
    .map((item) => (item as TextContent).text)
    .join("\n");
  const images = msg.content.filter((item) => item.type === "image") as ImageContent[];
  const output = images.length
    ? [
        ...(text ? [{ type: "input_text", text: sanitizeText(text) }] : []),
        ...images.map((image) => ({
          type: "input_image",
          detail: "auto",
          image_url: `data:${image.mimeType};base64,${image.data}`,
        })),
      ]
    : sanitizeText(text || (images.length ? "(see attached image)" : "(no tool output)"));

  return {
    type: "function_call_output",
    call_id: msg.toolCallId.split("|")[0],
    output,
  };
}

function convertCodexMessages(context: Context, systemPrompt?: string): any[] {
  const input: any[] = [];
  if (systemPrompt && systemPrompt.trim()) {
    input.push({
      type: "message",
      role: "developer",
      content: [{ type: "input_text", text: sanitizeText(systemPrompt) }],
    });
  }

  for (const msg of context.messages) {
    if (msg.role === "user") {
      const converted = convertCodexUserMessage(msg);
      if (converted) input.push(converted);
      continue;
    }

    if (msg.role === "assistant") {
      for (const block of msg.content) {
        const converted = convertCodexAssistantBlock(block);
        if (converted) input.push(converted);
      }
      continue;
    }

    if (msg.role === "toolResult") {
      input.push(convertCodexToolResult(msg as ToolResultMessage));
    }
  }
  return input;
}

function convertCodexTools(tools: Tool[]): any[] {
  return tools.map((tool) => ({
    type: "function",
    name: tool.name,
    description: tool.description,
    parameters: tool.parameters,
    strict: false,
  }));
}

function createCodexMetadata(sessionId: string, turnId: string) {
  const windowId = `${sessionId}:0`;
  const turnMetadata = JSON.stringify({
    installation_id: CODEX_INSTALLATION_ID,
    session_id: sessionId,
    thread_id: sessionId,
    turn_id: turnId,
    window_id: windowId,
    request_kind: "turn",
    thread_source: "user",
    turn_started_at_unix_ms: Date.now(),
  });
  return {
    windowId,
    turnMetadata,
    clientMetadata: {
      session_id: sessionId,
      thread_id: sessionId,
      turn_id: turnId,
      "x-codex-installation-id": CODEX_INSTALLATION_ID,
      "x-codex-window-id": windowId,
      "x-codex-turn-metadata": turnMetadata,
    },
  };
}

function createCodexHeaders(
  apiKey: string,
  sessionId: string,
  metadata: ReturnType<typeof createCodexMetadata>,
): Record<string, string> {
  return {
    authorization: `Bearer ${apiKey}`,
    accept: "text/event-stream",
    "content-type": "application/json",
    originator: "codex_exec",
    "user-agent": `codex_exec/${CODEX_VERSION} (Linux; x86_64) (codex_exec; ${CODEX_VERSION})`,
    "x-openai-internal-codex-responses-lite": "true",
    "x-codex-beta-features": "remote_compaction_v2",
    "x-codex-window-id": metadata.windowId,
    "x-codex-turn-metadata": metadata.turnMetadata,
    "x-client-request-id": sessionId,
    "session-id": sessionId,
    "thread-id": sessionId,
  };
}

function buildCodexRequestBody(
  model: Model<Api>,
  context: Context,
  options: SimpleStreamOptions | undefined,
  sessionId: string,
  metadata: ReturnType<typeof createCodexMetadata>,
): { body: Json; tools: Tool[]; systemPrompt: string } {
  const tools = resolveCurrentTools(context);
  const systemPrompt = resolveCurrentSystemPrompt(context);

  setDiagnosticsField("toolsInjected", tools.map((t) => t.name));
  setDiagnosticsField("systemPromptLength", systemPrompt.length);

  const body: Json = {
    model: model.id,
    input: convertCodexMessages(context, systemPrompt),
    tool_choice: "auto",
    parallel_tool_calls: false,
    reasoning: {
      effort: mapReasoningEffort(options?.reasoning),
      context: "all_turns",
    },
    store: false,
    stream: true,
    text: { verbosity: "low" },
    max_output_tokens: options?.maxTokens || model.maxTokens,
    include: ["reasoning.encrypted_content"],
    prompt_cache_key: sessionId,
    client_metadata: metadata.clientMetadata,
  };

  if (tools.length > 0) {
    body.tools = convertCodexTools(tools);
  }

  return { body, tools, systemPrompt };
}

function applyCodexUsage(output: AssistantMessage, response: any, model: Model<Api>): void {
  const usage = response?.usage;
  if (!usage) return;
  const cached = usage.input_tokens_details?.cached_tokens || 0;
  const cacheWrite = usage.input_tokens_details?.cache_write_tokens || 0;
  output.usage.input = Math.max(0, (usage.input_tokens || 0) - cached - cacheWrite);
  output.usage.output = usage.output_tokens || 0;
  output.usage.cacheRead = cached;
  output.usage.cacheWrite = cacheWrite;
  output.usage.totalTokens =
    usage.total_tokens || output.usage.input + output.usage.output + cached + cacheWrite;
  calculateCost(model, output.usage);
}

function handleCodexItemAdded(
  payload: any,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  slots: Map<number, any>,
): void {
  const item = payload.item;
  if (!item?.type) return;

  const contentIndex = output.content.length;

  if (item.type === "message") {
    const block = { type: "text" as const, text: "" };
    output.content.push(block as any);
    slots.set(payload.output_index, { type: "text", block, contentIndex });
    stream.push({ type: "text_start", contentIndex, partial: output });
    return;
  }

  if (item.type === "reasoning") {
    const block = { type: "thinking" as const, thinking: "", thinkingSignature: "" };
    output.content.push(block as any);
    slots.set(payload.output_index, { type: "thinking", block, contentIndex });
    stream.push({ type: "thinking_start", contentIndex, partial: output });
    return;
  }

  if (item.type === "function_call") {
    const block = {
      type: "toolCall" as const,
      id: `${item.call_id}|${item.id}`,
      name: item.name,
      arguments: {},
      partialJson: item.arguments || "",
    };
    output.content.push(block as any);
    slots.set(payload.output_index, { type: "toolCall", block, contentIndex });
    stream.push({ type: "toolcall_start", contentIndex, partial: output });
  }
}

function handleCodexDelta(
  type: string,
  payload: any,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  slot: any,
): void {
  if (!slot) return;

  if (type === "response.output_text.delta" && slot.type === "text") {
    const delta = String(payload.delta || "");
    slot.block.text += delta;
    stream.push({ type: "text_delta", contentIndex: slot.contentIndex, delta, partial: output });
    return;
  }

  if (
    (type === "response.reasoning_summary_text.delta" || type === "response.reasoning_text.delta") &&
    slot.type === "thinking"
  ) {
    const delta = String(payload.delta || "");
    slot.block.thinking += delta;
    stream.push({ type: "thinking_delta", contentIndex: slot.contentIndex, delta, partial: output });
    return;
  }

  if (type === "response.function_call_arguments.delta" && slot.type === "toolCall") {
    const delta = String(payload.delta || "");
    slot.block.partialJson += delta;
    const parsed = tryParseJson(slot.block.partialJson);
    if (parsed !== undefined) slot.block.arguments = parsed;
    stream.push({ type: "toolcall_delta", contentIndex: slot.contentIndex, delta, partial: output });
    return;
  }

  if (type === "response.function_call_arguments.done" && slot.type === "toolCall") {
    slot.block.partialJson = String(payload.arguments || slot.block.partialJson);
    slot.block.arguments = tryParseJson(slot.block.partialJson) || {};
  }
}

function handleCodexItemDone(
  payload: any,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  slots: Map<number, any>,
): void {
  const slot = slots.get(payload.output_index);
  slots.delete(payload.output_index);
  if (!slot) return;

  const item = payload.item;

  if (slot.type === "text") {
    slot.block.text =
      item?.content?.map((part: any) => part.text || part.refusal || "").join("") ||
      slot.block.text;
    stream.push({
      type: "text_end",
      contentIndex: slot.contentIndex,
      content: slot.block.text,
      partial: output,
    });
    return;
  }

  if (slot.type === "thinking") {
    slot.block.thinking =
      item?.summary?.map((part: any) => part.text).join("\n\n") ||
      item?.content?.map((part: any) => part.text).join("\n\n") ||
      slot.block.thinking;
    slot.block.thinkingSignature = JSON.stringify(item);
    stream.push({
      type: "thinking_end",
      contentIndex: slot.contentIndex,
      content: slot.block.thinking,
      partial: output,
    });
    return;
  }

  if (slot.type === "toolCall") {
    slot.block.arguments = tryParseJson(item?.arguments || slot.block.partialJson) || {};
    delete slot.block.partialJson;
    stream.push({
      type: "toolcall_end",
      contentIndex: slot.contentIndex,
      toolCall: slot.block,
      partial: output,
    });
  }
}

function applyCodexSsePayload(
  payload: any,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  model: Model<Api>,
  slots: Map<number, any>,
): void {
  const type = payload?.type;
  if (!type || type === "response.in_progress" || type === "response.metadata") return;

  if (type === "error") throw new Error(payload.message || JSON.stringify(payload));
  if (type === "response.failed") {
    throw new Error(payload.response?.error?.message || "Codex response failed");
  }

  if (type === "response.created") {
    output.responseId = payload.response?.id || output.responseId;
    return;
  }

  if (type === "response.output_item.added") {
    handleCodexItemAdded(payload, output, stream, slots);
    return;
  }

  if (type === "response.output_item.done") {
    handleCodexItemDone(payload, output, stream, slots);
    return;
  }

  if (type.endsWith(".delta") || type.endsWith(".done")) {
    const slot = slots.get(payload.output_index);
    handleCodexDelta(type, payload, output, stream, slot);
    return;
  }

  if (type === "response.completed" || type === "response.incomplete") {
    output.responseId = payload.response?.id || output.responseId;
    applyCodexUsage(output, payload.response, model);
    output.stopReason =
      type === "response.incomplete"
        ? "length"
        : output.content.some((block) => block.type === "toolCall")
          ? "toolUse"
          : "stop";
  }
}

async function tryStreamAnyRouterCodex(
  url: string,
  body: Json,
  apiKey: string,
  model: Model<Api>,
  output: AssistantMessage,
  stream: AssistantMessageEventStream,
  sessionId: string,
  metadata: ReturnType<typeof createCodexMetadata>,
  options?: SimpleStreamOptions,
): Promise<void> {
  const headers = createCodexHeaders(apiKey, sessionId, metadata);
  const response = await postSseWithRetry({
    url,
    headers,
    body: JSON.stringify(body),
    signal: options?.signal,
    onResponse: options?.onResponse,
  });

  if (!response.body) throw new Error("Codex stream response body missing");

  const slots = new Map<number, any>();
  let terminal = false;

  for await (const event of iterateSseEvents(response.body)) {
    if (event.data === "[DONE]") continue;
    const payload = tryParseJson(event.data);
    if (!payload) throw new Error(`invalid Codex SSE payload: ${event.data.slice(0, 200)}`);
    if (options?.onProviderStreamEvent) {
      await options.onProviderStreamEvent(payload, model);
    }
    applyCodexSsePayload(payload, output, stream, model, slots);
    if (payload.type === "response.completed" || payload.type === "response.incomplete") {
      terminal = true;
    }
  }

  if (!terminal) throw new Error("Codex stream ended before a terminal response event");
}

// =============================================================================
// Dedicated Stream Entry Points (Claude Code vs Codex Responses)
// =============================================================================

function createEmptyUsage() {
  return {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    totalTokens: 0,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
  };
}

export function streamAnyRouterClaude(
  model: Model<Api>,
  context: Context,
  options?: SimpleStreamOptions,
): AssistantMessageEventStream {
  const stream = createAssistantMessageEventStream();

  (async () => {
    const startTime = Date.now();
    const output: AssistantMessage = {
      role: "assistant",
      content: [],
      api: model.api,
      provider: model.provider,
      model: model.id,
      usage: createEmptyUsage(),
      stopReason: "stop",
      timestamp: startTime,
    };

    await runWithDiagnostics(async () => {
      setDiagnosticsField("modelId", model.id);
      setDiagnosticsField("protocol", "claude-code");

      try {
        const { baseUrl, apiKey } = resolveProviderCredentials(model, options);
        const sessionId = options?.sessionId || randomUUID();
        const url = `${baseUrl}/v1/messages?beta=true`;
        setDiagnosticsField("endpoint", url);

        let { requestBody } = buildClaudeCodeRequestBody(model, context, options, sessionId);

        if (options?.onPayload) {
          const replaced = await options.onPayload(requestBody, model);
          if (replaced !== undefined) requestBody = replaced as Json;
        }

        stream.push({ type: "start", partial: output });

        await tryStreamAnyRouterCc(
          url,
          requestBody,
          apiKey,
          model,
          output,
          stream,
          sessionId,
          options,
        );

        if (options?.signal?.aborted) throw new Error("Request was aborted");
        setDiagnosticsField("latencyMs", Date.now() - startTime);
        stream.push({
          type: "done",
          reason: output.stopReason as "stop" | "length" | "toolUse",
          message: output,
        });
        stream.end();
      } catch (error) {
        setDiagnosticsField("latencyMs", Date.now() - startTime);
        recordDiagnosticsError(error);

        output.stopReason = options?.signal?.aborted ? "aborted" : "error";
        const rawMessage =
          error instanceof Error ? `[anyrouter-claude] ${error.message}` : String(error);
        output.errorMessage = redactSecrets(rawMessage);
        stream.push({ type: "error", reason: output.stopReason, error: output });
        stream.end();
      }
    });
  })();

  return stream;
}

export function streamAnyRouterCodex(
  model: Model<Api>,
  context: Context,
  options?: SimpleStreamOptions,
): AssistantMessageEventStream {
  const stream = createAssistantMessageEventStream();

  (async () => {
    const startTime = Date.now();
    const output: AssistantMessage = {
      role: "assistant",
      content: [],
      api: model.api,
      provider: model.provider,
      model: model.id,
      usage: createEmptyUsage(),
      stopReason: "stop",
      timestamp: startTime,
    };

    await runWithDiagnostics(async () => {
      setDiagnosticsField("modelId", model.id);
      setDiagnosticsField("protocol", "codex-responses");

      try {
        const { baseUrl, apiKey } = resolveProviderCredentials(model, options);
        const sessionId = options?.sessionId || randomUUID();
        const turnId = randomUUID();
        const metadata = createCodexMetadata(sessionId, turnId);
        const url = getCodexResponsesUrl(baseUrl);
        setDiagnosticsField("endpoint", url);

        let { body: requestBody } = buildCodexRequestBody(
          model,
          context,
          options,
          sessionId,
          metadata,
        );

        if (options?.onPayload) {
          const replaced = await options.onPayload(requestBody, model);
          if (replaced !== undefined) requestBody = replaced as Json;
        }

        stream.push({ type: "start", partial: output });

        await tryStreamAnyRouterCodex(
          url,
          requestBody,
          apiKey,
          model,
          output,
          stream,
          sessionId,
          metadata,
          options,
        );

        if (options?.signal?.aborted) throw new Error("Request was aborted");
        setDiagnosticsField("latencyMs", Date.now() - startTime);
        stream.push({
          type: "done",
          reason: output.stopReason as "stop" | "length" | "toolUse",
          message: output,
        });
        stream.end();
      } catch (error) {
        setDiagnosticsField("latencyMs", Date.now() - startTime);
        recordDiagnosticsError(error);

        output.stopReason = options?.signal?.aborted ? "aborted" : "error";
        const rawMessage =
          error instanceof Error ? `[anyrouter-codex] ${error.message}` : String(error);
        output.errorMessage = redactSecrets(rawMessage);
        stream.push({ type: "error", reason: output.stopReason, error: output });
        stream.end();
      }
    });
  })();

  return stream;
}

// =============================================================================
// Extension Registration
// =============================================================================

function formatDiagnosticsStatus(): string {
  const diag = getLastDiagnostics();
  if (!diag.modelId && !diag.endpoint) {
    return "No AnyRouter requests recorded in this session yet.";
  }

  const lines = [
    `AnyRouter Diagnostics (Last Request):`,
    `  Model:         ${diag.modelId || "unknown"}`,
    `  Protocol:      ${diag.protocol || "unknown"}`,
    `  Endpoint:      ${diag.endpoint || "unknown"}`,
    `  HTTP Status:   ${diag.status ?? "n/a"}`,
    `  Latency:       ${diag.latencyMs != null ? `${diag.latencyMs}ms` : "n/a"}`,
    `  Retries:       ${diag.retryAttempt ?? 0}`,
    `  Request ID:    ${diag.requestId || "none"}`,
    `  System Prompt: ${diag.systemPromptLength ?? 0} chars`,
    `  Tools (${diag.toolsInjected?.length ?? 0}): ${diag.toolsInjected?.join(", ") || "none"}`,
  ];

  if (diag.error) {
    lines.push(`  Last Error:    ${diag.error}`);
  }

  return lines.join("\n");
}

function registerAnyRouterProvider(
  pi: ExtensionAPI,
  providerKey: string,
  providerCfg: any,
  apiId: Api,
  authJson: Json | undefined,
): void {
  const models = buildProviderModels(providerCfg);
  if (models.length === 0) {
    return;
  }

  const baseUrl = (
    process.env.ANYROUTER_BASE_URL ||
    providerCfg?.baseUrl ||
    DEFAULT_BASE_URL
  )
    .trim()
    .replace(/\/+$/, "");

  const apiKey =
    extractAuthEntryKey(authJson?.[providerKey]) ||
    resolveEnvOrLiteral(providerCfg?.apiKey) ||
    process.env.ANYROUTER_API_KEY ||
    "ANYROUTER_API_KEY";

  const streamSimple =
    apiId === API_CODEX_ID ? streamAnyRouterCodex : streamAnyRouterClaude;

  pi.registerProvider(providerKey, {
    baseUrl,
    apiKey,
    api: apiId,
    models: models.map((model) => ({
      id: model.id,
      name: model.name || model.id,
      api: apiId,
      reasoning: model.reasoning ?? true,
      input: model.input ?? ["text"],
      cost: {
        input: model.cost?.input ?? 0,
        output: model.cost?.output ?? 0,
        cacheRead: model.cost?.cacheRead ?? 0,
        cacheWrite: model.cost?.cacheWrite ?? 0,
      },
      contextWindow: model.contextWindow ?? 200000,
      maxTokens: model.maxTokens ?? 32000,
      ...(model.compat ? { compat: model.compat } : {}),
      ...(model.thinkingLevelMap ? { thinkingLevelMap: model.thinkingLevelMap } : {}),
    })),
    streamSimple,
  });
}

export default function (pi: ExtensionAPI): void {
  if (typeof (piAi as any).registerCompatApiProvider === "function") {
    (piAi as any).registerCompatApiProvider(API_CLAUDE_ID, {
      streamSimple: streamAnyRouterClaude,
    });
    (piAi as any).registerCompatApiProvider(API_CODEX_ID, {
      streamSimple: streamAnyRouterCodex,
    });
  }

  const agentDir = getAgentDir();
  const modelsJson = readJsonSafe(join(agentDir, "models.json"));
  const authJson = readJsonSafe(join(agentDir, "auth.json"));

  const providersObj = modelsJson?.providers;
  if (providersObj && typeof providersObj === "object") {
    for (const [providerKey, providerCfg] of Object.entries(providersObj)) {
      const apiId = resolveAnyRouterApi(providerCfg);
      if (apiId) {
        registerAnyRouterProvider(pi, providerKey, providerCfg, apiId, authJson);
      }
    }
  }

  pi.registerCommand("anyrouter-status", {
    description: "Show last AnyRouter request diagnostics",
    handler: async (_args: string, ctx: ExtensionCommandContext) => {
      const report = formatDiagnosticsStatus();
      if (ctx.hasUI) {
        ctx.ui.notify(report, "info");
      } else {
        console.log(report);
      }
    },
  });
}
