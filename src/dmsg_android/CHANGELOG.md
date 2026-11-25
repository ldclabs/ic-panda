# dMsg for Android changelog

The app version and build number are set in [app/build.gradle.kts](app/build.gradle.kts).

## 1.0.0

### Build 10 — 2026-11-25

- Enter an access code on the welcome screen to open a separate encrypted
  cloud workspace with sample vault entries and files. If setup is
  interrupted, it resumes the same redemption and adds the sample data
  once membership is confirmed.
- Redeem a promotional code in Plan and usage. Check the recipient account,
  plan and duration before confirming, and retry a pending redemption.
- Open the account deletion guide from About or the account deletion
  screen to review the steps and proof of account ownership.
- Remove multisig wallet actions from the app and wallet slots and
  activation fees from the plan comparison. Account cleanup and handover
  still show existing wallet references as text.
- Keep the image layout steady while the next preview loads and restore
  system bar icon colors when the preview closes.

### Build 9 — 2026-11-25

- Search the current workspace or all workspaces with pinyin and initials.
  Open a highlighted message result in its conversation and read earlier or
  later messages from there. The Autofill picker also matches pinyin.
- Browse files and conversation attachments in a full-screen preview.
  Filter by file type, zoom images and read formatted Markdown. Sharing
  uses the selected file version, including the original bytes for text.
- Read long filenames and conversation titles in the redesigned detail
  screens. File import progress and form actions stay within reach, and
  letter actions stack at larger font sizes.
- See copy confirmation beside the value and track when a vault one-time
  code expires with its countdown ring. Copies from decrypted file previews
  use the private clipboard.
