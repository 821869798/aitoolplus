use aitoolplus_core::settings::system_is_chinese;

/// Re-export the core enum so every layer agrees on one definition.
pub use aitoolplus_core::settings::Language;

/// Initialize i18n extension with GPUI Component.
pub fn init_i18n() {
    use gpui_kit::component as gpui_component;
    rust_i18n::extend!(gpui_component);
}

/// Set active application locale in rust-i18n and gpui-kit.
pub fn set_locale_from_language(lang: Language) {
    let code = match lang {
        Language::Zh => "zh-CN",
        Language::En => "en",
        Language::System => {
            if system_is_chinese() {
                "zh-CN"
            } else {
                "en"
            }
        }
    };
    rust_i18n::set_locale(code);
    gpui_kit::component::set_locale(code);
}

/// Resolved translator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct I18n {
    is_zh: bool,
}

impl I18n {
    pub fn new(lang: Language) -> Self {
        set_locale_from_language(lang);
        let is_zh = match lang {
            Language::Zh => true,
            Language::En => false,
            Language::System => system_is_chinese(),
        };
        Self { is_zh }
    }

    pub fn is_zh(&self) -> bool {
        self.is_zh
    }

    pub fn locale(&self) -> &'static str {
        if self.is_zh {
            "zh-CN"
        } else {
            "en"
        }
    }

    /// Primary lookup: resolves a key from the GPUI Kit locale YAML file.
    pub fn t<'a>(&self, key: &'a str) -> gpui::SharedString {
        let loc = self.locale();
        if let Some(val) = crate::_rust_i18n_try_translate(loc, key) {
            val.into_owned().into()
        } else {
            key.to_string().into()
        }
    }

    /// Runtime bilingual fallback helper when text is formatted/computed dynamically.
    pub fn raw<'a>(&self, zh: impl AsRef<str>, en: impl AsRef<str>) -> gpui::SharedString {
        if self.is_zh {
            zh.as_ref().to_string().into()
        } else {
            en.as_ref().to_string().into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_languages_resolve() {
        assert!(I18n::new(Language::Zh).is_zh());
        assert!(!I18n::new(Language::En).is_zh());
        assert_eq!(I18n::new(Language::Zh).raw("中", "En"), "中");
        assert_eq!(I18n::new(Language::En).raw("中", "En"), "En");
    }

    #[test]
    fn test_rust_i18n_lookup() {
        let zh = I18n::new(Language::Zh);
        assert_eq!(zh.t("layout.settings"), "系统设置");
        assert_eq!(rust_i18n::t!("layout.settings", locale = "zh-CN"), "系统设置");
        assert_eq!(zh.t("settings_general.theme_mode"), "界面主题");
        assert_eq!(zh.t("settings_general.appearance_language"), "外观与语言");
        assert_eq!(zh.t("settings_general.system"), "跟随系统");
        assert_eq!(zh.t("settings_general.dark"), "暗色模式");
        assert_eq!(zh.t("settings_general.light"), "亮色模式");
        assert_eq!(zh.t("settings_general.zh_cn"), "简体中文");
        assert_eq!(zh.t("settings_general.english"), "English");

        let en = I18n::new(Language::En);
        assert_eq!(en.t("layout.settings"), "Settings");
        assert_eq!(rust_i18n::t!("layout.settings", locale = "en"), "Settings");
        assert_eq!(en.t("settings_general.theme_mode"), "Theme Mode");
        assert_eq!(en.t("settings_general.appearance_language"), "Appearance & Language");
        assert_eq!(en.t("settings_general.system"), "System");
        assert_eq!(en.t("settings_general.dark"), "Dark");
        assert_eq!(en.t("settings_general.light"), "Light");
        assert_eq!(en.t("settings_general.zh_cn"), "简体中文");
        assert_eq!(en.t("settings_general.english"), "English");
    }
}
