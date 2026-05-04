# Wave 8 — Connectors, UI rebuild, privacy audit

**Goal:** the post-production-polish wave. By this point we have a
shipped, signed, auto-updating installer (Wave 7) running an engine
that benches well (Wave 6). This wave is where boothrflow becomes
_differentiated_ — not just a fast Wispr alternative, but the local-
first dictation tool that pushes its searchable corpus into the
rest of your knowledge stack and looks the part.

Three independent tracks; each is shippable on its own. Order
within the wave is decided by what's blocking other waves at the
time we start.

---

## Why this matters

After Wave 7, we have polish and trust. After Wave 8, we have
_reasons-to-switch_:

1. **Connectors** — the dictation history isn't a graveyard. A
   spoken thought lands in Obsidian as a note with frontmatter, gets
   pushed to Slack on voice command, embeds into the user's existing
   knowledge graph. The local-first FTS5 + nomic-embed-text infra we
   built in Wave 5 finally has consumers outside the app itself.
2. **Hyper-modern UI rebuild** — the current Settings panel and pill
   are functional but visually generic. A confident visual language,
   a Cmd-K command palette, and macOS-native vibrancy make boothrflow
   feel like a product, not a Tauri starter template.
3. **Privacy audit doc** — cheap to author, high trust signal. The
   exact "verify everything is local" prompt to feed an AI assistant,
   plus a checklist with file pointers + pass/fail. Reviewers and HN
   commenters notice this kind of thing.

---

## Phase 1 — Connectors (4–6 days)

The history store already has FTS5 + nomic-embed-text vectors per
dictation; currently the only consumer is in-app search. This phase
surfaces that data outward via a `Connector` trait.

### Deliverables

- **`Connector` trait** in `src-tauri/src/connectors/mod.rs`:
  ```rust
  trait Connector {
      fn id(&self) -> &str;          // "obsidian" | "slack" | "http-webhook"
      fn label(&self) -> &str;        // human-readable
      fn send(&self, payload: ConnectorPayload) -> Result<()>;
      fn config_schema(&self) -> Schema;  // for the Settings UI
  }
  ```
  Payload carries `{ raw, formatted, embedding, timestamp,
app_context, style }` so connectors can pick what they need.
- **Obsidian vault push** (highest user value):
  - User configures a vault directory in Settings → Connectors.
  - Each dictation lands as `YYYY-MM-DD-HHMM-<short-slug>.md` with
    YAML frontmatter: timestamp, source app, style, embedding (as a
    base64 vector), raw + formatted in fenced code blocks.
  - The auto-format `Assertive` style (Wave 6) is the natural feeder
    here — bullet lists / paragraph breaks / code fences land cleanly
    in Obsidian's Live Preview.
- **Custom HTTP webhook**: POST `{raw, formatted, embedding,
timestamp, app_context, style}` to a user-configured URL. Headers
  configurable; body shape fixed. Catch-all for anyone wiring
  boothrflow into a personal automation pipeline (n8n, Zapier
  webhooks, custom Lambda, etc.).
