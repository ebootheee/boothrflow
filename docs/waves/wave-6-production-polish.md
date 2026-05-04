# Wave 6 — Production Polish

**Goal:** turn boothrflow from "works on Eric's laptop in dev mode"
into "anyone can download a signed installer, get prompted by the
real app for permissions, and stay current via auto-update." Six
phases, each independently shippable.

After Wave 6, every subsequent feature follows a **staging → stable**
cadence (a `beta` update channel ships first, then promotes to
`stable` after soak). The cadence itself is part of this wave —
without code signing + auto-update, there's no real way to run two
channels.

## Why this matters

Three concrete frictions Eric hit during Wave 5 UAT that production
polish resolves:

1. **TCC permission attribution dance.** In `pnpm dev` mode the
   parent terminal owns Microphone / Accessibility / Input
   Monitoring / Screen Recording, not the app. boothrflow doesn't
   appear in System Settings → Privacy & Security panes; granting
   permission in the wrong row does nothing useful. A signed +
   notarized bundle is attributed to itself, so the panes show
   `boothrflow.app` and toggling it actually works.

2. **No way to give the build to anyone.** Currently anyone wanting
   to try the app has to clone the repo, install Rust + Node + Ollama,
   wait for whisper.cpp + sherpa-onnx to compile, run `pnpm dev:parakeet`,
   and grant TCC permissions to their terminal. That's a multi-hour
   onboarding for a tool whose value prop is "fast push-to-talk
   dictation."

3. **No upgrade path.** Even on Eric's machine, picking up new
   features means `git pull && cargo build`. After this wave, fixes
   land via auto-update.

---

## Phase 1 — Versioning + release infrastructure (1-2 days)

Foundation for everything below. No user-facing changes; this is
the plumbing that makes signing + auto-update possible.

### Deliverables

- **`VERSION` file** at repo root, source of truth for the SemVer
  string. Currently `Cargo.toml`'s `version = "0.0.0"` and
  `package.json`'s `"version": "0.0.0"` are the only version
  surfaces — both bumped manually, easy to drift. Centralize.
- **`scripts/version.sh`** that:
  - Reads `VERSION`, validates SemVer.
  - Writes the version into `src-tauri/Cargo.toml`,
    `src-tauri/tauri.conf.json` (`"version"` field),
    `package.json`, and `src-tauri/src/commands.rs`'s `app_version`
    response if it stops being driven by `CARGO_PKG_VERSION`.
  - Idempotent — running twice with the same VERSION is a no-op.
- **GitHub Actions release workflow** (`.github/workflows/release.yml`):
  - Trigger on tag push `v*.*.*`.
  - Matrix build: macOS-arm64, macOS-x64 (universal binary), Windows-x64.
  - Outputs: `.dmg` (macOS), `.msi` (Windows), `latest.json`
    (Tauri updater manifest).
  - Uploads to GitHub Release matching the tag.
- **`RELEASING.md`** at repo root: the playbook. SemVer rules, when
  to cut a beta vs stable, how to write release notes.
- **CHANGELOG.md → release notes mapping.** Each release pulls the
  latest `## YYYY-MM-DD` block from CHANGELOG.md and appends
  install instructions.

### Risks

- **Tauri 2 universal-binary builds on macOS** require extra config
  (`bundle.macOS.providerShortName` + `targets: ["aarch64-apple-darwin",
"x86_64-apple-darwin", "universal-apple-darwin"]`). First release
  may need a follow-up to fix architecture-specific issues with
  `whisper-rs` (Metal feature gate is aarch64-only).
- **GitHub Actions runners on macOS** have ~1.5h per-job time
  budgets. Whisper.cpp + (optionally) sherpa-onnx + ONNX runtime
  compile is ~10-20 min cold; should fit but warrants caching the
  cargo registry + target dir between runs.

### Acceptance

- `git tag v0.1.0-beta.1 && git push --tags` triggers a release
  build that produces unsigned `.dmg` + `.msi` + `latest.json` on
  the GitHub Release page.

---

## Phase 2 — macOS code signing + notarization (1 day)

Apple Developer ID + the notary service. Replaces the "in dev mode
TCC is owned by the parent terminal" dance with proper app
attribution.

### Deliverables

- **Developer ID Application cert** in the macOS Keychain on
  the build machine (Eric's local first; GitHub Actions secrets
  later).
- **`tauri.conf.json` `bundle.macOS.signingIdentity`** set to the
  Developer ID common name (e.g. `Developer ID Application: Eric
Boothe (TEAMID)`).
- **`tauri.conf.json` `bundle.macOS.entitlements`** pointing at
  `src-tauri/entitlements.plist` with the entitlements we need:
  - `com.apple.security.device.audio-input` (Microphone)
  - `com.apple.security.automation.apple-events` (Accessibility
    paste, if enigo needs it)
  - No `com.apple.security.cs.disable-library-validation` —
    minimum-privilege.
