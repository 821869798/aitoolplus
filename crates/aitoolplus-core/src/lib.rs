//! aitoolplus-core: the UI-independent business core.
//!
//! Everything here is synchronous, `serde`-typed and unit-testable without a
//! window. The GPUI layer consumes these APIs through plain function calls;
//! file IO happens on KB-sized JSON/TOML files, so synchronous reads are fine.

pub mod adapters;
pub mod api_hub;
pub mod backup;
pub mod claude_desktop;
pub mod claude_plugins;
pub mod cli_launch;
pub mod config;
pub mod deeplink;
pub mod dsh;
pub mod grok_plugins;
pub mod hermes;
pub mod import_current;
pub mod mcp;
pub mod oh_my_pi;
pub mod omp_extensions;
pub mod opencode_addons;
pub mod paths;
pub mod pi_extensions;
pub mod pi_pages;
pub mod pi_runtime;
pub mod presets;
pub mod prompt;
pub mod providers;
pub mod session;
pub mod settings;
pub mod skill_git;
pub mod skills;
pub mod store;
pub mod tools;
pub mod updater;
pub mod webdav;

pub use api_hub::{FetchedModel, ModelsFetchResult};
pub use claude_desktop::ClaudeDesktopPaths;
pub use claude_plugins::{InstalledPlugin, KnownMarketplace, MarketplacePlugin};
pub use dsh::DshRuntimePaths;
pub use hermes::HermesRuntimePaths;
pub use oh_my_pi::OmpRuntimePaths;
pub use paths::Paths;
pub use pi_extensions::{PiExtensionListResult, PiExtensionSummary};
pub use pi_pages::PiModelSettings;
pub use pi_runtime::PiRuntimePaths;
pub use presets::ProviderPreset;
pub use tools::ToolId;
