# AI ToolPlus 进度书

> 状态摘要见 `STATUS.md`；本文件记录里程碑、证据和开发历史。
>
> 状态：✅ 完成；🟨 部分完成/待真机验收；⬜ 未完成；⛔ 明确排除。

## 当前门禁

最后核验：2026-09-04。

```text
cargo test --workspace                                  PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo build --release                                  PASS
```

- 测试：170（core 160、UI 5、bin 5）
- Release：`target/release/aitoolplus.exe`，约 15 MB
- 真机脚本：关闭到托盘、deep-link IPC、config watcher 通过
- 截图：`docs/screenshots/`

## 里程碑

### M0 工程与 GPUI 框架

| 项 | 状态 |
|---|---|
| Cargo workspace：core/ui/bin | ✅ |
| GPUI window/workspace/theme/i18n | ✅ |
| TextInput/TextArea/components | ✅ |
| Windows release build | ✅ |

### M1 Provider 与工具 adapter

| 项 | 状态 |
|---|---|
| 12 工具 ToolId/路径/runtime 文件 | ✅ |
| Provider CRUD/分类/备注/网址 | ✅ |
| 应用、停用、导入导出、排序 | ✅ |
| 现有配置发现 | ✅ |
| cc-switch 预设/API-key 注入 | ✅ |
| `/v1/models` 和批量连通性 | ✅ |
| 托盘切换和 CLI 启动 | ✅ |
| 缺 runtime 工具真机验证 | 🟨 |

### M2 Prompt、MCP、Skills、Sessions

| 项 | 状态 |
|---|---|
| Prompt CRUD 和真实文件应用 | ✅ |
| MCP 多格式同步/发现/收藏/分组/env/headers/timeout | ✅ |
| Skills 中央仓库、本地/Git、6 工具同步 | ✅ |
| Sessions 扫描/详情/过滤/rename/import/export/delete/cache | ✅ |
| 所有历史 runtime fixture | 🟨 |

### M3 插件与扩展

| 项 | 状态 |
|---|---|
| Pi package/local extensions | ✅ |
| OMP package/local extensions | ✅ |
| Claude marketplaces/available/installed/install/enable/uninstall | ✅ |
| Grok installed/available/install/enable/update/uninstall | ✅ |
| 真实用户环境破坏性操作 | 🟨，未自动执行 |

### M4 专属模块

| 项 | 状态 |
|---|---|
| Pi provider/model/other settings | ✅ |
| Claude Desktop profile/official restore | ✅ |
| Hermes providers/memory | ✅ |
| DSH settings/credentials | ✅ |
| OMO unified/legacy profiles | ✅ |
| OMOS profiles/normalization | ✅ |

### M5 系统集成

| 项 | 状态 |
|---|---|
| 开机自启 | ✅ |
| 启动最小化/关闭到托盘 | ✅ |
| 托盘重开/重建窗口 | ✅ |
| 单实例与 IPC | ✅ |
| deep-link v1 import | ✅ |
| 配置文件 watcher | ✅ |
| 系统/直连/自定义代理 | ✅ |
| 可见工具/根目录/CLI 路径 | ✅ |
| Session filters/Claude/Codex/OMO policies | ✅ |

### M6 备份与更新

| 项 | 状态 |
|---|---|
| 完整 ZIP bundle/restore | ✅ |
| 文件过滤和自定义条目 | ✅ |
| 自动备份/本地轮转 | ✅ |
| WebDAV 全链路/远端管理 | ✅，模拟服务验证 |
| S3 | ⬜ |
| 自定义绝对恢复确认/冲突策略 | ⬜ |
| 更新检查/notes/asset 下载 | ✅ |
| 自动安装/回滚/签名校验 | ⬜ |

### M7 发行

| 项 | 状态 |
|---|---|
| README/设计/状态/测试/问题/运维文档 | ✅ |
| Windows installer/uninstaller | ⬜ |
| Windows icon/version resources | ⬜ |
| 代码签名 | ⬜ |
| macOS/Linux 真机适配 | ⬜ |
| 完整 UI 点击自动化 | ⬜ |

## 明确排除

- 浏览器插件导入
- OAuth 自动刷新
- SSH
- WSL
- Gateway
- Image

## 真机证据

### 已验证数据

- Pi：13 provider，`gt-token`、`gpt-5.6-sol`、`medium`
- Pi：16 package extensions + 1 local extension
- Claude Code：4 installed plugins + 3 marketplaces
- Claude/Codex/Gemini/OpenCode：现有配置导入
- API provider：19 models

### 系统脚本

- `tools/test-close-to-tray.ps1`
- `tools/test-deeplink-ipc.ps1`
- `tools/test-config-watch.ps1`

### 最终截图

- `57-provider-header-final.png`
- `31-start-debug.png`
- `32-pi-model-settings.png`
- `37-mcp-fixed.png`
- `39-plugins-final.png`
- `60-session-filters.png`
- `49-settings-webdav.png`
- `54-skills-git.png`
- `61-addons-unified.png`

## 开发历史与重大修正

### 初期原型误判

初版只实现了通用工作台，却错误标记为 M0–M5 完成。Pi provider 页面为空，多个 adapter 根据猜测实现。此后以 ai-toolbox 源码/模块文档重新对齐，并把“编译通过”和“功能完成”分开。

### Runtime 真值源修正

- Pi 改为 auth/models/settings 三源合并。
- Kimi 改为 `.kimi-code`。
- Grok 改为 TOML catalog。
- OpenClaw 改为 `openclaw.json`。
- Gemini 加入 managed env 清理和 auth selector。
- Codex 加入 TOML 分层、schema header 和 auth policy。

### 功能范围扩展

依次补齐：

- OMP/Hermes/DSH/Claude Desktop
- MCP 多格式
- Sessions/Skills
- API Hub/runtime preview
- cc-switch presets/tray
- Pi extensions/model/other settings
- Claude plugins
- Provider import/order/batch test/launch
- 本地/WebDAV backup/updater
- deep-link/config watcher
- OMP/Grok plugins
- OMO/OMOS
- app settings and file filters

### UI 截图驱动修正

截图发现并修复：

- 操作按钮横向溢出
- emoji glyph 透明
- MCP 操作按钮不可见
- Sessions 操作不可见
- 插件市场操作不可见
- 备份 toggle 不可见
- 扩展页 render 线程同步网络请求

### 工具事故

- 两次 Python 补丁脚本因 `newline='\\n'` 在校验失败前截断 `tool_page.rs`，之后完整重建。
- 已禁止再用会先截断目标的大文件 Python 补丁方式；使用 `write`/`edit`。
- Release EXE 运行时会被 Windows 锁定；重建前先终止进程。
- 截图改用 `PrintWindow`，避开前台焦点限制。
- PowerShell 测试改用无 BOM UTF-8 和兼容的 `Arguments`。

详细问题见 `KNOWN_ISSUES.md`。