- **Notary submission via `notarytool`** wired into the GitHub
  Actions workflow:
  - Secrets: `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `TEAM_ID`.
  - `xcrun notarytool submit ... --wait` blocks until Apple's
    notary service rules pass/fail.
  - On success, `xcrun stapler staple` writes the ticket into the
    `.dmg` so it's offline-verifiable.
- **First-launch test:** download the signed+notarized `.dmg` from
  GitHub Releases on a clean Mac, drag to Applications, launch.
  - macOS does not show the "unidentified developer" warning.
  - First mic capture prompts against `boothrflow.app` (not
    Terminal).
  - boothrflow.app appears in System Settings → Privacy & Security
    → Microphone (and the other panes).

### Risks

- **`enigo` requires Accessibility permission**, which TCC grants
  to a code-signed app. Without notarization, the OS may not
  surface boothrflow in the Accessibility list reliably. Need to
  test on a clean macOS install (or new VM) — Eric's existing TCC
  state is muddled from dev-mode grants.
- **Hardened Runtime + JIT.** ONNX Runtime (used by sherpa-onnx /
  Parakeet) sometimes needs `com.apple.security.cs.allow-jit`. If
  we hit that during smoke test, add the entitlement + document why.
- **Apple cert costs $99/year** + the personal info hassle if the
  cert isn't already on the team. Confirm Eric has an active
  Developer Program membership before kicking this off.

### Acceptance

- Signed `.dmg` available on GitHub Releases.
- `spctl -a -t exec -vv /Applications/boothrflow.app` returns
  `accepted` and `source=Notarized Developer ID`.
- Fresh-install boots, prompts for permissions correctly, dictates
  successfully end-to-end.

---

## Phase 3 — Auto-update wiring (1 day)

**Why before Windows signing:** auto-update on an unsigned macOS app
is broken UX — every update redownloads the bundle, Gatekeeper sees
a "new app," and the user has to do the Privacy & Security
"Open Anyway" dance on every release. So as soon as macOS signing
lands in Phase 2, wire auto-update immediately. That gives Eric a
real upgrade path on his daily driver before we touch Windows.

Tauri's updater plugin + GitHub Releases as the manifest server.

### Deliverables

- **`tauri-plugin-updater` initialized** in `lib.rs::run`.
- **Update endpoint:** `https://github.com/ebootheee/boothrflow/releases/latest/download/latest.json`.
  The `latest.json` is generated by the release workflow (Phase 1)
  and contains:
  ```json
  {
    "version": "0.1.0",
    "notes": "...",
    "pub_date": "2026-...",
    "platforms": {
      "darwin-aarch64": { "signature": "...", "url": "..." },
      "darwin-x86_64": { "signature": "...", "url": "..." },
      "windows-x86_64": { "signature": "...", "url": "..." }
    }
  }
  ```
