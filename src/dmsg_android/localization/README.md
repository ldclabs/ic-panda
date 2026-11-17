# Android localization sources

`catalog.json` maps stable Android resource names to English, Simplified Chinese
and, when the meaning is identical, a public iOS localization key. Locale JSON
files provide Android-specific translations. `plurals.json` defines complete
plural categories separately; exact amounts and identifiers are never converted
through floating point for display.

Run `python3 src/dmsg_android/tools/sync-localizations.py` from the repository root
after editing sources. Generated `strings.xml`, locale configuration and
`coverage.json` are checked in. `--check --complete` rejects stale outputs and
missing translations, and checks positional format arguments for every language.
The CI resources job runs that strict gate before builds. Do not fill missing
languages with English text to make the gate pass.

Shared iOS file hashes in coverage identify the source actually read. They do not
assert that the local checkout is the latest remote version. The app packages
resources for offline selection and has no runtime translation service or model.
Exploratory local machine-translation drafts were rejected and are not inputs to
this generator or files distributed with the app.

Resource completeness and format checks do not establish native-speaker approval
or device layout acceptance. Keep those findings in VALIDATION.md, including RTL,
large text, Activity recreation, system components and actual screenshots.
