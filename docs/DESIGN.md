# AI ToolPlus 设计文档

> Rust + GPUI 原生桌面应用，对标 ai-toolbox 的配置管理能力，供应商体验参考 cc-switch。
>
> 当前实现状态以 `STATUS.md` 为准，功能对标以 `UPSTREAM_PARITY.md` 为准。

## 目标与边界

目标：在不使用 WebView/JavaScript 前端的前提下，为 AI 编程 CLI 提供统一的 Provider、Prompt、MCP、Skills、Session、插件/扩展、备份和系统集成管理。

明确排除：浏览器插件导入、OAuth 自动刷新、SSH、WSL、Gateway、Image。

Windows 是当前正式开发目标；macOS/Linux 仅保留结构兼容性。

## Workspace

```text
aitoolplus/
├── crates/
│   ├── aitoolplus-core/   # 业务模型、配置 adapter、IO、测试
│   ├── aitoolplus-ui/     # GPUI 主题、组件、页面、Workspace
│   └── aitoolplus/        # 入口、托盘、自启、单实例、watcher
├── docs/
├── tools/                 # 真机测试与截图脚本
└── target/release/aitoolplus.exe
```

### 依赖方向

```text
aitoolplus (bin)
  ├─ aitoolplus-ui
  └─ aitoolplus-core

aitoolplus-ui
  └─ aitoolplus-core

aitoolplus-core
  └─ 无 GPUI 依赖
```

Core 必须能够脱离窗口执行 round-trip 测试。UI 不直接理解每个 CLI 的具体文件格式，优先调用 adapter 或专属 runtime 模块。

## 状态模型

### Store

`store.json` 保存：

- `tools.<tool>.providers`
- `tools.<tool>.common_config`
- `tools.<tool>.prompts`
- MCP
- Skills
- OpenCode add-on profiles

Store 使用 JSON，便于备份和人工检查。运行时事实源仍是各 CLI 自己的文件；Store 不取代 runtime。

### Settings

`settings.json` 保存：

- language/theme
- startup/tray behavior
- proxy
- visible tools
- tool root overrides
- manual CLI paths
- backup/WebDAV/filter/custom entries
- update checking
- session filters
- Codex/Claude/OMO policies

敏感性限制：WebDAV 密码目前仍在 settings JSON，正式版应迁移系统凭据库。

## 工具 adapter

统一 trait：

```rust
pub trait ToolAdapter {
    fn tool(&self) -> ToolId;
    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError>;
    fn read_current(&self, paths: &Paths) -> Result<Value, ApplyError>;
    fn default_settings(&self) -> Value;
    fn runtime_files(&self, paths: &Paths) -> Vec<(String, PathBuf)>;
    fn prompt_file(&self, paths: &Paths) -> Option<PathBuf>;
}
```

工具特有逻辑不能退化成通用 JSON 覆盖：

- Claude：managed env、model field projection、protected fields。
- Codex：TOML provider/common layering、auth.json policy。
- Gemini：managed `.env` 清理、auth selector、动态 Prompt filename。
- Grok/Kimi：TOML catalog。
- Pi：auth/models/settings 三源。
- OMP/Hermes/DSH：YAML runtime merge。
- Claude Desktop：profile library + meta appliedId。

## Runtime-backed modules

### Pi

```text
~/.pi/agent/
├── settings.json
├── auth.json
├── models.json
├── AGENTS.md
├── extensions/
└── sessions/
```

Provider view 合并 auth、models 和 default selection。`packages` 由扩展链拥有，Other Settings 保存时必须保留。

### Claude plugins

```text
~/.claude/
├── settings.json                 # enabledPlugins
└── plugins/
    ├── installed_plugins.json
    └── known_marketplaces.json
```

CLI 命令可能重写 marketplace 文件，因此 autoUpdate flags 在 CLI 前后快照/恢复。

### OpenCode add-ons

默认 OMO 使用：

