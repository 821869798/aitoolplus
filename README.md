<div align="center">
  <p><img src="assets/aitoolplus_icon.png" alt="AIToolPlus Logo" width="108" /></p>
  <h1>AIToolPlus</h1>
  <p><strong>A High-Performance Native AI Coding Assistant Configuration Workbench &amp; Workspace</strong></p>
  <p>GPU Hardware-Accelerated Native Rendering · Sub-100ms Cold Start · Ultra-Low Memory · Zero Electron or WebView Overhead</p>

  <p>
    <a href="https://github.com/aitoolplus/aitoolplus/releases/latest"><img src="https://img.shields.io/github/v/release/aitoolplus/aitoolplus?style=for-the-badge&color=2563eb" alt="Latest Release" /></a>
    <a href="https://github.com/aitoolplus/aitoolplus/releases"><img src="https://img.shields.io/github/downloads/aitoolplus/aitoolplus/total?style=for-the-badge&color=10b981" alt="Downloads" /></a>
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-2024%20(1.91+)-dea584?style=for-the-badge&logo=rust&logoColor=white" alt="Rust Edition 2024" /></a>
    <a href="https://github.com/zed-industries/zed"><img src="https://img.shields.io/badge/UI-GPUI%20Kit-7c3aed?style=for-the-badge" alt="GPUI Kit" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPL--3.0--or--later-blue?style=for-the-badge" alt="License: GPL-3.0-or-later" /></a>
  </p>

  <p>
    <a href="./README.md">English</a> ·
    <a href="./README_ZH.md">简体中文</a> ·
    <a href="#installation--download">Installation</a> ·
    <a href="#screenshots">Screenshots</a> ·
    <a href="#highlights--key-features">Key Features</a> ·
    <a href="#technology-stack">Tech Stack</a> ·
    <a href="#development--building">Development</a> ·
    <a href="#acknowledgements">Acknowledgements</a> ·
    <a href="https://github.com/aitoolplus/aitoolplus/releases/latest">Latest Release</a>
  </p>

  <p><img src="docs/screenshots/verify-sidebar-and-providers-v2.png" alt="AIToolPlus Overview Banner" width="860" /></p>
</div>

`AIToolPlus` is a **native, ultra-fast desktop workbench designed for developers using AI coding assistants**. It addresses fragmented configurations across multiple AI CLI tools (Claude Code, Codex, Gemini CLI, Pi, Grok, OpenCode, etc.), tedious endpoint switching, lack of real-time Token and cost monitoring, and quota exhaustion issues in multi-account environments.

Built with **pure Rust** and powered by the **GPUI Kit** framework (the same engine backing the Zed editor), AIToolPlus delivers instant UI responsiveness, GPU hardware-accelerated rendering, and a fraction of the system memory footprint compared to Electron or WebView-based alternatives.

---

## Supported AI Ecosystems

`AIToolPlus` natively integrates and supports the following AI coding CLI tools and runtime environments:

| Tool / Environment | Core Capabilities |
| :--- | :--- |
| **Claude Code** | Provider presets, endpoint switching, model mappings, prompt injection, plugin lifecycle, rich session parsing |
| **Codex** | TOML config management, model redirection, plugin manager, and MCP interoperability |
| **Gemini CLI** | Google AI provider configuration, environment variables, latency testing |
| **Grok** | Dedicated provider management, endpoint routing, session tree navigation, and cache clearing |
| **Kimi** | Kimi assistant configuration sync and endpoint management |
| **OpenCode / OpenClaw** | Provider hierarchy, local add-on bridging, and preset application |
| **Pi / Oh My Pi (OMP)** | Local extension scanning & protection, composite settings merge, multi-package retention |
| **Claude Desktop** | Multi-profile switching and native configuration monitoring |
| **Hermes / DSH** | Memory settings, environment variable overrides |
| **Antigravity (Dedicated)** | Multi-account quota tracking, Gemini 5-hour/weekly quotas, Claude/GPT quotas, exhaustion alerts |

