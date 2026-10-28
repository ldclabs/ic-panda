import XCTest
@testable import dMsg

final class LocalizationTests: XCTestCase {
    private let languages = ["en", "zh-Hans", "zh-Hant", "fr", "es", "ru", "ar", "hi", "id", "pt-BR", "ja", "de", "ko", "it", "tr", "vi", "th", "fil", "ms", "bn", "ur", "pl", "nl"]

    func testBundleContainsEveryInterfaceLanguageAndOfflinePolicy() throws {
        for language in languages {
            XCTAssertTrue(Bundle.main.localizations.contains(language), language)
            let path = try XCTUnwrap(Bundle.main.path(forResource: language, ofType: "lproj"), language)
            let bundle = try XCTUnwrap(Bundle(path: path), language)
            let label = bundle.localizedString(forKey: "Language", value: nil, table: nil)
            XCTAssertFalse(label.isEmpty, language)
            if language != "en" { XCTAssertNotEqual(label, "Language", language) }
            for key in ["Welcome back.", "Unlock with system authentication", "Authorize this device"] {
                let value = bundle.localizedString(forKey: key, value: nil, table: nil)
                if language != "en" { XCTAssertNotEqual(value, key, "\(language): \(key)") }
            }
            let policy = language == "zh-Hans" ? "zh" : language
            XCTAssertNotNil(Bundle.main.url(forResource: "privacy-" + policy, withExtension: "md"), language)
        }
        XCTAssertFalse(Bundle.main.localizations.contains("zh-TW"))
        XCTAssertFalse(Bundle.main.localizations.contains("zh-HK"))
    }

    func testSystemMatchesRegionalPreferencesAndTraditionalChinese() {
        let cases = [
            "zh-TW": "zh-Hant", "zh-HK": "zh-Hant", "zh-MO": "zh-Hant",
            "zh-Hant": "zh-Hant", "zh-CN": "zh-Hans", "zh-SG": "zh-Hans",
            "zh-Hans-HK": "zh-Hans", "zh-Hant-CN": "zh-Hant",
            "hi-IN": "hi", "id-ID": "id", "pt-BR": "pt-BR", "pt-PT": "pt-BR",
            "ja-JP": "ja", "de-AT": "de", "ko-KR": "ko", "it-IT": "it", "tr-TR": "tr",
            "vi-VN": "vi", "th-TH": "th", "fil-PH": "fil", "tl-PH": "fil",
            "ms-MY": "ms", "ms-BN": "ms", "bn-BD": "bn", "bn-IN": "bn",
            "ur-PK": "ur", "ur-IN": "ur", "pl-PL": "pl", "nl-NL": "nl", "nl-BE": "nl"
        ]
        for (preference, expected) in cases {
            XCTAssertEqual(Bundle.preferredLocalizations(from: languages, forPreferences: [preference]).first, expected, preference)
        }
        XCTAssertEqual(Bundle.preferredLocalizations(from: languages, forPreferences: ["sv-SE", "ja-JP", "en"]).first, "ja")
        XCTAssertEqual(Bundle.preferredLocalizations(from: languages, forPreferences: ["en", "zh-Hant"]).first, "en")
    }

    func testPluralResourcesFormatCountsWithNativeRules() throws {
        let cases: [(String, Int64, String)] = [
            ("en", 1, "1 download session"), ("en", 2, "2 download sessions"),
            ("ru", 2, "2 сеанса скачивания"), ("ru", 5, "5 сеансов скачивания"),
            ("ar", 0, "لا توجد جلسات تنزيل"), ("ar", 1, "جلسة تنزيل واحدة"),
            ("ar", 2, "جلستا تنزيل")
        ]
        for (language, count, expected) in cases {
            let path = try XCTUnwrap(Bundle.main.path(forResource: language, ofType: "lproj"))
            let bundle = try XCTUnwrap(Bundle(path: path))
            let format = bundle.localizedString(forKey: "%lld download sessions", value: nil, table: nil)
            XCTAssertEqual(String(format: format, locale: Locale(identifier: language), count), expected, language)
        }
        // Exercise the system's cardinal rules, including Polish 1/2/5/12/22
        // and languages whose nouns stay unchanged after a count.
        let expanded: [(String, Int64, String)] = [
            ("pl", 1, "sesja pobierania"), ("pl", 2, "sesje pobierania"),
            ("pl", 5, "sesji pobierania"), ("pl", 12, "sesji pobierania"),
            ("pl", 22, "sesje pobierania"),
            ("nl", 1, "downloadsessie"), ("nl", 2, "downloadsessies"),
            ("vi", 2, "phiên tải xuống"), ("th", 2, "รอบดาวน์โหลด"),
            ("fil", 4, "session ng pag-download"), ("fil", 7, "session ng pag-download"),
            ("ms", 2, "sesi muat turun"), ("bn", 0, "ডাউনলোড সেশন"),
            ("bn", 2, "ডাউনলোড সেশন"), ("ur", 1, "ڈاؤن لوڈ سیشن"),
            ("ur", 2, "ڈاؤن لوڈ سیشن")
        ]
        for (language, count, noun) in expanded {
            let path = try XCTUnwrap(Bundle.main.path(forResource: language, ofType: "lproj"))
            let bundle = try XCTUnwrap(Bundle(path: path))
            let locale = Locale(identifier: language)
            let format = bundle.localizedString(forKey: "%lld download sessions", value: nil, table: nil)
            let number = String(format: "%lld", locale: locale, count)
            XCTAssertEqual(String(format: format, locale: locale, count), "\(number) \(noun)", language)
        }
        let path = try XCTUnwrap(Bundle.main.path(forResource: "en", ofType: "lproj"))
        let bundle = try XCTUnwrap(Bundle(path: path))
        let format = bundle.localizedString(forKey: "%llu days", value: nil, table: nil)
        XCTAssertEqual(String(format: format, locale: Locale(identifier: "en"), UInt64(2)), "2 days")
    }
}