- **Slack incoming webhook**: per-channel webhook URL; on success,
  paste a Slack-formatted message into the configured channel.
  Markdown-light (Slack's mrkdwn flavor); no API token required.
- **Voice-triggered routing**: the cleanup pass detects routing
  instructions inline ("push this to Slack," "send to email," "drop
  into the ops channel") and treats them as a `Connector::SendTo`
  instead of a paste. The instruction itself is stripped from the
  body. Off by default; opt-in toggle in Settings → Connectors.
  Examples:
  - "Push this to Slack: meeting recap is..." → strips "Push this to
    Slack:", body sends to Slack.
  - "And drop a copy in Obsidian" at the end → also fans out to
    Obsidian.
- **History row "Push to..." dropdown**: each row in the History
  panel grows a dropdown listing configured connectors. Lets users
  retroactively push old dictations.
- **Settings → Connectors tab**: add/remove/configure connectors,
  per-connector enabled toggle, test-send button.

### Open questions

- **Auth complexity**: Slack incoming webhooks are simple (just a
  URL). Linear / Notion / Gmail need OAuth. Punt OAuth-required
  connectors to a follow-up wave; v1 = file-based (Obsidian) +
  webhook-based (HTTP, Slack incoming).
- **Connector failures**: should a Slack 500 fail the whole paste,
  or just log and move on? Default to "log and move on" — connectors
  are an _also_, not the primary destination.

### Acceptance

- Dictate → say "push to Obsidian" → see a markdown file land in the
  configured vault with frontmatter populated.
- Dictate → say "push to Slack" → see the message appear in the
  configured channel.
- Dictate normally → paste lands in the focused app + appears in
  History → click "Push to Obsidian" on the History row → file
  lands.

---

## Phase 2 — Hyper-modern UI rebuild (5–8 days)

The current Settings panel is functional but visually generic. The
pill is a rectangle. This phase invests in the most-visible surfaces.

### Deliverables

- **Visual language refresh**:
  - Pick a design system. Options: shadcn-svelte (community port),
    Park UI (CSS variables-based), or a hand-rolled small system.
    Decision factor: maintenance cost vs the variety of components
    we need (form fields, modals, segmented controls, command
    palette, leaderboard tables). Lean shadcn-svelte unless it adds
    more bundle size than we're willing to spend.
  - Confident typography (Inter or similar variable font, ~3 weight
    tiers).
  - Low-chrome cards; generous whitespace; motion on hover and
    transitions.
- **Pill redesign**:
  - During listening: a single pulsing dot (audio-reactive scale).
    Currently a rectangle with text inside.
  - During cleanup: typewriter trail of the cleaned text.
  - On paste: brief fade-out.
  - Non-blocking: still draggable, still small, still always-on-top.
  - Variant for `Assertive` style: shows a "structuring..." indicator
    during the longer LLM pass.
- **Liquid Glass / Vibrancy on macOS**:
  - Apply `NSVisualEffectView` blur + vibrancy to the Settings
    window's chrome and the pill background.
  - Tauri 2 + WebKit needs `objc2-app-kit` calls into the WKWebView's
    `contentView` to attach the visual effect view at the AppKit
    layer. ~50 lines of FFI.
- **Cmd-K command palette**:
  - Same window pattern as the quickpaste palette (already exists).
  - Fuzzy search over: settings actions ("set default style to
    moderate"), history entries, connectors ("push last dictation to
    Slack"), recent commands.
  - Activated via `Cmd+K` globally when the Settings window is open;
    `Cmd+Shift+K` from anywhere when the app is running.
- **Keyboard shortcuts everywhere**:
  - `Cmd+,` opens Settings (macOS convention).
  - `Cmd+/` opens the history search field.
  - `Esc` closes any modal.
  - `?` shows a keyboard-shortcut cheatsheet overlay.
- **Onboarding flow polish**: Wave 7 ships a permissions wizard;
  this phase makes it look like a continuation of the new visual
  language rather than a separate aesthetic.

### Open questions

- **Bundle size**: shadcn-svelte ports vary in size. Measure before
  committing. If > 200KB additional, hand-roll instead.
- **Liquid Glass on Linux/Windows**: macOS-only. Linux + Windows
  fall back to the existing solid-background variant. Document.
- **Tauri vs Tauri 2**: we're on Tauri 2 already; Liquid Glass FFI
  may be cleaner there (or may not). Verify before scope-locking.

### Acceptance

- Settings panel visually distinct from a Tauri starter template.
- Pill no longer rectangular by default.
- `Cmd+K` opens the palette and "set default style to assertive"
  works.
- macOS users see vibrancy behind the Settings window chrome.

---

## Phase 3 — Privacy audit doc (1 day)

Cheap to author, high trust signal. The exact "is this thing
actually local?" verification, in a form a skeptical reviewer can
work through in 15 minutes.

### Deliverables

- **`PRIVACY_AUDIT.md`** at repo root:
  - **Section 1 — The "verify everything is local" AI prompt**: a
    pre-written prompt the user can feed to an AI assistant
    (Claude, ChatGPT, etc.) that asks the assistant to scan the
    codebase for any network calls and verify they're all
    user-configured BYOK endpoints. Includes the exact files to look
    at and what "non-local" looks like.
  - **Section 2 — Default-features checklist**: every default
    feature (transcription, cleanup, embeddings, paste, history,
    OCR, learning) → file pointer → "where does the data go?"
    answer.
  - **Section 3 — Opt-in BYOK**: lists every endpoint config that
    _can_ be pointed at a cloud service (LLM endpoint, embeddings
    endpoint, future translation), what's transmitted when it is,
    and how to verify by sniffing local traffic
    (`mitmproxy` / `Charles` / `Little Snitch`).
  - **Section 4 — Telemetry**: confirms there is none. Any future
    crash-reporting / usage-metrics opt-in goes here too.
  - **Section 5 — Pass/fail**: a table of checks the reader can run,
    with expected outputs.
- **Settings → Privacy → "Run privacy audit"** button: opens the
  `PRIVACY_AUDIT.md` in the user's default browser (or in-app).
- **README.md badge**: "Privacy audit: 2026-XX-XX" with a link to
  the latest committed version. Visible commitment to keeping it
  current.

### Open questions

- **Audit cadence**: re-run by hand each release? Automate via a
  GitHub Action that diffs `PRIVACY_AUDIT.md` against the codebase
  on every PR and warns if a new network call appears? Probably
  start manual, automate once we've shipped a few releases.

### Acceptance

- A reviewer can clone the repo, open `PRIVACY_AUDIT.md`, run the
  Section 5 checks, and end up with all-green.
- Skeptical HN commenter who says "but how do I know?" gets pointed
  at this doc and is satisfied (or finds a real bug, which is also
  great).

---

## Out of scope (for this wave)

- **Meeting transcription mode** — separate product surface.
  Multitalker Parakeet (queued in Future Ideas) is a prerequisite.
- **Plugin API** — depends on the connector trait being battle-
  tested first. v0 connectors (Phase 1) are first-class; plugin API
  is a follow-up.
- **Insights dashboard** — straightforward to build but waits for
  enough usage data to be interesting.
- **Voice commands** ("press enter," "delete that") — adjacent to
  voice-triggered routing in Phase 1 but a different parser; punt
  to a follow-up wave.
- **Snippets / voice-activated text expanders** — same.
- **Linux port** — gated on rdev's Wayland coverage maturing.
- **Mobile companion (iOS, Path B)** — separate effort, separate
  repo possibly.

---

## Risks + mitigations

| Risk                                                                          | Impact | Mitigation                                                                                                   |
| ----------------------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------ |
| Connector trait design rots when adding OAuth-needing services in a follow-up | Med    | v1 covers file-based (Obsidian) + URL-only webhook (HTTP, Slack); review trait shape before opening to OAuth |
| UI rebuild creeps into a 3-week scope                                         | High   | Phase 2 has a hard 8-day cap; if we hit it, ship what's done and queue the rest                              |
| Liquid Glass FFI breaks on macOS minor-version updates                        | Low    | Pin a specific objc2-app-kit version; cover with a feature flag so it can be turned off if it explodes       |
| Privacy audit reveals a bug                                                   | Low    | If it does, fix the bug — that's the _point_ of the doc                                                      |
