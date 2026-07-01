# Packaging & distribution

What can be produced locally vs. what needs credentials/runners.

## macOS (works locally now)

```sh
cd apps/desktop
npm install
npm run tauri build       # → src-tauri/target/release/bundle/{macos/BudgetCut.app, dmg/*.dmg}
```

The DMG is **unsigned** — recipients open it with **right-click → Open** once. To
ship without that prompt you need an **Apple Developer account** ($99/yr): a
"Developer ID Application" certificate + notarization. That requires secrets
this environment doesn't have, so it's a CI step (below), not a local one.

## Cross-platform (CI only)

Windows (`.msi`/`.exe`) and Linux (`.deb`/`.AppImage`) bundles must be built on
their own runners — they can't be cross-compiled from macOS here. Use the
official `tauri-apps/tauri-action` on a GitHub matrix (`.github/workflows/release.yml`).

### Signing / notarization secrets (set in repo → Settings → Secrets)
- **macOS:** `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`,
  `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID`.
- **Windows:** a code-signing cert (`WINDOWS_CERTIFICATE` + password) or Azure Trusted Signing.
- Tauri's updater (optional): `TAURI_SIGNING_PRIVATE_KEY` + password.

Without the Apple secrets the workflow still builds installers — just unsigned.

## Status

| Target | Local here | CI (with secrets) |
|---|---|---|
| macOS `.app` + `.dmg` (unsigned) | ✅ done — see [TRYIT.md](../TRYIT.md) | ✅ |
| macOS signed + notarized | ✗ needs Apple Developer cert | ✅ |
| Windows `.msi` | ✗ needs Windows runner | ✅ |
| Linux `.deb` / `.AppImage` | ✗ needs Linux runner | ✅ |

Mobile (iOS/Android) packaging is tracked separately — see [docs/mobile.md](mobile.md).
