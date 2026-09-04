# 已知问题、风险与开发事故

> 这里同时记录尚未解决的问题和已经解决但值得保留的事故/经验，防止后续重复踩坑。

## 尚未解决 / 后续考虑

### 完整 UI 自动化框架
- 已具备参数化启动、全窗口 DirectComposition/PrintWindow 截图、系统级 IPC 与托盘测试。
- 目前尚未接入全表单、下拉菜单逐项自动化点击填写的全量 E2E 测试框架。

### 真机 CLI 覆盖不完整
- 本机环境缺少 OMP、Grok、Kimi、OpenClaw、Hermes、DSH、Claude Desktop 完整二进制环境。
- 这些模块依赖完备的测试夹具与 round-trip 解析测试验证。

### 跨平台未真机验收
- 当前完全交付并验证 Windows 原生平台（GPUI DirectX 11.1、系统托盘、Windows 快捷方式与注册表、DPAPI 凭据保护、NSIS 安装程序）。
- macOS / Linux 版本需要在相应物理机或 CI 虚拟机中验证打包。

### 代码签名
- 需具备公信力的商业代码签名证书（EV/OV 代码签名）以消除 Windows SmartScreen 提示。

## 已解决的重要问题

### S3 远端备份与轮转支持
- 问题：上游支持 S3 远端存储，初始版本只实现了 WebDAV。
- 解决：在 `aitoolplus-core::s3` 中实现了原生 AWS SigV4 认证签名算法，支持自定义 Endpoint、Region、Bucket、Prefix 与 Path-Style 选项。提供了连接测试、上传、列举、下载恢复、删除及与自动备份联动的远端轮转清理功能。

### 自定义绝对恢复路径与冲突策略
- 问题：旧版备份恢复遇到同名文件默认直接写入，且自定义绝对路径缺乏保护。
- 解决：引入 `ConflictStrategy`（覆盖原文件、跳过同名文件、另存副本为 `.restored` 后缀），支持沙箱隔离保护开关，并在 UI 中提供了直观的策略选择按钮。

### WebDAV 与 S3 凭据明文存储风险
- 问题：敏感密码和密钥明文保存在 `settings.json`。
- 解决：在 `aitoolplus-core::security` 中通过 Windows 原生 DPAPI (`CryptProtectData` / `CryptUnprotectData`) 进行加解密，保存时自动加密加前缀 `dpapi:`，读取时透明解密，并保持对旧明文的向前兼容。

### 更新自动安装与校验
- 问题：最初仅能下载 release 资产，无法自动启动安装或退出替换。
- 解决：在 `aitoolplus-core::updater` 中实现了 SHA-256 哈希校验，支持检测 `.exe` / `.msi` 安装包并自动使用 Windows ShellExecute 启动，或对绿色便携版通过脱机 PowerShell 脚本进行进程等待、文件就地替换与自动重启。

### Windows EXE 图标、版本资源与 NSIS 安装包
- 问题：无 Windows 原生图标与 `VERSIONINFO`，且缺少安装/卸载程序。
- 解决：
  - 生成 256x256 高清 `app.ico`，编写 `resources.rc` 包含文件版本 `0.1.0.0`、产品名、版权等元数据，通过 `build.rs` 自动调用 Windows SDK `rc.exe` 编译并链接。
  - 编写 `tools/installer.nsi`，支持简体中文与英文、自定义安装路径、创建桌面与开始菜单快捷方式、注册 `aitoolbox://` 协议，并提供干净的卸载程序（支持清理用户数据选项）。

### 没有 Git 仓库的交付风险
- 问题：项目目录初始并非 Git worktree，无法追踪版本历史。
- 解决：初始化本地 Git 仓库并创建完整基线提交。

### OpenCode 单元测试环境变量并发污染
- 问题：`adapters::opencode::tests::opencode_config_env_override_wins` 在修改 `OPENCODE_CONFIG` 时与 `jsonc_takes_precedence_and_comments_parse` 并发运行导致偶发失败。
- 解决：在并发测试中引入 `ENV_LOCK` 互斥锁，彻底消除并发污染与测试竞态。

### GPUI 托盘应用多窗口截图定位
- 问题：GPUI 创建托盘系统通知图标时生成 50x50 隐藏/辅助窗口，导致基于首个 HWND 的截图脚本截取到任务栏托盘区域。
- 解决：在 `capture-ui.ps1` 中实现 `FindMainWindow`，通过 `EnumWindows` 遍历本进程所有的可视顶级窗口并按最大像素面积进行加权选取，确保始终精准定位主渲染窗口。

