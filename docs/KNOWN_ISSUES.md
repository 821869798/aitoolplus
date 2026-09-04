# 已知问题、风险与开发事故

> 这里同时记录尚未解决的问题和已经解决但值得保留的事故/经验，防止后续重复踩坑。

## 尚未解决

### S3 未实现

- 当前远端备份只有 WebDAV。
- 上游设置中存在 S3 配置。
- 需要实现 SigV4、endpoint/region/bucket/path-style、连接测试、上传/列举/下载/删除和真实兼容测试。

### 自定义绝对恢复路径不完整

- 自定义文件/目录可备份。
- 默认恢复到 `appdata/restored-custom/` 安全沙箱。
- 绝对恢复路径需要恢复前预览、逐项确认和冲突策略，目前会被安全跳过。

### 更新不会自动安装

- 已实现检查、版本比较、release notes、资产选择和下载。
- 未实现退出后替换 EXE、启动 MSI/NSIS、失败回滚和签名校验。
- 默认 GitHub 仓库地址是项目占位地址，正式发布前必须确认实际 Releases 仓库。

### WebDAV 凭据明文

- 密码当前保存在 `settings.json`。
- 正式版本应迁移到 Windows Credential Manager/macOS Keychain/Linux Secret Service。

### 完整 UI 自动化不足

- 有页面启动、截图和系统级脚本。
- Windows 前台焦点限制导致普通鼠标注入不可靠。
- 还没有自动填写/点击所有表单和确认框的测试框架。

### 真机覆盖不完整

- 本机缺 OMP、Grok、Kimi、OpenClaw、Hermes、DSH、Claude Desktop 完整运行环境。
- 这些模块主要依靠夹具与 round-trip 测试。

### 跨平台未验收

- macOS/Linux 托盘、启动项、打包和 Skills symlink 未测试。
- 当前正式支持目标应写作 Windows 开发版。

### 没有 Git 仓库

- 当前目录不是 Git worktree，无法通过 `git status`、diff、commit 或回滚管理变更。
- 这是交付风险。正式继续开发前建议初始化 Git 或放入已有仓库。

## 已解决的重要问题

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
