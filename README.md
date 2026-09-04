# AI ToolPlus

Rust + GPUI 原生 AI 编程助手配置工作台，对标 `coulsontl/ai-toolbox`，Provider 使用体验参考 cc-switch。

> 当前是 Windows 可运行开发版，不是已签名的正式发行包。准确完成度见 [当前状态](docs/STATUS.md)。

## 支持范围

12 个主工具：

- Claude Code
- Codex
- Gemini CLI
- Grok
- Kimi
- OpenCode
- OpenClaw
- Pi
- Oh My Pi
- Claude Desktop
- Hermes
- DSH

附加运行时：Oh My OpenAgent、Oh My OpenCode Slim。

## 主要功能

- Provider：现有配置发现、CRUD、预设、应用、导入导出、排序、批量测试、模型获取、CLI 启动、托盘切换。
- Prompt、MCP、Skills、Sessions。
- Pi/OMP extensions、Claude/Grok plugins。
- Pi model/other settings、Hermes memory、Claude Desktop profiles、OpenCode add-ons。
- Runtime 文件预览与外部配置变更自动刷新。
- 本地/自动/WebDAV ZIP 备份恢复。
- `aitoolbox://v1/import` 深度链接和单实例转发。
- 启动/托盘/代理/路径/可见工具/Session 过滤/更新检查。

明确排除：浏览器插件导入、OAuth 自动刷新、SSH、WSL、Gateway、Image。

尚未完成：S3、更新自动安装、Windows 安装包/卸载器/签名、macOS/Linux 真机适配、完整 UI 点击自动化。详见 [已知问题](docs/KNOWN_ISSUES.md)。

## 构建

```bash
cargo run -p aitoolplus
cargo build --release
```

Windows 产物：

```text
target/release/aitoolplus.exe
```

## 验证

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

最后一次核验：170 个测试，Clippy 零警告，release 成功。

真机脚本：

```powershell
powershell -ExecutionPolicy Bypass -File tools/test-close-to-tray.ps1
powershell -ExecutionPolicy Bypass -File tools/test-deeplink-ipc.ps1
powershell -ExecutionPolicy Bypass -File tools/test-config-watch.ps1
```

完整测试说明：[docs/TESTING.md](docs/TESTING.md)。

## 数据目录

Windows 默认：

```text
%APPDATA%\aitoolplus\
├── store.json
├── settings.json
├── aitoolplus.log
├── skills\
├── backups\
└── updates\
```

主要环境变量：

- `AITOOLPLUS_HOME`
- `AITOOLPLUS_APPDATA`
- `AITOOLPLUS_<TOOL>_ROOT`
- `AITOOLPLUS_CLI_<COMMAND>`
- `AITOOLPLUS_UPDATE_API`
- `AITOOLPLUS_START_PAGE` / `AITOOLPLUS_START_TAB`（自动化）

## 文档

| 文档 | 内容 |
|---|---|
| [STATUS.md](docs/STATUS.md) | 当前做到哪里、未完成什么 |
| [UPSTREAM_PARITY.md](docs/UPSTREAM_PARITY.md) | ai-toolbox 对标矩阵 |
| [DESIGN.md](docs/DESIGN.md) | 架构、数据模型、运行时语义 |
| [PROGRESS.md](docs/PROGRESS.md) | 里程碑、开发历史和证据 |
| [TESTING.md](docs/TESTING.md) | 自动化、真机脚本和截图 |
| [KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md) | 未解决问题、事故和踩坑 |
| [OPERATIONS.md](docs/OPERATIONS.md) | 使用、备份、恢复、诊断 |
| [RELEASE.md](docs/RELEASE.md) | 正式发布前的安装、签名、安全与真机清单 |

## 重要说明

当前目录尚未初始化 Git。正式继续开发或发布前，应先放入版本控制，以便审查 diff、回滚和生成版本记录。
