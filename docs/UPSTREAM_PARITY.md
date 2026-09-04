# ai-toolbox 对标矩阵

> 对标来源：`coulsontl/ai-toolbox` 当前源码、各 `AGENTS.md`、`web/constants/modules.tsx`、页面和命令实现。
>
> 状态含义：✅ 已实现并有证据；🟨 已实现主要路径但缺真机/边界验收；⬜ 未实现；⛔ 用户明确排除。

## 编码工具

| 上游模块 | AI ToolPlus | 状态 | 事实源/说明 |
|---|---|---|---|
| Claude Code | Claude Code | ✅ | `.claude/settings.json`、`.claude.json`、`CLAUDE.md`、plugins；受管 env 和 protected fields 对齐 |
| Codex | Codex | ✅ | `.codex/config.toml`、`auth.json`、`AGENTS.md`；TOML 分层合并、OAuth token 保留策略 |
| Gemini CLI | Gemini CLI | ✅ | `.gemini/.env`、`settings.json`、动态 Prompt 文件；managed env 清理和 auth selector |
| Grok | Grok | 🟨 | `config.toml`、`auth.json`、Prompt、原生插件；本机缺 Grok CLI |
| Kimi | Kimi | 🟨 | `.kimi-code/config.toml`、credentials 路径；`max_context_size` 兜底；本机缺完整 runtime |
| OpenCode | OpenCode | ✅ | `opencode.jsonc` 优先、`AGENTS.md` 同目录、provider map 保留 |
| OpenClaw | OpenClaw | 🟨 | `.openclaw/openclaw.json` 单文件深合并；本机缺 runtime |
| Pi | Pi | ✅ | `auth.json` + `models.json` + `settings.json`；模型/设置/扩展/Prompt/Session |
| Oh My Pi | Oh My Pi | 🟨 | `models.yml`、`config.yml`、`mcp.json`、`AGENTS.md`、`omp plugin`；本机缺 CLI |
| Claude Desktop | Claude Desktop | 🟨 | live config、configLibrary、`_meta.json`、官方恢复；缺实际 Desktop profile 测试 |
| Hermes | Hermes | 🟨 | `config.yaml`、`SOUL.md`、memory 文件；缺真实 CLI |
| DSH | DSH | 🟨 | `settings.yaml`、`.credentials.yaml`、`AGENTS.md`；缺真实 CLI |
| Oh My OpenAgent | OpenCode 附加工具 | ✅ | 默认 `~/.omo/omo.jsonc` 的 `opencode` block，Legacy 可选 |
| Oh My OpenCode Slim | OpenCode 附加工具 | ✅ | `oh-my-opencode-slim.json`，profile/global merge，v2 规范化 |

## Provider 能力

| 能力 | 状态 | 说明 |
|---|---|---|
| CRUD | ✅ | 本地 Store + runtime apply |
| 应用/停用 | ✅ | 工具 adapter 写真实配置 |
| 分类/备注/网址 | ✅ | ProviderRecord |
| 导入/导出 | ✅ | JSON 文件；UI 入口 |
| 排序 | ✅ | 上移/下移，sort_index |
| 现有配置发现 | ✅ | Claude/Codex/Gemini/OpenCode/Grok/Pi 等 runtime-backed 模块 |
| 预设模板 | ✅ | cc-switch 风格，API-key 注入 |
| `/v1/models` | ✅ | HTTP 获取、解析、真实 provider 测试 |
| 单条/批量连通性 | ✅ | 延迟、模型数和错误状态 |
| 托盘切换 | ✅ | 按工具分组，applied 标记 |
| CLI 启动 | ✅ | 应用后启动，手动 CLI 路径，Claude full-access 可选 |
| 官方 OAuth 登录/刷新 | ⛔ | 用户明确不需要；采用 cc-switch 的 API-key 范围 |
| 浏览器扩展导入 | ⛔ | 用户明确不要 |

## MCP

