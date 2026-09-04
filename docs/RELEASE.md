# 发行清单

> 当前尚未完成正式发行。本文件定义发布前必须满足的条件。

## 当前已有

- Windows release EXE 可构建。
- Clippy 零警告。
- 170 个测试。
- 页面截图和系统真机脚本。
- 更新检查和资产下载逻辑。
- `aitoolbox://` 协议运行时注册。

## 发布前必做

### 版本控制

- [x] 初始化或迁入 Git 仓库。
- [x] 审查完整 diff。
- [ ] 添加 LICENSE。
- [ ] 创建版本 tag。
- [ ] 生成变更日志。

### Windows 资源

- [x] 正式 `.ico`（256x256 高清多分辨率）。
- [x] EXE product/file version（0.1.0.0）。
- [x] Company/Product/Copyright metadata（嵌入 VERSIONINFO）。
- [x] 托盘和窗口图标验证。

### 安装器

- [x] 选择 MSI、WiX、NSIS 或 cargo-wix（选定 NSIS，提供 `tools/installer.nsi`）。
- [x] 安装到用户目录（`$LOCALAPPDATA\Programs\AIToolPlus`，免管理员权限）。
- [x] 开始菜单/桌面快捷方式。
- [x] 注册 `aitoolbox://`。
- [x] 配置开机自启选择。
- [x] 卸载时删除协议和 Run key。
- [x] 询问是否保留 `%APPDATA%\aitoolplus`。

### 签名

- [ ] Windows Authenticode 证书。
- [ ] 签名 EXE 和安装器。
- [ ] 验证时间戳服务器。
- [ ] SmartScreen 基础验证。

### 更新

- [ ] 确认实际 GitHub Releases 仓库。
- [ ] 修正 Cargo repository/update API。
- [ ] 发布 Windows asset 命名规范。
- [x] 下载后启动安装器。
- [x] 退出当前实例。
- [x] 更新失败回滚与脱机替换脚本支持。
- [x] SHA-256 校验。

### 安全

- [x] WebDAV 密码与 S3 密钥迁移 Windows DPAPI 加密存储。
- [x] 自定义绝对路径恢复确认与沙箱隔离。
- [x] 覆盖/跳过/另存冲突策略。
- [ ] 备份加密方案（压缩包密码）。
- [ ] 深度链接 import 二次确认 UI（当前直接导入）。

### 真机矩阵

- [ ] Claude Code
- [ ] Codex
- [ ] Gemini CLI
- [ ] Grok
- [ ] Kimi
- [ ] OpenCode
- [ ] OpenClaw
- [ ] Pi
- [ ] Oh My Pi
- [ ] Claude Desktop
- [ ] Hermes
- [ ] DSH
- [ ] Oh My OpenAgent
- [ ] Oh My OpenCode Slim
- [ ] 真实 WebDAV：Nextcloud/坚果云/群晖至少两种

### UI

- [ ] 100% / 125% / 150% DPI。
- [ ] 1080p / 2K / 4K。
- [ ] 深色主题全部页面。
- [ ] 中英文全部页面。
- [ ] 键盘导航和 Focus。
- [ ] 屏幕阅读器基本语义。
- [ ] 全部表单点击自动化。
- [ ] 危险操作确认框。

## 发布门禁

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

系统脚本：

```powershell
powershell -ExecutionPolicy Bypass -File tools/test-close-to-tray.ps1
powershell -ExecutionPolicy Bypass -File tools/test-deeplink-ipc.ps1
powershell -ExecutionPolicy Bypass -File tools/test-config-watch.ps1
```

发布时还必须在干净 Windows VM 中安装、启动、升级、卸载各测试一次。
