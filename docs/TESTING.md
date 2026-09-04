# 测试与验收

> 本文记录可重复的验证命令、测试范围、真机脚本和截图证据。

## 标准门禁

在项目根目录执行：

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

最后一次权威结果：

- 测试总数：170
- core：160
- UI：5
- bin：5
- Clippy：零警告
- Release：成功，`target/release/aitoolplus.exe` 约 15 MB

获取实时测试数：

```bash
cargo test --workspace -- --list | awk '/: test$/{n++} END{print n}'
```

## 测试覆盖范围

### Core

- JSON/TOML/YAML 原子写与未知字段保留
- Provider CRUD、排序、导入导出、现有配置发现
- Claude managed env/protected fields
- Codex TOML 分层和 auth token 策略
- Gemini managed env 清理与 auth selector
- Grok/Kimi TOML catalog
- Pi auth/models/settings 三源合并
- OMP/Hermes/DSH/Claude Desktop runtime round-trip
- MCP 多工具格式与 `cmd /c`
- Skills 本地/Git/同步
- Sessions 格式、导入导出、缓存
- Pi/OMP 扩展
- Claude/Grok 插件
- OpenCode 附加工具
- ZIP 备份、恢复、过滤、轮转、防目录穿越
- WebDAV 全链路模拟服务
- updater 版本比较和 mock Release API
- deep-link parser、Base64、脱敏和 store 去重

### UI

UI 测试目前以纯逻辑和渲染启动为主：

- 主题
- i18n
- TextInput secret offset
- TextArea 行导航

完整 UI 点击自动化仍未完成，见 `KNOWN_ISSUES.md`。

### Bin / 系统

- 托盘菜单 ID 编解码和 snapshot
- 单实例行协议
- Windows 自启值名

## 真机级脚本

### 关闭到托盘

```powershell
powershell -ExecutionPolicy Bypass -File tools/test-close-to-tray.ps1
```

成功条件：发送 `WM_CLOSE` 后进程仍存活且响应正常。

### 深度链接 IPC

```powershell
powershell -ExecutionPolicy Bypass -File tools/test-deeplink-ipc.ps1
```

成功条件：第二实例退出，第一实例继续运行，`IpcRelay` 写入第一实例 store。

### 配置变更监听

```powershell
powershell -ExecutionPolicy Bypass -File tools/test-config-watch.ps1
```

成功条件：外部修改 Claude settings 后，store 自动更新为 `second.example` 和新 token。

## 截图验收

页面启动器：

```powershell
powershell -ExecutionPolicy Bypass -File tools/launch-and-capture.ps1 `
  -Page pi -Tab extensions -Name pi-extensions
```

底层截图使用 `PrintWindow(PW_RENDERFULLCONTENT)`，不依赖窗口处于前台。

推荐最终截图：

| 页面 | 文件 |
|---|---|
| Provider 操作 | `docs/screenshots/57-provider-header-final.png` |
| Pi 扩展 | `docs/screenshots/31-start-debug.png` |
| Pi 模型设置 | `docs/screenshots/32-pi-model-settings.png` |
| MCP | `docs/screenshots/37-mcp-fixed.png` |
| Claude 插件 | `docs/screenshots/39-plugins-final.png` |
| Sessions 过滤 | `docs/screenshots/60-session-filters.png` |
| WebDAV | `docs/screenshots/49-settings-webdav.png` |
| Skills Git | `docs/screenshots/54-skills-git.png` |
| OpenCode Add-ons | `docs/screenshots/61-addons-unified.png` |
| 设置/备份 | `docs/screenshots/46-settings-backup-controls.png` |

`01`–`30` 的部分图片是开发中间诊断图，可能展示已修复的问题，不应作为最终 UI 证据。

## 真实环境已验证

- Windows 10/11 当前开发机
- GPU：NVIDIA RTX 5060，Direct3D 11.1
- Pi：13 provider、16 package extensions、1 local extension
- Claude Code：4 installed plugins、3 marketplaces
- Claude/Codex/Gemini/OpenCode 现有配置导入
- 真实 API provider `/v1/models`：19 models

## 尚未真实验证

- 真实 WebDAV 服务器
- S3
- macOS/Linux
- OMP/Grok/Kimi/OpenClaw/Hermes/DSH/Claude Desktop 完整 runtime
- 真实插件卸载或更新等破坏性操作
- Windows 安装器/卸载器/签名

## 安全说明

自动化默认使用隔离目录：

```text
C:\temp\aitoolplus-*\appdata
```

涉及用户真实配置的测试应保持只读。不要在自动化中执行真实插件卸载、provider 删除、远端备份删除或覆盖恢复，除非明确授权。
