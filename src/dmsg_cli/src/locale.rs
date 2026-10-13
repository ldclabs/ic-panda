//! The language of the CLI's own text: Simplified Chinese when the locale
//! (`LC_ALL`, else `LC_MESSAGES`, else `LANG`) starts with `zh`, otherwise
//! English. `--json` output, error codes and the daemon's messages stay as
//! they are.

use std::sync::OnceLock;

/// Whether the CLI speaks Chinese.
pub(crate) fn zh() -> bool {
    static ZH: OnceLock<bool> = OnceLock::new();
    *ZH.get_or_init(|| chinese(|name| std::env::var(name).ok()))
}

/// The first locale variable that is set and not empty decides, as in POSIX.
fn chinese(var: impl Fn(&str) -> Option<String>) -> bool {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| var(name).filter(|v| !v.is_empty()))
        .is_some_and(|v| v.starts_with("zh"))
}

/// `format!` with the English or the Chinese template.
macro_rules! tr {
    ($en:literal, $zh:literal $(, $arg:expr)* $(,)?) => {
        match $crate::locale::zh() {
            true => format!($zh $(, $arg)*),
            false => format!($en $(, $arg)*),
        }
    };
}
pub(crate) use tr;

/// A line of a review: the label in nine columns, then the value.
pub(crate) fn row(en: &str, zh_label: &str, value: impl std::fmt::Display) -> String {
    let label = if zh() { zh_label } else { en };
    let width: usize = label
        .chars()
        .map(|c| if c.is_ascii() { 1 } else { 2 })
        .sum();
    format!(
        "{label}{} {value}",
        " ".repeat(9usize.saturating_sub(width))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_language_like_posix() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert!(chinese(env(&[("LANG", "zh_CN.UTF-8")])));
        assert!(chinese(env(&[
            ("LC_ALL", "zh_TW.UTF-8"),
            ("LANG", "en_US.UTF-8")
        ])));
        assert!(!chinese(env(&[("LC_ALL", "C"), ("LANG", "zh_CN.UTF-8")])));
        assert!(chinese(env(&[("LC_ALL", ""), ("LANG", "zh_CN.UTF-8")])));
        assert!(!chinese(env(&[])));
    }
}
