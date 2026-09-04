# 使用与运维说明

## 启动

开发：

```bash
cargo run -p aitoolplus
```

Release：

```bash
cargo build --release
./target/release/aitoolplus.exe
```

默认行为：

- 单实例。
- 默认显示主窗口。
- 可设置启动时最小化。
- 可设置关闭按钮最小化到托盘。
- 托盘可重新激活或重新创建主窗口。

## 数据目录

Windows 默认应用目录：

```text
%APPDATA%\aitoolplus\
```

主要文件：

- `store.json`：Provider、Prompt、MCP、Skills、OpenCode add-on profiles。
- `settings.json`：主题、语言、代理、备份、路径和行为设置。
- `aitoolplus.log`：运行日志。
- `skills/`：中央 Skill 仓库。
- `backups/`：手动/自动备份。
- `updates/`：下载的更新资产。

环境覆盖：

- `AITOOLPLUS_HOME`
- `AITOOLPLUS_APPDATA`
- `AITOOLPLUS_<TOOL>_ROOT`
- `AITOOLPLUS_CLI_<COMMAND>`
- `AITOOLPLUS_UPDATE_API`

自动化路由：

- `AITOOLPLUS_START_PAGE`
- `AITOOLPLUS_START_TAB`

## 配置发现

启动时读取已有 CLI 配置并生成 applied provider：

- Claude Code
- Codex
- Gemini CLI
- OpenCode
- Grok
- Pi
- OMP/Hermes/DSH/Claude Desktop runtime-backed profiles

外部文件修改由 watcher 监控，约 350 ms debounce 后刷新。

## Provider 操作

- 新增：支持空白配置和 cc-switch 风格预设。
- 应用：写真实 CLI 配置，写前备份。
- 启动：必要时先应用，再启动 CLI。
- 模型：调用 `/v1/models`。
- 批量测试：显示延迟、模型数量或错误。
- 导入/导出：Provider JSON 数组。
- 排序：上移/下移。
- 托盘：按工具分组快速切换。

## Prompt

Prompt 应用目标由工具 adapter 决定：

- Claude：`CLAUDE.md`
- Codex/OpenCode/Pi/OMP/DSH：`AGENTS.md`
- Gemini：跟随 `settings.json context.fileName`
- Hermes：`SOUL.md`

## MCP

统一模型支持：

- stdio：command、args、env
- HTTP/SSE：URL、headers
- timeout
- favorite/group/note
- 每工具 enabled_tools 和 sync_details

Windows 对 npx/npm/yarn/pnpm/node/bun/deno 自动做 `cmd /c` 目标规范化，Store 中保留可移植原始命令。

## Skills

中央仓库路径：

- 设置中的 `central_repo_path`
- 缺失时回退 `%APPDATA%\aitoolplus\skills`

安装来源：

- 本地目录
- Git URL

同步目标：Claude、Codex、Pi、OpenCode、OMP、Kimi。

## 插件与扩展

### Pi

- `pi list --no-approve`
- 兼容旧 Pi：未知参数时去掉 `--no-approve` 重试
- install/remove/update
- 本地 `.ts` 与 `<dir>/index.ts`

### Oh My Pi

- `omp plugin list --json`
- install/uninstall/upgrade
- 本地扩展删除边界保护

### Claude Code

- installed plugins
- marketplaces
- available plugins
- enable/disable/install/uninstall
- marketplace add/update/remove
- `autoUpdateEnabled` 在 CLI 重写后恢复

### Grok

- installed/available
- install `--trust`
- enable/disable/update/uninstall

## Sessions

- 默认缓存 15 秒。
- 支持 rename sidecar，不改写 CLI 原始会话。
- export schema：`ai-toolbox.session-export.v2`。
- 可按用户/助手/文本/思考/工具调用/命令过滤。

## 备份

### 本地 ZIP

包含：

- Store/settings
- CLI 配置
- Prompt/MCP
- Pi auth/models
- Claude plugin state
- OMO/OMOS
- 中央 Skills
- 自定义条目

### 自动备份

- 按天间隔。
- 保留数量，0 表示不限制。
- 应用启动时检查是否到期。

### WebDAV

支持：

- 测试连接
- 创建远端目录
- 上传
- 列表
- 下载恢复
- 删除
- 自动轮转

注意：WebDAV 密码当前在 settings JSON 中，未接系统凭据库。

### 恢复安全

- ZIP manifest 必须是 `aitoolplus.backup.v1`。
- `home/` 与 `appdata/` 映射到当前机器路径。
- 路径穿越被拒绝。
- 自定义条目默认恢复到 `restored-custom` 沙箱。

## 深度链接

格式：

```text
aitoolbox://v1/import?resource=provider&app=claude&name=MyRelay&apiKey=...&baseUrl=https%3A%2F%2F...
```

支持 app：

- claude
- codex
- gemini

敏感参数 `apiKey/config/extra` 在日志中脱敏。

## 更新

- 检查 GitHub latest release。
- 默认 24 小时自动检查。
- 显示版本和 release notes。
- 下载平台资产到 app-data updates。
- 不自动安装，不静默替换程序。

## 日志与诊断

日志：

```text
%APPDATA%\aitoolplus\aitoolplus.log
```

建议排查顺序：

1. `cargo test --workspace`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. 检查日志是否有 panic/error
4. 查看 Runtime Files 页面
5. 用隔离 `AITOOLPLUS_APPDATA` 重现
6. 对照 `KNOWN_ISSUES.md`