| 能力 | 状态 |
|---|---|
| CRUD、收藏、分组、备注 | ✅ |
| Stdio command/args/env | ✅ |
| HTTP/SSE URL/headers | ✅ |
| startup/tool timeout | ✅ |
| 每工具启停与 sync_details | ✅ |
| Windows `cmd /c` 规范化 | ✅ |
| Claude/Codex/Gemini/OpenCode/Grok/Kimi/Pi/OMP/OpenClaw 格式 | ✅ |
| 现有配置发现 | ✅ |
| Cordis/特殊 runtime patch 的全部边界 | 🟨；核心格式实现，未覆盖上游全部历史版本 fixture |

## Skills

| 能力 | 状态 |
|---|---|
| 中央仓库唯一事实源 | ✅ |
| 本地目录导入 | ✅ |
| Git clone/pull | ✅ |
| Claude/Codex/Pi/OpenCode/OMP/Kimi 同步 | ✅ |
| per-tool target 状态 | ✅ |
| Windows copy/junction fallback | ✅ |
| Unix symlink 真机测试 | ⬜ |

## Sessions

| 能力 | 状态 |
|---|---|
| 多工具扫描 | ✅ |
| Grok summary/chat_history 目录格式 | ✅ |
| JSONL 解析和消息提取 | ✅ |
| 搜索、详情、过滤 | ✅ |
| rename sidecar | ✅ |
| export v2 / import | ✅ |
| 删除 | ✅ |
| 15 秒缓存 | ✅ |
| 所有上游历史版本/快照格式 | 🟨；核心格式覆盖，未穷尽历史 fixture |

## 插件与扩展

| 能力 | 状态 |
|---|---|
| Pi package/local extension list | ✅ |
| Pi install/remove/update | ✅ |
| npm latest 检查 | ✅；已移出 render 线程 |
| OMP package/local extension | ✅ |
| Claude marketplace list/add/update/remove | ✅ |
| Claude available/install/installed/enable/uninstall | ✅ |
| Claude autoUpdate flag 保留 | ✅ |
| Grok installed/available/install/trust/enable/update/uninstall | ✅ |
| 所有真实 CLI 破坏性操作真机测试 | 🟨；出于安全未对用户真实环境执行卸载/安装 |

## 应用设置与系统

| 能力 | 状态 |
|---|---|
| 语言、主题 | ✅ |
| 可见工具 | ✅ |
| 配置根目录 | ✅ |
| 手动 CLI 路径 | ✅ |
| 系统/直连/自定义代理 | ✅ |
| 开机自启 | ✅ |
| 启动最小化 | ✅ |
| 关闭最小化到托盘 | ✅ |
| 托盘重开/重建窗口 | ✅ |
| 单实例 | ✅ |
| 深度链接 | ✅ |
| 配置文件监听 | ✅ |
| Session 过滤 | ✅ |
| Claude full-access 启动 | ✅ |
| Codex auth 保留策略 | ✅ |
| OMO 路径/清除/variant 策略 | ✅ |

## 备份与更新

| 能力 | 状态 |
|---|---|
| 完整 ZIP bundle | ✅ |
| CLI 文件过滤 | ✅ |
| 自定义文件/目录 | ✅ |
| 本地自动备份与轮转 | ✅ |
| WebDAV 测试/上传/列表/下载恢复/删除/轮转 | ✅；模拟服务全链路 |
| S3 测试/上传/列表/下载恢复/删除/轮转（SigV4） | ✅；单元与端到端测试 |
| 自定义绝对路径确认和冲突策略（覆盖/跳过/另存） | ✅；带沙箱保护 |
| 更新检查、release notes、资产下载 | ✅ |
| 自动安装/重启/校验（SHA-256、NSIS、脱机替换脚本） | ✅ |
| 凭据安全（Windows DPAPI 加密存储） | ✅ |

## 明确排除

- 浏览器插件导入
- OAuth 自动刷新
- SSH
- WSL
- Gateway
- Image
