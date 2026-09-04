//! Bilingual UI text. All strings go through `t(zh, en)` at the call site;
//! this module only resolves the effective language.

use aitoolplus_core::settings::system_is_chinese;

/// Re-export the core enum so every layer agrees on one definition.
pub use aitoolplus_core::settings::Language;

/// Resolved translator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct I18n {
    is_zh: bool,
}

impl I18n {
    pub fn new(lang: Language) -> Self {
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

    /// The one true lookup: every UI string is authored bilingually at the
    /// call site, so nothing can be forgotten in a table.
    pub fn t(&self, zh: &str, en: &str) -> gpui::SharedString {
        if self.is_zh { zh.into() } else { en.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_languages_resolve() {
        assert!(I18n::new(Language::Zh).is_zh());
        assert!(!I18n::new(Language::En).is_zh());
        assert_eq!(I18n::new(Language::Zh).t("中", "En"), "中");
        assert_eq!(I18n::new(Language::En).t("中", "En"), "En");
    }
}
