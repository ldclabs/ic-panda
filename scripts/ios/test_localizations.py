"""Check failures that can silently change localized messages or plural behavior."""
import copy
import unittest

from localizations import check_localizable


class LocalizationTests(unittest.TestCase):
    def setUp(self):
        self.strings = {"Language": "Language", "Saved %@": "Saved %@"}
        self.plurals = {"%lld sessions": {
            "NSStringLocalizedFormatKey": "%#@value@",
            "value": {
                "NSStringFormatSpecTypeKey": "NSStringPluralRuleType",
                "NSStringFormatValueTypeKey": "lld",
                "one": "%lld session",
                "other": "%lld sessions",
            },
        }}

    def check(self, strings=None, plurals=None):
        return check_localizable(self.strings, self.plurals,
                                 self.strings if strings is None else strings,
                                 self.plurals if plurals is None else plurals)

    def test_missing_and_empty_translations_are_rejected(self):
        self.assertTrue(any("missing Saved %@" in error for error in self.check({"Language": "Sprache"})))
        self.assertTrue(any("Incomplete Language" in error for error in self.check({**self.strings, "Language": " "})))

    def test_printf_type_and_unexpected_keys_are_rejected(self):
        errors = self.check({**self.strings, "Saved %@": "Saved %lld", "Unused": "Unused"})
        self.assertIn("Format mismatch Saved %@", errors)
        self.assertIn("strings: unexpected Unused", errors)

    def test_arabic_worded_counts_and_additional_plural_forms_are_valid(self):
        translated = copy.deepcopy(self.plurals)
        translated["%lld sessions"]["value"].update({
            "zero": "لا توجد جلسات", "one": "جلسة واحدة", "two": "جلستان",
            "few": "%lld جلسات", "many": "%lld جلسة", "other": "%lld جلسة",
        })
        self.assertEqual(self.check(plurals=translated), [])

    def test_plural_other_and_numeric_type_are_required(self):
        translated = copy.deepcopy(self.plurals)
        rule = translated["%lld sessions"]["value"]
        del rule["other"]
        rule["NSStringFormatValueTypeKey"] = "@"
        errors = self.check(plurals=translated)
        self.assertTrue(any("Invalid plural forms" in error for error in errors))
        self.assertTrue(any("Plural type mismatch" in error for error in errors))

    def test_plural_format_and_parameters_must_match(self):
        translated = copy.deepcopy(self.plurals)
        translated["%lld sessions"]["NSStringLocalizedFormatKey"] = "%#@missing@"
        translated["%lld sessions"]["value"]["other"] = "%@ sessions"
        errors = self.check(plurals=translated)
        self.assertIn("Plural format mismatch %lld sessions", errors)
        self.assertIn("Plural format mismatch %lld sessions/value/other", errors)


if __name__ == "__main__":
    unittest.main()