```text
~/.omo/omo.jsonc
└── opencode: { ... }
```

Legacy 可选：

```text
~/.config/opencode/oh-my-openagent.jsonc
```

OMOS：

```text
~/.config/opencode/oh-my-opencode-slim.json
```

## MCP

统一服务器模型：

- stdio：command、args、env
- http/sse：url、headers
- timeout、favorite、group、enabled_tools、sync_details

格式映射：

| 工具 | 格式 |
|---|---|
| Claude/Pi/OMP/OpenClaw | JSON `mcpServers` |
| OpenCode | JSON `mcp`，local/remote |
| Gemini | JSON `mcpServers`，httpUrl/url |
| Codex/Kimi | TOML `mcp_servers` |
| Grok | 专属 TOML，无 type，带 enabled |

Windows 本地目标对 npx/npm/yarn/pnpm/node/bun/deno 做 `cmd /c` 包装；Store 中保留原命令。

## Skills

中央仓库是唯一内容事实源：

```text
settings.central_repo_path
  fallback → app_data/skills
```

Skill 可来自本地目录或 Git。同步记录每工具 target path、mode、status 和时间。

## Sessions

Session 不存数据库，直接扫描 runtime。

- Claude/Codex/Pi 等：JSONL
- Grok：`summary.json + chat_history.jsonl` 目录
- 归一化 SessionMeta/Message
- 15 秒列表缓存
- rename 使用 sidecar，避免修改 runtime
- export schema v2

## UI 架构

Workspace 持有：

- StoreHandle
- AppSettings
- Theme/I18n
- 当前 Page/Tab
- transient editors/dialogs/results
- host callbacks

页面：

- 12 工具页面
- MCP
- Skills
- Sessions
- Settings

工具页标签按能力动态出现：

- Providers
- Common Config
- Prompts
- Runtime Files
- Pi/OMP Extensions
- Claude/Grok Plugins
- OpenCode Add-ons

### GPUI 规则

- 所有 Entity 在 GPUI context 中创建。
- 文件/网络/CLI/Git 操作不得在 render 线程执行。
- 外部模型回显使用 silent setter。
- Modal 挂根层。
- 不依赖 emoji 字体作为唯一动作表达。
- 长列表使用可预测两行卡片和 wrap。

## 生命周期

```text
main
├─ logging
├─ single-instance mutex + IPC listener
├─ protocol registration
├─ settings/paths
├─ auto-backup/update check
├─ runtime discovery
├─ config watcher
├─ tray thread
└─ GPUI application
   └─ Workspace window
```

关闭窗口可最小化到托盘。托盘在没有 Workspace window 时可以重新创建窗口。

## 单实例与深度链接

- Windows named mutex 防双实例。
- localhost line protocol 转发第二实例 argv。
- `aitoolbox://v1/import` 支持 Claude/Codex/Gemini provider。
- 敏感 query 参数日志脱敏。

## 配置监听

`notify` 递归监听已存在的工具根目录：

- 350 ms debounce
- app-data 不监听，避免 Store 保存反馈循环
- 变更触发 runtime provider 重新发现

## 备份

ZIP manifest：`aitoolplus.backup.v1`。

命名空间：

- `home/...`
- `appdata/...`
- `custom/...`

恢复拒绝目录穿越。自定义条目默认进入安全沙箱。

WebDAV 使用 PROPFIND/MKCOL/PUT/GET/DELETE。自动备份可上传并轮转远端文件。

## 更新

- GitHub latest Release API
- semver-ish 比较
- 平台资产选择
- 下载大小校验
- 不自动安装

## 测试策略

详见 `TESTING.md`。原则：

1. Core 格式必须 round-trip。
2. 未知字段必须保留。
3. 配置切换必须清理旧受管字段。
4. 破坏性真机操作默认不执行。
5. UI 必须有截图证据。
6. 系统功能必须有独立 PowerShell 真机脚本。
