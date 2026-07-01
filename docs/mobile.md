# Mobile companion (Phase 4)

Status: **roadmap / not built here.** Mobile is a separate target that needs an
iOS (Xcode + Apple Developer cert) / Android (SDK + NDK) toolchain not available
in this environment, so it can't be compiled or run from here. The path is real
and the architecture already supports it.

## Why it's low-risk to add later

- The shell is **Tauri 2**, which targets iOS and Android from the *same*
  project as the desktop app — `apps/desktop` becomes the mobile project too.
- The mobile app is a **thin client over the existing sync server** (HTTP +
  WebSocket, already built and tested). It uses the same `budgetcut-core::view`
  DTOs the server computes — **no calc engine ships on the device**, so there's
  no Rust-on-mobile heavy lift for the read/approve flows.

## Setup (when a toolchain is available)

```sh
cd apps/desktop
npm run tauri ios init        # or: android init
npm run tauri ios dev         # run on simulator/device
npm run tauri ios build
```

## Scope (per the brief)

Mobile is **read-heavy + approvals + light edits**, not full budgeting —
serious budgeting stays on the desktop. Good mobile surfaces: topsheet/summary
viewing, PO/expense approvals, presence, light line edits.

## App Store Review 4.2 (the brief's flagged risk)

Apple rejects "thin WebView wrappers." Mitigations, all available to a Tauri app:
plan **genuinely native surfaces** (native navigation, share sheet, push
notifications via the OS, biometric unlock, offline cache), not just a website
in a frame. Budget the review risk; ship native-feeling approval/notification
flows rather than a wrapped web page.
