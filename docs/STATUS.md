# AI ToolPlus 当前状态

> 最后核验：2026-09-04
>
> 本文是项目完成度的唯一权威摘要。功能细节见 `UPSTREAM_PARITY.md`，测试证据见 `TESTING.md`，已知问题见 `KNOWN_ISSUES.md`。

## 结论

AI ToolPlus 已经是可构建、可运行的 Windows 开发版，核心配置管理功能已实现，并用本机真实数据验证了主要工具。

它还不是正式发行版，也不能宣称与 ai-toolbox 所有边界行为、所有平台和所有发行能力完全等价。

当前估算：

- Windows 日常配置管理场景：约 90% 完成。
- 正式发行与全平台交付：约 70% 完成。
- ai-toolbox 除明确排除项外逐命令、逐交互完全等价：尚未证明。

## 已完成

### 工具范围

主工作台支持 12 个工具：

1. Claude Code
2. Codex
3. Gemini CLI
4. Grok
5. Kimi
6. OpenCode
7. OpenClaw
8. Pi
9. Oh My Pi
10. Claude Desktop
11. Hermes
12. DSH

OpenCode 附加运行时还包括：

- Oh My OpenAgent
- Oh My OpenCode Slim

### 核心功能

- Provider：现有配置发现、CRUD、分类、预设、应用、停用、导入导出、排序、连通性批测、模型列表、CLI 启动。
- cc-switch 风格：API-key 预设模板、托盘按工具分组快速切换。
- Prompt：预设 CRUD 与工具真实 Prompt 文件应用。
- MCP：服务器 CRUD、收藏、分组、工具级启停、env、headers、timeout、多格式同步、Windows `cmd /c` 规范化、现有配置发现。
- Skills：中央仓库唯一事实源、6 工具同步、Git clone/pull、本地导入、状态记录。
- Sessions：扫描、搜索、详情、过滤、重命名、导入导出、删除、Grok 目录会话、缓存。
- Pi：provider runtime 合并、模型设置、其他设置、包扩展、本地扩展。
- Claude Code：插件市场、可安装插件、安装、启停、卸载、市场增删改、自动更新标志。
- Grok：原生插件已安装/可安装、安装、启停、更新、卸载。
- Oh My Pi：包扩展与本地扩展。
- Hermes：provider、默认模型、Prompt、memory 文件和启用开关。
- Claude Desktop：第三方 profile、configLibrary、appliedId、恢复官方端点。
- OpenCode 附加工具：OMO/OMOS profile、统一/Legacy 路径、global 合并、apply/clear、Slim 规范化。

### 应用与运维

- Windows 开机自启。
- 启动时最小化。
- 关闭时最小化到托盘。
- 托盘重新打开或重新创建主窗口。
- 单实例与第二实例 IPC。
- `aitoolbox://v1/import` 深度链接。
- 外部配置文件变更监听与自动刷新。
- 系统/直连/自定义代理。
- 可见工具控制。
- 工具配置根目录与手动 CLI 路径。
- Session 详情过滤。
- 本地 ZIP 备份、自动备份、文件过滤、自定义条目。
- WebDAV 上传、列举、下载恢复、删除和远端轮转。
- GitHub Releases 更新检查与资产下载。
- Runtime 文件只读预览。

## 明确排除

以下功能由用户明确要求不做，不属于未完成项：

- 浏览器插件或浏览器 LevelDB 导入。
- 官方账号 OAuth 自动刷新。
- SSH 同步。
- WSL 同步。
- 本机代理 Gateway。
- Image 工作台。

## 尚未完成

### 产品和发行

- S3 远端备份。
- Windows MSI/NSIS 安装包。
- 开始菜单与桌面快捷方式的安装管理。
- 卸载时清理协议、开机自启和可选用户数据。
- Windows EXE 正式图标/版本资源。
- 代码签名。
- 更新包自动安装、失败回滚和签名校验。

### 安全和恢复

- 自定义绝对恢复路径的逐项确认。
- 文件冲突策略：覆盖、跳过、另存。
- 备份加密。
- WebDAV 密码目前存储在 settings JSON 中，尚未接 Windows Credential Manager。

### 验收

- 尚未用真实 WebDAV 服务验证，仅有模拟服务器全链路测试。
- Oh My Pi、Grok、Kimi、OpenClaw、Hermes、DSH、Claude Desktop 缺少完整本机 runtime，主要依赖夹具/round-trip 测试。
- macOS/Linux 未真机测试。
- 尚无覆盖所有表单和确认框的稳定 UI 点击自动化。
- 未逐像素复制 ai-toolbox 前端；当前是 GPUI 原生布局。

## 当前证据

- 170 个测试：core 160、UI 5、bin 5。
- Clippy：`-D warnings` 通过。
- Release：`target/release/aitoolplus.exe`，约 15 MB。
- 真实配置验证：Claude Code、Codex、Gemini CLI、OpenCode、Pi。
- 真实 Pi：13 个 provider、16 个包扩展、1 个本地扩展。
- 真实 Claude Code：4 个已安装插件、3 个市场。
- 真实 API Hub：从实际 provider 拉取 19 个模型。
- 真机脚本：关闭到托盘、深度链接 IPC、配置监听全部通过。