### Pi provider 页面为空

- 原因：错误地把 `settings.json` 当 provider 真值源。
- 正确语义：合并 `auth.json`、`models.json.providers`、`settings.json.defaultProvider/defaultModel`。
- 结果：真机发现 13 个 provider，正确识别 `gt-token`。

### Kimi 路径和 schema 错误

- 错误：使用 `~/.kimi`。
- 正确：`~/.kimi-code`，`[models.<key>]`、`[providers.<key>]`，`max_context_size` 必须为正，兜底 262144。

### Grok 配置格式错误

- 错误：写 `config.json`。
- 正确：`config.toml`，`[models].default` 和 `[model.<key>]`。

### OpenClaw 文件名错误

- 错误：`config.json`。
- 正确：`~/.openclaw/openclaw.json`。

### Gemini 陈旧 env 污染

- 原因：只 upsert，不清除旧的 managed keys。
- 解决：固定 14 个 managed env keys，应用前清理；official/custom auth selector 双向规范化。

### Codex TOML fixture 误判

- 测试曾把根 `model_provider` 写在 `[mcp_servers.fs]` 后。
- TOML 语义下它属于该表，不是根键。
- 修复 fixture，并保留真实 root-scalar-first 布局。

### Provider/MCP/Sessions/插件按钮不可见

- 原因：GPUI flex intrinsic width、scroll container 和长中文文本造成横向溢出。
- 解决：两行卡片、独立操作行、左对齐、`flex_wrap`、明确 `min_w(0)`。
- 由 `PrintWindow` 截图验收确认。

### Emoji/符号透明

- 原因：Windows 当前 GPUI 字体链没有对应 glyph fallback。
- 解决：关键动作改成文字按钮或 ASCII，tooltip 保留语义。

### Pi 扩展页冻结

- 原因：`render()` 中同步请求 npm registry，每个 package 最坏 8 秒。
- 解决：列表阶段只读取本地；版本检查拆为后台阶段。

### CLI 插件操作阻塞 UI

- 原因：Pi/OMP/Claude/Grok 安装、更新和卸载同步运行在 GPUI 线程。
- 解决：统一后台执行器，完成后回到 Workspace 更新 Toast。

### 截图抓到后方窗口

- 原因：Windows 前台焦点限制和 DirectComposition。
- 解决：使用目标 HWND 的 `PrintWindow(PW_RENDERFULLCONTENT)`。

### 自动测试启动路由不生效

- 原因：测试运行的是旧 release binary，且 Start-Process 环境传递不稳定。
- 解决：每次截图前重建；使用 `ProcessStartInfo.UseShellExecute=false` 显式传环境变量。

### 工具页面曾被补丁脚本清空两次

- 原因：Python `open(..., 'w', newline='\\n')` 在参数校验失败前先截断文件。
- 恢复：依据上下文完整重建 `tool_page.rs`。
- 预防：大文件修改使用 `write`/`edit`，不再使用会先截断的临时 Python 补丁脚本。

### release 构建被拒绝访问

- 原因：截图测试仍运行 `aitoolplus.exe`，Windows 锁住输出文件。
- 解决：release 重建前先停止进程并等待句柄释放。

### 配置监听测试最初失败

- 原因：PowerShell `Set-Content -Encoding UTF8` 在旧版 Windows PowerShell 写入 BOM，JSON parser 读取失败。
- 解决：测试使用 `UTF8Encoding(false)` 无 BOM 写入。

### 深度链接 IPC 测试最初未传参数

- 原因：旧 PowerShell 不支持 `ProcessStartInfo.ArgumentList`。
- 解决：使用兼容的 `Arguments` 字段并保留独立端到端脚本。

## GPUI 约束

- Entity 必须在 GPUI run/context 内创建。
- 系统对话框必须异步，避免同步模态导致 RefCell 重入。
- Modal 必须挂根层，不能依赖子元素突破父级 clip。
- 不要用 observe 回显输入框，使用 `set_text_silent` 防止事件风暴。
- 网络、CLI、Git 和包操作不能在 render/UI 线程执行。
- 长列表需要固定/可预测布局；图标不能假设 emoji 字体存在。