- **Update signing keypair.** Tauri requires update artifacts to be
  signed with a key separate from the code-signing identity (so a
  compromised release server can't push a malicious update). Generate
  via `pnpm tauri signer generate`. Public key in `tauri.conf.json`;
  private key in GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY`.
- **Settings → About → "Check for updates" button** with status:
  `Up to date · v0.1.0`, `Update available — v0.1.1`, or
  `Checking…`. On macOS, downloaded updates install on next launch
  via the `tauri::updater::UpdaterBuilder` flow.
- **Update channel selector** in Settings → About: `Stable` (default)
  / `Beta`. Beta channel pulls from a separate `latest-beta.json`
  manifest. Phase 6 makes use of this; Phase 3 just wires the
  plumbing.
- **Background check** on app launch (and every ~6h while running):
  if there's an update, show a small badge on the tray icon and a
  one-time toast. Never auto-installs without user consent.
- **Windows entry in `latest.json` is empty until Phase 4 lands.**
  The macOS-only stretch is fine — Eric is on macOS and the macOS
  cohort gets working updates immediately. Windows users on early
  releases stay on whatever build they installed; once signing lands
  the manifest auto-includes them.

### Risks

- **macOS bundle remount races** during the update install can
  brick the app if interrupted. Tauri's plugin handles this
  correctly but we should test interruption (kill the updater
  mid-download).
- **Update signing key compromise** would let an attacker push
  malicious updates to all users. Store the private key in the
  GitHub Actions secret, never commit, rotate yearly.

### Acceptance

- Cut v0.1.0-beta.1 (signed for macOS via Phase 2) → v0.1.0-beta.2.
- Run beta.1 locally; "Check for updates" finds beta.2; install
  succeeds; relaunch shows beta.2.

---

## Phase 4 — Windows code signing (1-2 days)

Azure Trusted Signing (cheap path) or an EV code-signing cert.
Avoids SmartScreen scaring users away from the `.msi`.

### Deliverables

- **Azure Trusted Signing account + signing identity** registered.
  Cheaper than a traditional EV cert (~$10/mo vs ~$300/yr) and
  Microsoft itself attests to the publisher — better SmartScreen
  reputation out of the gate.
- **`tauri.conf.json` `bundle.windows.signCommand`** points at
  `signtool.exe` invoking the Azure Trusted Signing dlib.
- **GitHub Actions secrets:** `AZURE_TENANT_ID`,
  `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_CODESIGN_ACCOUNT`,
  `AZURE_CODESIGN_PROFILE`.
- **First-launch test:** download `.msi` from GitHub Releases on
  a clean Windows VM. Run.
  - SmartScreen does not block (publisher reputation accumulates
    over a few signed releases — first release may still get a
    "More info → Run anyway" prompt; expected).
  - First hotkey press doesn't trigger UAC weirdness.

### Risks

- **EV cert alternative** if Azure Trusted Signing onboarding
  drags. Sectigo EV: ~$300/yr, USB-token-based historically (now
  HSM-cloud). Slower but turn-key.
- **Antivirus false positives.** rdev's low-level keyboard hook
  is statistically how keyloggers work. Even a signed binary may
  get flagged by Defender / Symantec / etc. on the first few
  releases. Mitigations are documented in PLAN.md's risk register;
  the practical move is: submit to MS Defender via
  `submit.microsoft.com` after each release.

### Acceptance

- Signed `.msi` available on GitHub Releases.
- `signtool verify /pa /v boothrflow.msi` succeeds.
- Fresh-install on a clean Windows VM boots and dictates.

---

## Phase 5 — Onboarding wizard (2 days)

First-launch flow that walks the user through permissions, model
download, and hotkey config. Replaces the current "good luck, read
the README" experience.

### Deliverables

- **First-launch detection.** A `first_run` boolean in the
  settings store, flipped to `false` once the user finishes the
  wizard or hits "Skip."
- **Wizard window**, separate from the main Settings window so
  closing it doesn't dismiss progress. Five steps, each with a
  Continue button and a "Skip for now" link:
  1. **Welcome.** What boothrflow does, the privacy promise, the
     "100% local" callout.
  2. **Microphone permission.** Test mic capture in real time
     (live waveform), surface the System Settings deep link if
     it's blocked. Wait until working.
  3. **Accessibility + Input Monitoring permissions** (macOS) /
     just notes (Windows). Same UX: "Click here to grant," wait,
     re-probe.
  4. **STT model download.** Default Whisper tiny.en (~75MB). On
     a fast connection this finishes during the wizard. On slow,
     show progress + offer to skip-and-finish-in-background.
     Optional: Parakeet section with the trade-off explained
     ("higher quality, larger download, English-only").
  5. **Hotkey + LLM check.** Show the default hotkeys, let the
     user customize via the existing capture flow. Probe Ollama
     via the existing `llm_test_connection` command — if it
     returns a green light, they're done. If not, link to
     `pnpm ollama:pull` instructions.

  Final step closes the wizard, flips `first_run = false`, opens
  the main app with a "Try a dictation now" toast.

- **Wizard reachable from Settings → About → "Re-run onboarding"**
  for users who skipped.

### Risks

- **Accessibility permission grant requires app relaunch in dev
  mode.** With code signing landed (Phase 2), this is no longer
  true for production bundles, but the wizard text needs to be
  accurate per build mode. Detect via `cfg!(debug_assertions)`
  and show different copy.
- **Permission-grant detection on macOS is asynchronous and
  flaky.** TCC state is cached per-process; even after granting,
  our existing process may not see the change without a relaunch.
  Mitigation: after a permission grant, the wizard's "Verify"
  button calls `microphone_available` + the Screen Recording
  preflight in a loop with a 500ms cadence. If it doesn't flip
  within 5 seconds, surface "Try relaunching the app."

### Acceptance

- Fresh-install on a clean Mac → app launches → wizard appears →
  walking through all steps in order finishes with a working
  dictation. No CLI, no manual config.

---

## Phase 6 — Beta → Stable channel separation (0.5 day)

The plumbing for Phase 4 supports it; this phase actually uses it.

### Deliverables

- **Two release artifact streams:**
  - `v0.1.0-beta.N` tags → `latest-beta.json` manifest.
  - `v0.1.0` tags → `latest.json` manifest.
- **Promotion script** `scripts/promote-beta.sh`: takes a beta tag
  (`v0.1.0-beta.3`) and re-publishes it as the matching stable tag
  (`v0.1.0`) on the stable manifest. No re-build, no re-sign — same
  binaries, different manifest pointer.
- **`RELEASING.md` updated** with the cadence:
  - All non-trivial features ship as beta first.
  - Beta soaks for 3-7 days minimum (user-judgement on impact).
  - Promote to stable when:
    - No new bugs reported against the beta in 48h.
    - Eric has used the beta as his daily driver for at least 24h.

### Risks

- **Beta users get burned** by an update cadence that's too fast.
  3-7 day soak is the floor; set explicit "do not promote until X"
  blockers on each beta where applicable.

### Acceptance

- v0.1.0-beta.1 cut → soaks → promoted to v0.1.0 on the stable
  manifest. Stable users on auto-update get v0.1.0.

---

## Total estimate

| Phase                 | Days | Cumulative |
| --------------------- | ---- | ---------- |
| 1: Release infra      | 1-2  | 1-2        |
| 2: macOS signing      | 1    | 2-3        |
| 3: Auto-update        | 1    | 3-4        |
| 4: Windows signing    | 1-2  | 4-6        |
| 5: Onboarding wizard  | 2    | 6-8        |
| 6: Channel separation | 0.5  | 6.5-8.5    |

Realistic full-wave: **6-9 days of focused work** spread across
sessions. Each phase is independently shippable, so we can ship
phases 1-2 first and start running production builds while phases
3-6 land incrementally.

## Suggested ordering

The phase numbering above already reflects the dependency ordering —
release infra → macOS signing → macOS auto-update → Windows signing
→ onboarding → channels.

The early Wave 6 deliverable (phases 1+2+3) gives Eric a working
release loop on macOS: signed `.dmg`s on GitHub Releases that
auto-update him on his daily driver. **Auto-update is paired with
macOS signing because unsigned auto-update is broken UX** — every
update redownloads the bundle, Gatekeeper sees a "new app," user
re-does the Privacy & Security "Open Anyway" dance per release.
Sign first, auto-update second, and the loop is real.

After 1+2+3, Windows can lag by a release if Azure Trusted Signing
onboarding drags. Phase 5 (onboarding wizard) is FE-only and can
land in parallel with Phase 4. Phase 6 (channels) is last because
it depends on auto-update being solid.

---

## After Wave 6 — staging → production cadence

Wave 6 unlocks a real release loop. Every subsequent feature
follows this pattern:

1. **Land on a feature branch** (`feat/wave-N-name`), same as Wave 5.
2. **Local UAT** on the branch — checklist in `docs/uat/wave-N-checklist.md`.
3. **Cut a beta** when the UAT is green: `git tag v0.X.Y-beta.1`.
4. **Beta soak** (3-7 days) on Eric's daily driver. Beta users on
   the Beta update channel get it automatically.
5. **Promote to stable** via `scripts/promote-beta.sh`. Stable
   channel users get it on their next auto-update check.
6. **Hot-fix** path: if a bug ships to stable, cut a `v0.X.Y+1`
   stable directly (no beta) — auto-update pushes it within hours.

## Risks & mitigations summary

| Risk                                                   | Likelihood | Mitigation                                            |
| ------------------------------------------------------ | ---------- | ----------------------------------------------------- |
| Apple Developer Program membership not active          | low        | Confirm before starting Phase 2                       |
| Azure Trusted Signing onboarding delay                 | medium     | EV cert as fallback (Sectigo)                         |
| Defender false positives on rdev                       | medium     | Submit to MS Defender after each release              |
| GitHub Actions macOS-13 vs macOS-14 runner differences | low        | Pin runner version in workflow YAML                   |
| TCC state hangover from dev mode                       | high       | Test signed builds on a clean Mac VM                  |
| Universal-binary build issues with arch-gated features | medium     | Phase 1 acceptance includes a successful matrix build |
| Update signing key compromise                          | low        | Rotate yearly, store in GitHub Actions secret         |

## Open questions for Eric

- **Apple Developer Program** — already enrolled or need to start?
- **Azure account** — want to use one already in place at GreenPoint
  or set up a new dedicated one for boothrflow?
- **Domain for landing page** — `boothrflow.com` or similar? Not
  required for Wave 6 but tightly related (release-notes link target,
  ProductHunt-eventually anchor).
- **Beta channel users** — invite-only (handful of friends) or
  public-but-marked-beta? Affects how loud the "this is unstable"
  copy needs to be.