---

## Installation & Download

> [!IMPORTANT]
> **Official Distribution Security Notice**: Please only download installers from official GitHub Releases. Never run modified binaries from untrusted third-party sources to ensure your API keys and credentials remain secure.

### Windows Official Installer

Download the official setup installer directly from the [GitHub Releases Page](https://github.com/aitoolplus/aitoolplus/releases/latest):
- File: `aitoolplus-setup.exe`
- Built with standard NSIS installer packaging, featuring smooth upgrades, clean uninstallation, and zero registry bloat.
- **In-App Auto Update**: Features one-click update checks, real-time download progress with EMA smoothing, and domestic CDN mirror acceleration.

---

## Screenshots

| Provider Management & Workspace | Antigravity Multi-Account 3-Column Quota |
| :---: | :---: |
| ![Providers & Workspace](docs/screenshots/verify-sidebar-and-providers-v2.png) | ![Antigravity Quota](docs/screenshots/verify-antigravity-3col-fixed.png) |
| **Token Usage Analytics & Smooth Trend Chart** | **Skills Store & Multi-Repo Git Manager** |
| ![Usage Analytics & Trend](docs/screenshots/verify-usage-cc-switch-final.png) | ![Skills Repo Manager](docs/screenshots/verify-skills-repo-manager.png) |
| **MCP Server Management & Live Validation** | **App Updater & Domestic CDN Acceleration** |
| ![MCP Server Management](docs/screenshots/verify-mcp-scroll-top.png) | ![App Updater & CDN Mirrors](docs/screenshots/verify-settings-about-updater.png) |

---

## Highlights & Key Features

### 1. ⚡ Native Performance & Ultra-Low Resource Usage
- **Pure Rust + GPUI Kit**: Replaces heavy Chromium runtimes with direct GPU rendering. Cold starts take less than 100ms, while memory usage remains consistently under 50MB.
- **Non-blocking Concurrency**: Powered by Tokio and non-blocking channels. Network speed testing, file scanning, and config synchronization run smoothly in background threads without UI jitter.

### 2. 🛡️ Antigravity Multi-Account Quota & Exhaustion Alerts
- **Three-Column Information Architecture**:
  - **Column 1 (Gemini Quota)**: Displays 5-hour rolling quota and weekly quota progress bars.
  - **Column 2 (Claude / GPT Quota)**: Displays multi-model quotas and reset countdowns.
  - **Column 3 (Actions)**: Instant account switching, editing, speed tests, and quota refreshes.
- **Weekly Quota Exhaustion Warning**: Real-time tracking of weekly usage. If a weekly quota reaches 0%, a distinct crimson warning badge is applied to prevent workflow interruptions.
- **Multi-Modal Import Ecosystem**: Supports **automated Google Web OAuth**, **RefreshToken instant import**, and 1-click import from existing Antigravity desktop clients.

### 3. 📊 Token Usage Analytics & Cost Trend Curves
- **CC-Switch Algorithm Parity**: Track Token consumption, request volume, and costs across daily, weekly, and monthly dimensions.
- **Smooth Cubic Bézier Trend Line**: Hover and interact with data points to view detailed model breakdowns. Tooltips automatically vanish when the cursor leaves the chart boundary.
- **Custom Model Pricing Multipliers**: Ships with official reference rates and allows customized price multipliers for third-party API proxy gateways.
- **Seamless Data Import**: One-click import for legacy CC-Switch and TokenRouter databases.

### 4. 🧩 MCP Server & Skills Ecosystem
- **MCP Server Controls**: Quick toggle switches with real-time JSON syntax highlighting and schema validation.
- **Multi-Source Skills Management**:
  - Discover skills from the official store, local ZIP archives, or Git repositories (GitHub, Gitee, GitLab, self-hosted).
  - **Windows NTFS Junctions**: Centrally manage skills and project them to Claude Code, Codex, and other tool directories without duplicating files.

### 5. 🚀 In-App Updates with Domestic CDN Mirror Support
- **Dual API Compatibility**: Supports both standard GitHub Releases API and lightweight `latest.json` manifests.
- **Fast Regional Mirrors**: Built-in 1-click switching between `GitHub Official`, `ghproxy.net (Recommended)`, `mirror.ghproxy`, `gh-proxy.com`, and custom CDN proxies, delivering 10–50 MB/s download speeds in mainland China.
- **EMA Smooth Speed Calculation**: Employs an exponential moving average ($\text{speed} = \text{speed} \times 0.7 + \text{speed\_calc} \times 0.3$) to eliminate bandwidth display jitter.
- **Atomic Safe Replacement**: Validates SHA-256 integrity, streams into `.tmp` files, and automatically launches the official NSIS installer.

### 6. 🔒 Data Privacy, Snapshots & Cloud Backup
- **Local Multi-Version Snapshots**: Create full configuration snapshots and rollback at any time.
- **WebDAV Cloud Synchronization**: Compatible with standard WebDAV servers (such as Jianguoyun) for cross-device backup and recovery.
- **Hardware-Level Encryption**: Sensitive credentials and API keys are protected using Windows DPAPI.

---

## Technology Stack

- **Core Language**: [Rust 2024 Edition](https://www.rust-lang.org/) (MSRV 1.91+)
- **UI Framework**: [GPUI Kit](https://github.com/zed-industries/zed) (GPU hardware-accelerated desktop UI engine)
- **Async Runtime**: [Tokio](https://tokio.rs/) & [async-channel](https://crates.io/crates/async-channel)
- **Networking**: [Reqwest](https://crates.io/crates/reqwest) (Connection pooling, streaming chunk downloads, proxy auto-discovery)
- **Serialization**: [Serde](https://serde.rs/), [serde_json](https://crates.io/crates/serde_json), [toml_edit](https://crates.io/crates/toml_edit)
- **Security & Crypto**: [sha2](https://crates.io/crates/sha2), [hmac](https://crates.io/crates/hmac), Windows DPAPI
- **Distribution Packaging**: [NSIS (Nullsoft Scriptable Install System)](https://nsis.sourceforge.io/)

---

## Development & Building

### 1. Prerequisites

- [Rust Toolchain](https://rustup.rs/) (1.91 or later).
- Windows MSVC C++ Build Tools (via Visual Studio Installer).
- [NSIS](https://nsis.sourceforge.io/) installed with `makensis` available in your system `PATH` (for building Windows installer packages).

### 2. Local Testing & Verification

```powershell
# Run local debug build
cargo run -p aitoolplus

# Run all 231 workspace unit tests
cargo test --workspace

# Validate code quality with zero compiler warnings
cargo clippy --workspace --all-targets -- -D warnings
```

### 3. Production Release & Installer Packaging

```powershell
# Compile optimized release binary
cargo build --release

# Generate the official Windows NSIS setup package (outputs to target/dist/aitoolplus-setup.exe)
powershell -ExecutionPolicy Bypass -File tools/build-installer.ps1
```

---

## Acknowledgements

The inception and growth of `AIToolPlus` are inspired by remarkable pioneers in the open-source community. We express our deepest gratitude to:

- **[cc-switch](https://github.com/farion1231/cc-switch)**: Crafted by farion1231, setting the gold standard for Claude/Codex provider management and usage analysis. AIToolPlus is profoundly inspired by its intuitive provider interaction design, Token usage statistics models, and import architecture.
- **[ai-toolbox](https://github.com/coulsontl/ai-toolbox)**: Developed by coulsontl, pioneering a comprehensive desktop workbench for AI coding tools. AIToolPlus drew invaluable insights from its multi-tool lifecycle coverage, updater smoothing algorithms, and unified desktop workspace philosophy.

---

## License

This project is licensed under the [GNU General Public License v3.0 or later (GPL-3.0-or-later)](./LICENSE).
