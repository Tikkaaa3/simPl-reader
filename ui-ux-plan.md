# Reader UI and UX plan

This plan makes simPl a quieter Windows reader. The reading screen loses its permanent chrome. A floating top bar hides itself, and full-height strips at both edges turn pages on hover and click. A thin progress line keeps your place, and the scrollbar hides while you read. The macOS window-button mode is deleted because simPl ships only for Windows.
The rule for every PR is that the page never moves when chrome appears or disappears. Chrome floats over the page and never takes layout space from it.
PRs land in this order. UX-0 driver and probes, UX-1 Windows-only chrome, UX-2 floating bar, UX-3 page-turn strips, UX-4 progress line and idle scrollbar, UX-5 icon bar and **Aa** panel, UX-6 floating panels and contents drawer, UX-7 wheel to next page, UX-8 page surface and state styling, UX-9 library header, UX-10 settings sections.

## How to read this

One box is one unit of work. Every box names the evidence that checks it. A nested box is a sub-step of the box above it. Check a box only when its evidence exists, a file, a log line, a screenshot, a test run, or a SHA. The body is a how-to. The appendices explain and record.

The program runs `~/.claude/skills/poteto-mode/playbooks/autopilot-stack.md`. The work is sequenced and coupled in one reader view, and the operator reviews before landing. The root appends each verified PR to one linear stack on `main`. The operator lands the stack bottom-up. Every PR from UX-1 to UX-10 changes an interaction and stops at merge-ready for the operator. UX-0 changes no interaction.

Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

## Program checklist

### Arm the program

- [ ] State the protocol and this plan to the operator, then stop. Start execution only on the operator's explicit go.
- [ ] Read these at program start. Re-read them at every tick.
  - [ ] `git show origin/main:ui-ux-plan.md`
  - [ ] `git show origin/main:design/DESIGN.md`
  - [ ] `~/.claude/skills/poteto-mode/playbooks/autopilot-stack.md`
  - [ ] `~/.claude/skills/swarm/SKILL.md`
  - [ ] `~/.claude/skills/poteto-mode/playbooks/opening-a-pr.md`
  - [ ] `~/.claude/skills/principle-model-the-domain/SKILL.md`, `~/.claude/skills/principle-experience-first/SKILL.md`, and `~/.claude/skills/principle-prove-it-works/SKILL.md`
- [ ] On the operator's go, arm the audit tick as `/loop 1h` with the tick prompt below. Never leave the cadence to memory.
- [ ] Use this tick prompt, verbatim. "Re-read the execution playbook from trunk. Audit the operation against it and fix drift in this tick. Probe every active lane and judge progress by side effects only. Stand down a stuck lane and dispatch its replacement now. Then post a short status message to the operator in chat only when the audit found a tracked change that no earlier status message reported, such as a PR opened, a code-ready head, a round launched or closed, a verdict, a merge, a stuck agent and the action taken, a blocker added or cleared, or a decision only the operator can make. Name every such change and nothing else. Do not repeat a table, the merged list, or an unchanged blocker. If the audit found none, end the turn with no reply text. Either way, log this tick's row in your decision trail. The row names the items reported, or none."
- [ ] On the operator's hold or stand-down, send every owner a zero-writes order at once.

### Spawn owners

- [ ] Spawn one owner per PR with the full lifecycle the execution playbook names.
- [ ] Follow this dependency graph. Base each branch on its parent branch, because the program stacks.
  - [ ] UX-0 is first. It branches from `main`.
  - [ ] UX-1 after UX-0.
  - [ ] UX-2 after UX-1.
  - [ ] UX-3 after UX-2.
  - [ ] UX-4 after UX-3.
  - [ ] UX-5 after UX-4.
  - [ ] UX-6 after UX-5.
  - [ ] UX-7 after UX-6.
  - [ ] UX-8 after UX-7.
  - [ ] UX-9 after UX-8 in the stack. Its owner may build in parallel from UX-1, because it touches only the library.
  - [ ] UX-10 after UX-9.
- [ ] Hold the file boundaries.
  - [ ] UX-0 touches only `scripts/ui-drive.ps1`, `scripts/subset-icons.py`, `crates/iced-shell/src/book_preview.rs`, `crates/iced-shell/src/reader_profile.rs`, and the `#[cfg(test)]` module list in `crates/iced-shell/src/app.rs`.
  - [ ] UX-9 touches only `crates/iced-shell/src/shelf.rs` and `README.md`.
  - [ ] Every other PR keeps to the paths in its **Files.** block.
- [ ] Hold the review gate. UX-1 to UX-10 change an interaction. They wait for the operator's review in chat with screenshots and a video before merge.

### PR mechanics, for every PR

- [ ] Resolve the forge once. Default to `gh`; if `command -v origin` succeeds and Origin can resolve the repository, use `origin pr` for every PR operation. Record any fallback to `gh`. Never require `gt`.
- [ ] Open the PR ready, never draft, per **Opening a PR**. Use the run's built-in PR tool when it has one, else `origin pr create --status open --base <base-branch>` or `gh pr create --base <base-branch>` according to the resolved forge. A stack child targets its parent branch.
- [ ] Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 check` once before the PR-facing push. It runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `cargo test --workspace --all-targets --locked`. Paste its last lines in the PR body, because `.github/workflows/release.yml` runs only on tags and never on PRs. Push with hooks on.
- [ ] Run `/deslop` before each commit and `/no-comments` before review. Without `deslop`, reread the diff and cut slop by hand.
- [ ] Triage every Bugbot and security-reviewer comment per `~/.claude/skills/poteto-mode/references/bugbot-triage.md`. If the repo has no Bugbot, record that once.
- [ ] Rebase onto current trunk before the code-ready report and babysit. Keep that merge base in fix rounds. Rebase again only at merge prep, on a `git merge-tree` conflict with trunk, or on a CI failure that comes from a change on trunk.
- [ ] Update `README.md` in the same PR whenever the PR changes a shortcut, a control, or a sentence the README states.

### Verdict and merge, for every PR

- [ ] At the code-ready head SHA and at each later push that changes the patch, run the swarm per `~/.claude/skills/swarm/SKILL.md`. One gates lane runs `scripts\dev.ps1 check`. The ten live lanes come from the PR's **Verify, live** block. The perf lane comes from its **Verify, perf** block. Two or more audit lanes, each with its own focus, read the diff and the receipts and distrust the PR body. The root audits the receipts in the merge-ready report before the verdict.
- [ ] Clean only when every lane is `PASS`. Findings go back to the owner, including a defect that a lane filed as a note. A new head gets a fresh swarm and a fresh verdict, except for results that stay valid under the patch-id rule in `~/.claude/skills/poteto-mode/playbooks/shipping.md`.
- [ ] On a clean verdict, the root appends the PR to the linear stack on `main` in the order above. No owner merges. The operator lands the stack bottom-up. A rebase that keeps the patch-id keeps the verdict. A rebase that changes the patch-id sends the PR back through a fresh swarm.

### Boot recipe, for every live lane

simPl is a native Windows app, so every lane runs on a Windows desktop at the PR head, not on a cloud VM. No control skill drives native Windows windows. Lanes drive the app through `scripts/ui-drive.ps1`, which UX-0 adds. That script moves the global cursor, so lanes on one desktop run one at a time.

- [ ] `git fetch origin <head-branch> && git worktree add ..\simPl-<pr-id> <head SHA>`.
- [ ] In the worktree, run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 build`. Then run `python scripts/promotion-fixtures.py target\lane-fixtures` to create the demo books.
- [ ] Launch with `scripts\ui-drive.ps1 -Launch "target\lane-fixtures\The Quiet Hours.epub" -Profile target\lane-profile-<n> -Size 1400x900`. The script sets `LOCALAPPDATA` to the lane profile, so the real library stays untouched.
- [ ] Deliver input only through `ui-drive.ps1` (`-Move`, `-Click`, `-Wheel`, `-Key`, `-Resize`, `-Hold`). Read-only diagnostics are `-Shot`, `-IdleCpu`, the window title, and the files under the lane profile's `simPl` folder.
- [ ] Save every screenshot to `target\swarm-<pr-id>\worker-<n>\<slug>.png` and return the paths with the report.

## Add the live-lane driver and the frame probe (UX-0)

**Depends on.** None.

**Files.**

- [ ] Create `scripts/ui-drive.ps1`.
- [ ] Create `scripts/subset-icons.py`.
- [ ] Create `crates/iced-shell/src/reader_profile.rs`.
- [ ] Edit `crates/iced-shell/src/book_preview.rs`.
- [ ] Edit `crates/iced-shell/src/app.rs`, only the `#[cfg(test)]` module list near `release_audit` at line 40.

**Build.**

- [ ] Write `scripts/ui-drive.ps1` from the session driver in Appendix A. It takes `-Launch <path>`, `-Profile <dir>`, `-Size WxH`, `-Move x,y`, `-Click`, `-Wheel <delta>`, `-Key <SendKeys string>`, `-Hold <ms>`, `-Shot <png>`, `-IdleCpu <seconds>`, and `-Stop`. Coordinates are client-relative, because the window has no OS decorations (`decorations: false`, app.rs:7035).
- [ ] Add `render_with_cursor(reader, path, point)` beside `render` in `book_preview.rs:370`. It passes `mouse::Cursor::Available(point)` where `render_at_scale` passes `mouse::Cursor::Unavailable` today (book_preview.rs:503 and 518), so headless renders can show hover states.
- [ ] Add the ignored test `profile_reader_frames` in `reader_profile.rs`, modeled on `profile_settings_scroll` (settings_profile.rs:98). It renders 120 frames per state at 1280x800 and prints one line per state, `reader_frame_ms state=<name> median=<ms> p95=<ms>`. The states are `library`, `reading-idle`, `reading-page-turn`, and `settings`. Later PRs add their own states.
- [ ] Write `scripts/subset-icons.py`. It downloads the Material Symbols source font at the blob SHA pinned in `assets/licenses/Material-Symbols-CHANGES.txt`, subsets it to a codepoint list kept in the script, writes `assets/fonts/MaterialSymbolsOutlined-Subset.ttf`, and rewrites the codepoint line of the change notice. The list starts as the ten codepoints shipped today.

**You see.**

- [ ] `scripts\ui-drive.ps1 -Launch ... -Shot target\ux0\reading.png` writes a 1400x900 PNG of the reading screen.
- [ ] `cargo test -p iced-shell --release profile_reader_frames -- --ignored --nocapture --test-threads=1` prints four `reader_frame_ms` lines.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `book_preview.rs` gains `render_with_cursor_shows_card_hover`. It renders the library with the cursor over a card and asserts that the favourite button's pixels differ from a render without a cursor. Run `cargo test -p iced-shell --locked render_with_cursor`.
- [ ] Run `scripts\dev.ps1 check`. It passes with no new warnings.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no driver, so record that fact. Launch trunk by hand and head through the driver with the same fixture, then capture the reading screen at 1400x900. Save `ux0-regression.png`. Pass when both captures show the same layout.
- [ ] Lane 2. Launch with `-Profile target\lane-profile-2`. Save `ux0-isolated.png`. Pass when `target\lane-profile-2\simPl\library.json` exists and the modified time of `%LOCALAPPDATA%\simPl` in the real profile is unchanged.
- [ ] Lane 3. Run `-Resize 860x700`, then `-Shot`. Save `ux0-resize.png`. Pass when the PNG is exactly 860x700.
- [ ] Lane 4. Press Ctrl+W, then move the cursor over a library card. Save `ux0-hover.png`. Pass when the card shows its favourite and remove buttons.
- [ ] Lane 5. Click the Continue Reading card. Save `ux0-click.png`. Pass when the book opens at its saved page.
- [ ] Lane 6. Send `-Key '{F8}'`. Save `ux0-key.png`. Pass when the reading toolbar collapses.
- [ ] Lane 7. Send `-Wheel -360` over the page. Save `ux0-wheel.png`. Pass when the page text sits higher than in the capture before the wheel.
- [ ] Lane 8. Run `-IdleCpu 10` with the book open and the mouse still. Save `ux0-idle.png`. Pass when the script prints `idle_cpu_ms_per_10s=<n>` with a number.
- [ ] Lane 9. Run the `profile_reader_frames` command from **You see.** Save `ux0-profile.png` as a capture of the console. Pass when four state lines print with numeric median and p95.
- [ ] Lane 10. Run `python scripts/subset-icons.py` with the shipped codepoint list. Save `ux0-icons.png` as a capture of the console. Pass when the SHA-256 of the regenerated font equals the committed file, or the lane records the `fontTools` version that changed the bytes.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. The size in bytes of `target\release\iced-shell.exe`. Trunk lacks the frame probe, so the diff-added work gets an absolute budget, the wall time of `profile_reader_frames`.
- [ ] Probe. Build release at trunk, then at head, then at trunk again, and record the exe size each time. Time `profile_reader_frames` at head three times.
- [ ] Baseline. Record the trunk exe size first.
- [ ] Rule. Fail when the head exe size differs from trunk by more than 0 bytes, because all new code is test-only or script. Fail when the median `profile_reader_frames` wall time exceeds 90 seconds.

**Review gate.** None. UX-0 is not review-gated.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-0 to the stack. The operator lands it bottom-up.

## Delete the macOS window-button mode (UX-1)

**Depends on.** UX-0.

**Files.**

- [ ] Edit `crates/reader-document/src/preferences.rs`.
- [ ] Edit `crates/iced-shell/src/chrome.rs`.
- [ ] Edit `crates/iced-shell/src/app.rs`.
- [ ] Edit `crates/iced-shell/src/book_preview.rs`.
- [ ] Edit `crates/iced-shell/src/promotion_preview.rs`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Delete `enum WindowControls` (preferences.rs:36-45), the `window_controls` field of `Preferences` (:50), and the field of `Record` (:67) with its load and save mappings (:125, :142). Delete the field, not only the `Mac` variant. `Record` has no `deny_unknown_fields`, so serde then ignores the `"window_controls":"mac"` key that most saved files contain. Removing only the variant makes `load_from` (:100) reject those files, and app.rs:3807 then turns off saving for the session.
- [ ] In `chrome.rs`, delete `dot` (:124-147), `blend` (:51), the `dots` row (:233-256), the Mac arm of the tool order (:292-366), and the Mac side widths (:397-418). Delete `Controls.style` (:217). The Windows layout becomes the only layout.
- [ ] In `app.rs`, delete `Control::SettingsWindowControls` (:1189), `Message::SetWindowControls` (:1327), the handler (:3775-3790), the settings row (:5938-5945), `window_controls_button` (:6168-6181), the focus entries (:2277-2280), and the field (:1032, :1132, :2108). Collapse the per-style title-bar focus order (:2230-2262) to the Windows order.
- [ ] Delete `SIMPL_PREVIEW_WINDOWS` handling (book_preview.rs:1368-1372), the `windows-controls-*` captures (book_preview.rs:1064-1069), and the `SetWindowControls` call (promotion_preview.rs:47).
- [ ] Rewrite README.md:34 so it no longer offers a choice of window buttons.

**You see.**

- [ ] A fresh profile opens with minimize, maximize, and close at the right of the title bar, and Settings is the outermost icon on the left.
- [ ] Settings has no **Window controls** row.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `preferences.rs` gains `legacy_mac_window_controls_key_still_loads`. It writes a version 1 record with `"window_controls":"mac"` and `"appearance":"dark"`, calls `load_from`, and asserts `Ok` with `Appearance::Dark`. Run `cargo test -p reader-document --locked legacy_mac`.
- [ ] Replace the round-trip loop over both variants (preferences.rs:176) and the Mac default assertion (:212) with a round-trip that asserts the saved JSON contains no `window_controls` key. Run `cargo test -p reader-document --locked`.
- [ ] Update `saved_theme_reaches_a_book_that_is_already_open` (app.rs:7996) to drop the field. Run `cargo test -p iced-shell --locked saved_theme`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Open a fresh profile at trunk and at head. Save `ux1-regression.png`. Pass when trunk shows round dots at the left and head shows Windows caption buttons at the right.
- [ ] Lane 2. Copy a trunk `preferences.json` with `"window_controls":"mac"`, the Soft theme, and dark appearance into the lane profile, then launch head. Save `ux1-legacy.png`. Pass when the app shows Soft dark with no "Invalid preferences" notice.
- [ ] Lane 3. In the lane 2 profile, change the text size in Settings, then quit. Save `ux1-resave.png`. Pass when `preferences.json` is rewritten with the new size and without a `window_controls` key.
- [ ] Lane 4. Open Settings. Save `ux1-settings.png`. Pass when no **Window controls** row or macOS option appears.
- [ ] Lane 5. Press Tab through the title bar. Save `ux1-focus.png` with focus on Close. Pass when focus visits Settings, the other tools, Minimize, Maximize, and Close in that order.
- [ ] Lane 6. Click Maximize, then click it again. Save `ux1-maximize.png`. Pass when the window maximizes, the glyph changes to restore, and the second click restores it.
- [ ] Lane 7. With a book on page 4, click Close, then relaunch. Save `ux1-close.png`. Pass when the book reopens on page 4.
- [ ] Lane 8. Double-click the empty title bar, then drag it. Save `ux1-drag.png`. Pass when double-click maximizes and the drag moves the restored window.
- [ ] Lane 9. Restore a backup ZIP made at trunk that contains `"window_controls":"mac"`. Save `ux1-restore.png`. Pass when the restore review appears and **Restore and replace** finishes without an error.
- [ ] Lane 10. Resize to the minimum 540x360 in dark appearance. Save `ux1-narrow.png`. Pass when the title, tools, and caption buttons do not overlap.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `reader_frame_ms` p95 for `library` and `reading-idle`.
- [ ] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [ ] Baseline. Record the trunk p95 for both states first.
- [ ] Rule. Fail when the head median of the three p95 values exceeds trunk by more than 10 percent in either state.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, and 5 screenshots into `target\ui-ux-review\UX-1-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the change on the lane desktop with `ffmpeg -f gdigrab -framerate 30 -i title=simPl -t 45`, or with Xbox Game Bar. Save it as `target\ui-ux-review\UX-1-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-1 to the stack. The operator lands it bottom-up.

## Float the reading bar and hide it when idle (UX-2)

**Depends on.** UX-1.

**Files.**

- [ ] Create `crates/iced-shell/src/reading_chrome.rs`.
- [ ] Edit `crates/iced-shell/src/app.rs`.
- [ ] Edit `crates/iced-shell/src/chrome.rs`.
- [ ] Edit `crates/iced-shell/src/reader_profile.rs`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Model the bar's visibility as one state machine in `reading_chrome.rs`, replacing `toolbar_expanded` (app.rs:1020). Use `enum BarState { Pinned, Shown { hide_at: Option<Instant> }, Hidden }` plus an `iced::Animation<bool>` for the fade. `toolbar_expanded` is session-only today, so no saved data migrates.
- [ ] In a book, merge the title bar and `document_toolbar` (app.rs:6252) into one 44px bar. Render it as the top layer of a `stack!` over the reading area, not as a row of the page column (app.rs:6827). The page keeps its current position whether the bar shows or not.
- [ ] Give the bar an opaque `lowest` background and a 1px `border` hairline at its bottom edge. Do not use a drop shadow or translucency. Appendix A shows why.
- [ ] Show the bar for 2 seconds after a book opens. Show it while the pointer is in the top 48px or over the bar. Hide it 1.5 seconds after the pointer leaves. Fade in and out over 150ms.
- [ ] Keep the bar shown while a bar control has keyboard focus or while a panel the bar opened is open. Keep every bar control in `Reader::controls()` (app.rs:2161) at all times, and reveal the bar when focus lands on one. Escape hides the bar when nothing else is open.
- [ ] While hidden, render only a transparent 48px `mouse_area` with `on_enter` and no `interaction`. Its pointer state stays `None`, so the page under it keeps hover and clicks. The bar's buttons are not in the tree while hidden.
- [ ] Subscribe to `window::frames()` only while `hide_at` is pending or the fade runs. That follows the rule at app.rs:6914, no idle redraw loop.
- [ ] Change F8 from hide or show to pin or unpin. Delete the `▤` title-bar icon (chrome.rs:325-337) and `Control::ToggleToolbar`.
- [ ] Put the light and dark toggle at the same position in the bar on the Library and the reading screens.
- [ ] Keep the static, always-visible title bar on the Library screen.
- [ ] In fullscreen, use the same bar without caption buttons.
- [ ] Add the states `reading-bar-reveal` and `reading-bar-hidden` to `profile_reader_frames`.

**You see.**

- [ ] On opening a book, the bar shows and then fades out after 2 seconds. The first line of text stays at the same y position before and after.
- [ ] Moving the pointer to the top edge brings the bar back within 150ms.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `reading_chrome.rs` gains transition tests that drive `BarState` with fixed `Instant`s. `opened_then_idle_hides_after_two_seconds` asserts `Hidden` at open plus 2.1 seconds. `focus_holds_the_bar` asserts `Shown { hide_at: None }` while a bar control is focused. `pin_survives_idle` asserts `Pinned` after 60 seconds. Run `cargo test -p iced-shell --locked reading_chrome`.
- [ ] `app.rs` gains `bar_overlay_does_not_move_the_page`. It renders a book with the bar shown and hidden through `book_preview::render` and asserts that the first text row has the same y in both. Run `cargo test -p iced-shell --locked bar_overlay`.
- [ ] Replace the `ToggleToolbar` tests (app.rs:7600, 8453-8471) with tests of the pin behavior.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Open the book at trunk and at head, and capture at 0.5 seconds and at 3 seconds after opening. Save `ux2-regression.png`. Pass when trunk shows both bars in both captures, head shows no bar at 3 seconds, and the head's first text line has the same y in both captures.
- [ ] Lane 2. Move the pointer to y=20, then to the page centre, and capture 1.0 and 2.0 seconds later. Save `ux2-reveal.png`. Pass when the bar shows at y=20, is still shown at 1.0 seconds, and is gone at 2.0 seconds.
- [ ] Lane 3. Rest the pointer on the bar for 10 seconds. Save `ux2-hold.png`. Pass when the bar is still shown at 10 seconds.
- [ ] Lane 4. Press F8, wait 5 seconds, press F8, then wait 3 seconds. Save `ux2-pin.png`. Pass when the bar stays at 5 seconds and is gone 3 seconds after unpinning.
- [ ] Lane 5. With the bar hidden, press Tab, wait 5 seconds, then press Escape. Save `ux2-keyboard.png`. Pass when Tab reveals the bar with a focus ring on its first control, the bar is still shown at 5 seconds, and Escape hides it.
- [ ] Lane 6. Reveal the bar, drag it, then double-click it. Save `ux2-drag.png`. Pass when the drag moves the window and the double-click maximizes it.
- [ ] Lane 7. Press F11, wait 3 seconds, move to the top edge, then press Escape. Save `ux2-fullscreen.png`. Pass when the bar hides in fullscreen, reveals without caption buttons, and Escape leaves fullscreen.
- [ ] Lane 8. Press Ctrl+W and wait 10 seconds on the Library. Save `ux2-library.png`. Pass when the Library title bar is still shown.
- [ ] Lane 9. Resize to 860x700 in dark appearance and reveal the bar. Save `ux2-narrow.png`. Pass when no bar controls overlap and the page does not move.
- [ ] Lane 10. Capture the bar on the Library and on the reading screen. Save `ux2-toggle-position.png`. Pass when the light and dark toggle has the same x position on both screens, within 2px.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `idle_cpu_ms_per_10s` with a book open and the pointer still, and `reader_frame_ms` p95. Trunk has no fade, so the fade gets an absolute budget, the p95 of `reading-bar-reveal`.
- [ ] Probe. Run `ui-drive.ps1 -IdleCpu 10` at trunk, head, trunk, head, trunk, head. Run `profile_reader_frames` the same way.
- [ ] Baseline. Record the trunk idle CPU and the trunk `reading-idle` p95 first.
- [ ] Rule. Fail when the head median idle CPU exceeds trunk by more than 30ms per 10 seconds. Fail when the head `reading-idle` p95 exceeds trunk by more than 10 percent. Fail when the `reading-bar-reveal` p95 exceeds 33ms.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, 5, and 7 screenshots into `target\ui-ux-review\UX-2-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the reveal, the hide, the pin, and fullscreen on the lane desktop. Save it as `target\ui-ux-review\UX-2-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-2 to the stack. The operator lands it bottom-up.

## Turn pages from full-height edge strips (UX-3)

**Depends on.** UX-2.

**Files.**

- [ ] Create `crates/iced-shell/src/page_strips.rs`.
- [ ] Edit `crates/iced-shell/src/app.rs`.
- [ ] Edit `crates/iced-shell/src/notes_ui.rs`.
- [ ] Edit `crates/iced-shell/src/pdf_reader.rs`.
- [ ] Edit `crates/iced-shell/src/reader_profile.rs`.
- [ ] Edit `scripts/subset-icons.py` and `assets/fonts/MaterialSymbolsOutlined-Subset.ttf`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Add a left and a right strip as a `stack!` layer above the reading area and below the bar. Each strip is the full height of the reading area. Its width is `(window_width * 0.07).clamp(48.0, 128.0)`, which gives 59px at 844px, 97px at 1384px, and 128px from about 1830px. The right strip ends at the left edge of the notes drawer when the drawer is open.
- [ ] Turn the page on `on_press` inside the strip. Send `Message::BookPage(bool)` in Book view, through `adjacent_book_page` (app.rs:1682). Send the PDF previous or next message in PDF Document view. A drag that starts on the page and ends in a strip never turns the page, because the press happened outside the strip.
- [ ] Forward the wheel. A strip with a pointer cursor makes `stack!` hide the cursor from the page under it (`iced_widget-0.14.2/src/stack.rs:266-272`), so the page stops scrolling there. Give each strip `on_scroll` that sends `operation::scroll_by` to the reading scrollable.
- [ ] Draw nothing until hover. On hover, fade in a gradient from about 7 percent black at the window edge to clear, plus a chevron icon of 28 to 32px in a faint 48px circle, centred vertically in the strip. Use a hand cursor. Pick the final look from the Appendix A variants at the review gate.
- [ ] Disable a strip when `can_turn` (app.rs:1652) is false. A disabled strip shows no shade, no chevron, no hand cursor, and ignores presses.
- [ ] Add `chevron_left` and `chevron_right` to the codepoint list in `subset-icons.py` and regenerate the font.
- [ ] Delete the `‹` edge toggle from `notes_ui::sidebar` (notes_ui.rs:1471-1520) and its 26px edge width in `sidebar_width` (notes_ui.rs:25). Add a **Notes** button to the bar that sends `Action::ToggleList`. Ctrl+B is unchanged.
- [ ] Delete the toolbar's previous and next page buttons, `Control::PreviousBookPage`, `Control::NextBookPage`, and the PDF `FocusControl::Previous` and `FocusControl::Next` buttons. Keep the page field and the arrow keys.
- [ ] Keep the 14px document scrollbar rail (`document_scroll.rs:11`) at the far right edge. The right strip sits just inside it, so the rail stays draggable.
- [ ] Add the state `reading-strip-hover` to `profile_reader_frames`, rendered through `render_with_cursor`.
- [ ] Rewrite the README sentence about the arrow at the right edge.

**You see.**

- [ ] Hovering anywhere along the right edge shows a soft shade and a chevron. A click at the top, middle, or bottom of the edge turns to the next page.
- [ ] No arrow tab sits on the right edge. The bar has a **Notes** button.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `page_strips.rs` gains `strip_width_follows_the_window`. It asserts the literal widths 48 at 600, 59 at 844, 97 at 1384, and 128 at 2560. Run `cargo test -p iced-shell --locked strip_width`.
- [ ] `app.rs` gains `strip_press_turns_the_page` and `strip_press_on_first_page_does_nothing`. They send the strip messages through `update` and assert the page number. Run `cargo test -p iced-shell --locked strip_press`.
- [ ] `app.rs` gains `strip_wheel_scrolls_the_page`. It sends the forwarded wheel message and asserts that the viewport offset grows. Run `cargo test -p iced-shell --locked strip_wheel`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Turn from page 1 to page 4 at trunk with the toolbar arrow. At head, click the right strip at y=10 percent, 50 percent, and 90 percent of the window height. Save `ux3-regression.png`. Pass when head reaches page 4 and the page field reads 4.
- [ ] Lane 2. Hover the right strip, then move to the page centre. Save `ux3-hover.png`. Pass when the shade and chevron show on hover and are gone after leaving.
- [ ] Lane 3. On page 1, hover and click the left strip. Save `ux3-first-page.png`. Pass when the left strip shows nothing and the page stays 1.
- [ ] Lane 4. Go to the last page of chapter 1 and click the right strip. Save `ux3-chapter.png`. Pass when chapter 2 opens on its first page.
- [ ] Lane 5. Hover the right strip and send `-Wheel -360`. Save `ux3-wheel.png`. Pass when the page text moves up and the page number is unchanged.
- [ ] Lane 6. Press on a word, drag into the right strip, and release there. Save `ux3-select.png`. Pass when a selection exists and the page number is unchanged.
- [ ] Lane 7. Hover the right strip at 860, 1400, and 1920 window widths. Save `ux3-widths.png`. Pass when the shade widths are 60, 98, and 128px within 2px.
- [ ] Lane 8. Reveal the bar, click **Notes**, then hover the right strip. Save `ux3-notes.png`. Pass when no edge tab exists, the drawer opens, and the right strip sits left of the drawer.
- [ ] Lane 9. Open `The Art of Paying Attention.pdf` in Document view with Fit width, then click the right strip. Save `ux3-pdf.png`. Pass when the strip overlays the page edge and the PDF turns to the next page.
- [ ] Lane 10. Reveal the bar, type 7 in the page field, press Enter, then press the right arrow key. Save `ux3-keys.png`. Pass when the bar has no previous or next buttons and the page reads 8.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `reader_frame_ms` p95 for `reading-idle` and `reading-page-turn`. Trunk has no strips, so `reading-strip-hover` gets an absolute budget.
- [ ] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head. Run `ui-drive.ps1 -IdleCpu 10` the same way.
- [ ] Baseline. Record the trunk p95 values and idle CPU first.
- [ ] Rule. Fail when a head p95 exceeds trunk by more than 10 percent. Fail when `reading-strip-hover` p95 exceeds 33ms. Fail when head idle CPU exceeds trunk by more than 30ms per 10 seconds.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 2, 7, 8, and 9 screenshots into `target\ui-ux-review\UX-3-review-<slug>.png`. Include one screenshot of each Appendix A strip variant on the real reader so the operator picks the final look.
- [ ] Record a 30 to 60 second video of hover, page turns at several heights, the wheel over a strip, and the notes drawer. Save it as `target\ui-ux-review\UX-3-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-3 to the stack. The operator lands it bottom-up.

## Keep your place with a progress line and hide the idle scrollbar (UX-4)

**Depends on.** UX-3.

**Files.**

- [ ] Create `crates/iced-shell/src/progress_line.rs`.
- [ ] Edit `crates/iced-shell/src/document_scroll.rs`.
- [ ] Edit `crates/iced-shell/src/app.rs`.
- [ ] Edit `crates/iced-shell/src/reader_profile.rs`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Check the progress formula in `capture_progress` (app.rs:2076) first. An explorer suspects that EPUB progress mixes a per-chapter page number with a whole-book total. Write the failing test before any fix, and fix it only if the test fails.
- [ ] Draw a 2px line along the bottom edge of the reading area as a `stack!` layer. Fill it with `accent` up to the progress fraction over a `border` track. It shows at all times in a book.
- [ ] On hover over the bottom 24px, grow the line to 6px and show `Page 4 of 14 · 21%` in 11px muted text above it. The label also shows while the bar is shown.
- [ ] Clicking the line jumps to the page at that fraction, through the same path as the page field.
- [ ] Add the state `reading-progress-hover` to `profile_reader_frames`.
- [ ] Hide the document scrollbar while you read. The operator asked for this at the UX-3 review: a scrollbar that shows all the time is noise, and the progress line now keeps your place. `document_scroll.rs` draws its own 14px rail and thumb (`draw`, about line 500), so the change stays in that widget.
  - The thumb, and the rail's background fill under it, fade out over 150ms one second after the last scroll. While hidden, nothing is drawn, so the desk runs to the window edge.
  - They fade back in on any scroll movement: wheel, touchpad, keyboard, a page turn, a find jump, a page-field jump and middle-button autoscroll. They also show while the pointer is over the 14px rail and for the whole of a thumb drag.
  - The rail stays a live hit target while hidden. A press on it grabs or pages exactly as today, and the hand-over to the right page-turn strip is unchanged.
  - Keep the fade inside the widget's `State` with an explicit `Instant`, and drive it with `Shell::request_redraw_at` only while a fade or the one-second delay runs. Once hidden it requests nothing, so idle CPU stays at zero.
  - Leave the horizontal scrollbar as it is. It shows only while a page is zoomed wider than the window, and the strips already stop above it then.
  - Leave the library, Settings and panel scrollbars as they are. They are thin `ui::scrollbar` bars owned by iced.

**You see.**

- [ ] With the bar hidden, a thin accent line at the bottom shows how far through the book you are.
- [ ] Hovering the bottom edge shows the page and percent.
- [ ] While you read, there is no scrollbar on the right. It appears as soon as you scroll or point at the right edge, and fades about a second after you stop.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `app.rs` gains `epub_progress_grows_across_chapters`. It opens the Quiet Hours fixture, records progress at the last page of chapter 1 and the first page of chapter 2, and asserts the second is larger. Run `cargo test -p iced-shell --locked epub_progress`.
- [ ] `progress_line.rs` gains `click_fraction_maps_to_page`. It asserts that fraction 0.5 of 14 pages maps to page 7 and that fraction 1.0 maps to page 14. Run `cargo test -p iced-shell --locked click_fraction`.
- [ ] `document_scroll.rs` gains `idle_scrollbar_hides_and_returns`. It asserts that the thumb shows after a scroll, is hidden one second plus the fade later with no redraw requested, shows again while the pointer is over the rail, and that a press on the hidden rail still starts a thumb drag. Run `cargo test -p iced-shell --locked idle_scrollbar`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Twelve lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Hide the toolbar with F8 at trunk on page 4. Let the bar hide at head on page 4. Save `ux4-regression.png`. Pass when trunk shows no position and head shows a line filled to between 21 and 29 percent of its width.
- [ ] Lane 2. Hover the bottom 24px. Save `ux4-hover.png`. Pass when the line grows and the label reads `Page 4 of 14` with a percent.
- [ ] Lane 3. Click the line at 50 percent of its width. Save `ux4-jump.png`. Pass when the page field reads 7 or 8.
- [ ] Lane 4. Read the percent on the last page of chapter 1 and the first page of chapter 2. Save `ux4-chapters.png`. Pass when the second percent is larger.
- [ ] Lane 5. Note the percent, press Ctrl+W, and read the Continue Reading card. Save `ux4-library.png`. Pass when the card percent equals the line percent.
- [ ] Lane 6. Switch to dark appearance. Save `ux4-dark.png`. Pass when the filled part and the track are both visible.
- [ ] Lane 7. Press F11. Save `ux4-fullscreen.png`. Pass when the line shows in fullscreen.
- [ ] Lane 8. Open the PDF in Document view. Save `ux4-pdf.png`. Pass when the line shows the page fraction.
- [ ] Lane 9. Hover the bottom edge, then move away for 2 seconds. Save `ux4-no-jump.png`. Pass when the label is gone and the page text has the same y position as before the hover.
- [ ] Lane 10. Resize to 540x360 and hover the bottom edge. Save `ux4-narrow.png`. Pass when the label fits on one line inside the window.
- [ ] Lane 11. Read for 3 seconds without moving the pointer. Save `ux4-scrollbar-idle.png`. Wheel down once and save `ux4-scrollbar-scroll.png` within 300ms. Pass when the first shows no scrollbar on the right and the second shows the thumb, in both light and dark appearance.
- [ ] Lane 12. With the scrollbar hidden, press on the rail where the thumb sits and drag down 200px. Save `ux4-scrollbar-drag.png` mid-drag. Pass when the page scrolled, the thumb showed for the whole drag, and it hid again about a second after the release.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `reader_frame_ms` p95 for `reading-idle`. Trunk has no line, so `reading-progress-hover` gets an absolute budget.
- [ ] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [ ] Baseline. Record the trunk `reading-idle` p95 first.
- [ ] Rule. Fail when the head `reading-idle` p95 exceeds trunk by more than 10 percent. Fail when `reading-progress-hover` p95 exceeds 33ms.
- [ ] Rule. Fail when `idle_cpu_ms_per_10s` after the scrollbar has faded out is above trunk's.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, 6, and 11 screenshots into `target\ui-ux-review\UX-4-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of reading with the line, hovering it, jumping by click, and the scrollbar fading out and back in on scroll and on a rail hover. Save it as `target\ui-ux-review\UX-4-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-4 to the stack. The operator lands it bottom-up.

## Replace text pills with icons and an Aa panel (UX-5)

**Depends on.** UX-4.

**Files.**

- [ ] Create `crates/iced-shell/src/reading_panel.rs`.
- [ ] Edit `crates/iced-shell/src/app.rs`.
- [ ] Edit `crates/iced-shell/src/reading_ui.rs`.
- [ ] Edit `crates/iced-shell/src/pdf_reader.rs`.
- [ ] Edit `scripts/subset-icons.py` and `assets/fonts/MaterialSymbolsOutlined-Subset.ttf`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Make every bar control a 32px icon button with a tooltip that names the action and its shortcut. The bar holds **Library**, the book title in muted text, the page field, **Contents**, **Find**, **Listen**, **Notes**, **Aa**, the light and dark toggle, **Search**, and **Settings**. No two buttons share a glyph. Add the needed codepoints to `subset-icons.py` and regenerate the font.
- [ ] Add an **Aa** panel that drops down from its button. It holds the theme cards, text size, font, line spacing, side margins, zoom with **Fit width**, and **Use reading defaults**. Move the per-book controls from `reading_ui::settings` (reading_ui.rs:363) into it. In PDF views it shows only zoom and **Fit width**.
- [ ] Delete the toolbar's zoom buttons (`Control::FontDown`, `FitBookWidth`, `FontUp`), the PDF `zoom_controls` in the bar (pdf_reader.rs:1289), and `appearance_button` (app.rs:6201). Keep Ctrl+plus, Ctrl+minus, Ctrl+0, and Ctrl+Shift+F.
- [ ] Keep the bar shown while the **Aa** panel is open. Escape closes the panel and returns focus to **Aa**.
- [ ] Fit the bar on one row down to the 540px minimum width. Hide the book title first, then fold **Search** and **Settings** into an overflow button.

**You see.**

- [ ] The bar is one row of quiet icons with tooltips.
- [ ] **Aa** opens a panel where text size, font, spacing, margins, theme, and zoom change the open book.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `reading_panel.rs` gains `aa_panel_changes_the_open_book_only`. It raises the text size through the panel messages and asserts the book override changed while the library default did not. Run `cargo test -p iced-shell --locked aa_panel`.
- [ ] `app.rs` gains `every_bar_control_has_a_tooltip`. It walks the bar controls and asserts a non-empty tooltip for each. Run `cargo test -p iced-shell --locked bar_control`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Capture the bar at trunk and head, then hover each head icon. Save `ux5-regression.png`. Pass when trunk shows text pills and every head icon shows a tooltip with an action name.
- [ ] Lane 2. Open **Aa**, press text size plus twice, close the book, and reopen it. Save `ux5-size.png`. Pass when the text is larger after reopening.
- [ ] Lane 3. Pick the Soft theme in **Aa**. Save `ux5-theme.png`. Pass when the page uses Spectral on warm paper.
- [ ] Lane 4. In **Aa**, press zoom plus, then **Fit width**. Save `ux5-zoom.png`. Pass when the page grows and then fits the window width.
- [ ] Lane 5. Open the PDF in Document view and open **Aa**. Save `ux5-pdf.png`. Pass when only zoom and **Fit width** are enabled.
- [ ] Lane 6. Tab into the **Aa** panel, then press Escape. Save `ux5-keyboard.png`. Pass when focus moves through the panel controls and Escape returns focus to **Aa**.
- [ ] Lane 7. Open **Aa** and wait 5 seconds without moving. Save `ux5-hold.png`. Pass when the bar and panel are still shown.
- [ ] Lane 8. Press Ctrl+plus twice and Ctrl+0. Save `ux5-shortcuts.png`. Pass when the zoom grows and returns to 100 percent.
- [ ] Lane 9. Resize to 540x360 and reveal the bar. Save `ux5-narrow.png`. Pass when the bar is one row with no overlap.
- [ ] Lane 10. Press Ctrl+W, open Settings, and raise the default text size. Open a book that has no override. Save `ux5-defaults.png`. Pass when that book uses the new default.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `reader_frame_ms` p95 for `reading-bar-reveal` and `settings`. Trunk has no **Aa** panel, so a new state `reading-aa-open` gets an absolute budget.
- [ ] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [ ] Baseline. Record the trunk p95 values first.
- [ ] Rule. Fail when a head p95 exceeds trunk by more than 10 percent. Fail when `reading-aa-open` p95 exceeds 33ms.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, 5, and 9 screenshots into `target\ui-ux-review\UX-5-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the icon bar, the tooltips, and the **Aa** panel. Save it as `target\ui-ux-review\UX-5-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-5 to the stack. The operator lands it bottom-up.

## Float the panels and move contents to a left drawer (UX-6)

**Depends on.** UX-5.

**Files.**

- [ ] Edit `crates/iced-shell/src/app.rs`.
- [ ] Edit `crates/iced-shell/src/read_aloud.rs`.
- [ ] Edit `crates/iced-shell/src/notes_ui.rs`.
- [ ] Edit `crates/iced-shell/src/page_strips.rs`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Draw the read-aloud player (`read_aloud::player`, read_aloud.rs:1063) as a floating pill centred above the progress line, as a `stack!` layer.
- [ ] Draw the find bar (`find_bar`, app.rs:6019) as a floating box at the top right, below the bar's position.
- [ ] Move the EPUB contents list (app.rs:6590) into a left drawer with the same width rule as `sidebar_width`. **Contents** and Ctrl+T toggle it. Escape closes it.
- [ ] Below 900px window width, both drawers overlay the page instead of narrowing it.
- [ ] Move the left strip to the right edge of the contents drawer while the drawer is open.
- [ ] Keep errors, help, and notices in the `auxiliary` column. They are rare, and moving them is out of scope.

**You see.**

- [ ] Listen, Find, and Contents open without moving the page.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `app.rs` gains `panels_do_not_move_the_page`. It renders a book with Listen, Find, and Contents each open and asserts the first text row keeps its y. Run `cargo test -p iced-shell --locked panels_do_not_move`.
- [ ] `page_strips.rs` gains `left_strip_follows_the_contents_drawer`. It asserts that the left strip starts at the drawer width when the drawer is open. Run `cargo test -p iced-shell --locked left_strip_follows`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Start Listen at trunk and at head. Save `ux6-regression.png`. Pass when trunk moves the page down and head shows a pill at the bottom with the page unmoved.
- [ ] Lane 2. Press Ctrl+F, type `garden`, and press Enter. Save `ux6-find.png`. Pass when the find box shows at the top right, a match is highlighted, and the page did not move for the box.
- [ ] Lane 3. Press Ctrl+T, click chapter 2, then press Escape. Save `ux6-contents.png`. Pass when a left drawer opens, chapter 2 opens, and Escape closes the drawer.
- [ ] Lane 4. Open the contents drawer and click the left strip on page 3. Save `ux6-left-strip.png`. Pass when the strip sits right of the drawer and the page turns to 2.
- [ ] Lane 5. At 1100px width, open both drawers. Save `ux6-both.png`. Pass when the page fits between the drawers with no overlap.
- [ ] Lane 6. Use the pill's pause, slower, and faster buttons, then click the right strip. Save `ux6-player.png`. Pass when speech pauses and resumes, the speed label changes, and the page turns.
- [ ] Lane 7. At 860px width, open the contents drawer. Save `ux6-overlay.png`. Pass when the drawer overlays the page and the page keeps its width.
- [ ] Lane 8. Open the contents drawer and press Tab five times. Save `ux6-focus.png`. Pass when focus moves through the drawer entries.
- [ ] Lane 9. Open Find, then move to the top edge. Save `ux6-stacking.png`. Pass when the bar and the find box do not overlap.
- [ ] Lane 10. Delete the open book's file from the lane profile and reopen it. Save `ux6-error.png`. Pass when the error notice with **Locate** still shows.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `reader_frame_ms` p95 for `reading-idle`. Trunk renders panels in the column, so new states `reading-listen` and `reading-contents` get absolute budgets.
- [ ] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [ ] Baseline. Record the trunk `reading-idle` p95 first.
- [ ] Rule. Fail when the head `reading-idle` p95 exceeds trunk by more than 10 percent. Fail when `reading-listen` or `reading-contents` p95 exceeds 33ms.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, 3, and 7 screenshots into `target\ui-ux-review\UX-6-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of Listen, Find, and the contents drawer. Save it as `target\ui-ux-review\UX-6-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-6 to the stack. The operator lands it bottom-up.

## Continue to the next page when the wheel passes the end (UX-7)

**Depends on.** UX-6.

**Files.**

- [ ] Edit `crates/iced-shell/src/app.rs`.
- [ ] Edit `crates/iced-shell/src/pdf_reader.rs`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Today `Message::Scroll` (app.rs:4398) clamps the offset inside the page, so the wheel never turns it. Add an overscroll counter to the reader. When the page is at its bottom and wheel-down input adds up to 240px within 700ms, turn to the next page and land at its top. Wheel-up input at the top turns to the previous page and lands at its bottom. Any reverse input resets the counter.
- [ ] Count line deltas as 60px per line and pixel deltas as given, so a mouse and a touchpad behave alike.
- [ ] Never turn on Ctrl+wheel, which zooms (document_scroll.rs:272), or during middle-button auto-scroll.
- [ ] Apply the same rule in PDF Document view.

**You see.**

- [ ] Scrolling down past the end of a page goes to the next page after a short push. One small flick at the bottom does not.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `app.rs` gains `overscroll_turns_after_240px`, `one_notch_does_not_turn`, `reverse_input_resets_overscroll`, and `ctrl_wheel_never_turns`. They drive `Message::Scroll` with literal deltas and assert the page number and offset. Run `cargo test -p iced-shell --locked overscroll`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Scroll to the bottom of page 1, then send five wheel notches down at trunk and at head. Save `ux7-regression.png`. Pass when trunk stays on page 1 and head shows the top of page 2.
- [ ] Lane 2. At the bottom of page 1, send one notch down. Save `ux7-one-notch.png`. Pass when the page stays 1.
- [ ] Lane 3. At the top of page 3, send five notches up. Save `ux7-up.png`. Pass when page 2 shows at its bottom.
- [ ] Lane 4. Zoom out until a page fits the window, then send five notches down. Save `ux7-short.png`. Pass when the next page shows.
- [ ] Lane 5. Send twenty small pixel deltas of 20 down at a page bottom. Save `ux7-touchpad.png`. Pass when the page turns once.
- [ ] Lane 6. At a page bottom, hold Ctrl and send five notches down. Save `ux7-ctrl.png`. Pass when the zoom changes and the page number does not.
- [ ] Lane 7. Send notches up on the first page and down on the last page. Save `ux7-ends.png`. Pass when the page number does not change.
- [ ] Lane 8. Use middle-button auto-scroll to the page bottom and hold it there for 3 seconds. Save `ux7-autoscroll.png`. Pass when the page does not turn.
- [ ] Lane 9. At the bottom of the last page of chapter 1, send five notches down. Save `ux7-chapter.png`. Pass when chapter 2 opens at its top.
- [ ] Lane 10. Open the PDF in Document view at a zoom where a page scrolls, then wheel past its end. Save `ux7-pdf.png`. Pass when the next PDF page shows.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `reader_frame_ms` p95 for `reading-page-turn`.
- [ ] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [ ] Baseline. Record the trunk `reading-page-turn` p95 first.
- [ ] Rule. Fail when the head p95 exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, and 5 screenshots into `target\ui-ux-review\UX-7-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of reading through three pages with the wheel only. Save it as `target\ui-ux-review\UX-7-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-7 to the stack. The operator lands it bottom-up.

## Make the paper the lightest surface and split selected from focused (UX-8)

**Depends on.** UX-7.

**Files.**

- [ ] Edit `crates/iced-shell/src/book_style.rs`.
- [ ] Edit `crates/iced-shell/src/themes.rs`.
- [ ] Edit `crates/iced-shell/src/ui.rs`.
- [ ] Edit `crates/iced-shell/src/app.rs`.

**Build.**

- [ ] In every light theme variant, make the paper (`book_style::paper`, book_style.rs:103) at least as light as the desk (`book_style::desk`, :116). Tint the desk slightly instead of greying the paper. Keep the dark variants as they are.
- [ ] In `ui::button_style` (ui.rs:191), draw selected as a tinted fill with no outline. Draw keyboard focus as the 2px `accent` outline only. Today both draw a blue outline, so a selected tab looks focused.
- [ ] Centre the paper in the reading area at every width. Measure the gutters on both sides, because the 26px notes edge left with UX-3.

**You see.**

- [ ] In light themes the page is the brightest thing on screen.
- [ ] Selected controls are filled, and only the focused control has an outline.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `themes.rs` gains `light_paper_is_never_darker_than_the_desk`. It walks every theme's light palette and asserts the paper's relative luminance is at least the desk's. Run `cargo test -p iced-shell --locked light_paper`.
- [ ] `ui.rs` gains `selected_has_no_outline_and_focus_does`. It asserts border width 0 for selected and unfocused, and 2 for focused. Run `cargo test -p iced-shell --locked selected_has_no_outline`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Capture the Default light reading screen at trunk and head and sample one paper pixel and one desk pixel. Save `ux8-regression.png`. Pass when trunk paper is darker than its desk and head paper is at least as light as its desk.
- [ ] Lane 2. Repeat the samples for Soft, Clear, and Compact light. Save `ux8-themes.png`. Pass when the paper is at least as light as the desk in all three.
- [ ] Lane 3. Capture all four dark variants. Save `ux8-dark.png`. Pass when the paper edge is still visible against the desk.
- [ ] Lane 4. Open Contents so its button is selected, then Tab to another control. Save `ux8-selected.png`. Pass when **Contents** shows a fill without an outline and the focused control shows the outline.
- [ ] Lane 5. Open Settings and Tab across the theme cards. Save `ux8-cards.png`. Pass when the selected card and the focused card look different.
- [ ] Lane 6. On the Library, capture the sort control with **Title** selected. Save `ux8-sort.png`. Pass when **Title** is filled and has no outline.
- [ ] Lane 7. At 860px width, capture the page. Save `ux8-centre.png`. Pass when the left and right gutters differ by at most 2px.
- [ ] Lane 8. Open the notes drawer and capture the page. Save `ux8-centre-notes.png`. Pass when the paper is centred in the space left of the drawer, within 2px.
- [ ] Lane 9. Capture a page with a yellow highlight and a green highlight in light and dark. Save `ux8-highlights.png`. Pass when both highlights are readable on the new paper.
- [ ] Lane 10. Open the PDF in Document view. Save `ux8-pdf.png`. Pass when the PDF page keeps its own colours.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `reader_frame_ms` p95 for `reading-idle`, `library`, and `settings`.
- [ ] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [ ] Baseline. Record the trunk p95 values first.
- [ ] Rule. Fail when a head p95 exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, 4, and 7 screenshots into `target\ui-ux-review\UX-8-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the four light themes, then Tab through selected and focused controls. Save it as `target\ui-ux-review\UX-8-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-8 to the stack. The operator lands it bottom-up.

## Shrink the library header and clear its noise (UX-9)

**Depends on.** UX-8 in the stack. The owner may build from UX-1.

**Files.**

- [ ] Edit `crates/iced-shell/src/shelf.rs`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Replace the hero block in `Shelf::view` (shelf.rs:893), the `LOCAL WORKSPACE` label (:911), the `simPl Reader` heading (:916), and the tagline with one row. The row holds **Library** in Literata with the book count in muted text, and **Open** and **Add** buttons at the right. **Open** reads a file where it is without copying it, and **Add** imports a copy into the library (`lib-fix.md`, LF-1). Dropping a file into the window opens it in place, the same as **Open**, and highlights **Open**.
- [ ] Delete the `N active` pill (:971), the `Press Space to resume current` hint (:978), and the `N items` pill (:1056-1066). Space still resumes the latest book (app.rs:4997). The README shortcut table lists it, and UX-10 adds it to the F1 help.
- [ ] Show **Continue Reading** only when the library holds more than three books. With three or fewer, every book is already visible in the grid.
- [ ] Move the format badge off the cover art (:1843) into the line under the cover.

**You see.**

- [ ] At 1280x800, the first row of covers sits above the fold.
- [ ] A new library with one book shows a header and that one cover, with no duplicate card.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `shelf.rs` gains `continue_reading_needs_more_than_three_books`. It builds shelves of three and four books with progress and asserts the section is absent and present. Run `cargo test -p iced-shell --locked continue_reading_needs`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Capture the library with eight books at 1280x800 at trunk and head. Save `ux9-regression.png`. Pass when the trunk hero takes about 250px and the head shows the first cover row starting above y=400.
- [ ] Lane 2. Launch head with an empty profile. Save `ux9-empty.png`. Pass when the header, the **Open** and **Add** buttons, and a drop hint show.
- [ ] Lane 3. Import three books and open one, then import a fourth. Save `ux9-continue.png`. Pass when **Continue Reading** is absent with three books and present with four.
- [ ] Lane 4. Capture the EPUB and PDF covers. Save `ux9-badge.png`. Pass when the format label sits under the cover and no label covers the art.
- [ ] Lane 5. Click **Add**, then **Open**. Save `ux9-add.png`. Pass when each opens the Windows file dialog. Close each with Escape.
- [ ] Lane 6. With no control focused, press Space. Save `ux9-space.png`. Pass when the latest unfinished book opens.
- [ ] Lane 7. Click each sort option. Save `ux9-sort.png`. Pass when the grid order changes for **Title** and **Format**.
- [ ] Lane 8. Hover a card, click favourite, then click remove and cancel. Save `ux9-card.png`. Pass when the book moves to Favourites and the remove confirmation appears and cancels.
- [ ] Lane 9. Switch to dark appearance. Save `ux9-dark.png`. Pass when the header text, **Open** and **Add** are readable.
- [ ] Lane 10. Resize to 540x360. Save `ux9-narrow.png`. Pass when the header fits on one row and covers wrap.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `reader_frame_ms` p95 for `library`.
- [ ] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [ ] Baseline. Record the trunk `library` p95 first.
- [ ] Rule. Fail when the head p95 exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, 3, and 4 screenshots into `target\ui-ux-review\UX-9-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of the library from empty to four books. Save it as `target\ui-ux-review\UX-9-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-9 to the stack. The operator lands it bottom-up.

## Split Settings into sections (UX-10)

**Depends on.** UX-9.

**Files.**

- [ ] Edit `crates/iced-shell/src/app.rs`.
- [ ] Edit `crates/iced-shell/src/reading_ui.rs`.
- [ ] Edit `crates/iced-shell/src/profile_ui.rs`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Give the Settings modal (built in `overlays`, app.rs:5903 onward) a list of sections at its left. The sections are **Reading**, **Appearance**, **Listen**, **Translation**, **Library & data**, and **Shortcuts**. Show one section at a time.
- [ ] Put the reading defaults in **Reading**. When a book is open, **Reading** says that the open book's settings live in **Aa**.
- [ ] Put the theme cards, light and dark, and **Fullscreen** in **Appearance**.
- [ ] Delete the **Book zoom** row (app.rs:5926), because zoom lives in **Aa**.
- [ ] Update the F1 help (app.rs:6711) and **Shortcuts** so F8 reads as keep the bar visible and no line mentions an edge arrow. Add Space, which resumes the latest book from the Library, because UX-9 removes the on-screen hint.

**You see.**

- [ ] Settings opens on **Reading**, and each section fits without a long scroll.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] `app.rs` gains `settings_focus_stays_in_the_open_section`. It opens **Listen** and asserts that the focus list holds only the section list and the **Listen** controls. Run `cargo test -p iced-shell --locked settings_focus`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Open Settings at trunk and head, and at head click **Listen**. Save `ux10-regression.png`. Pass when trunk shows one long sheet and head shows only the Listen settings.
- [ ] Lane 2. Visit every section. Save `ux10-no-zoom.png`. Pass when no **Book zoom** row appears.
- [ ] Lane 3. Tab to the section list, press the down arrow twice, then press Escape. Save `ux10-keyboard.png`. Pass when the third section opens and Escape closes Settings.
- [ ] Lane 4. From the Library, raise the default text size in **Reading**, then open a book with no override. Save `ux10-defaults.png`. Pass when the book uses the new size.
- [ ] Lane 5. In **Appearance**, toggle dark and then **Fullscreen**. Save `ux10-appearance.png`. Pass when the app turns dark and enters fullscreen.
- [ ] Lane 6. Open **Translation**. Save `ux10-translation.png`. Pass when the language pair and the dictionary download controls show.
- [ ] Lane 7. With no book open, start a backup in **Library & data**. Save `ux10-backup.png`. Pass when the save dialog for the ZIP opens.
- [ ] Lane 8. Open **Shortcuts**, then press F1. Save `ux10-shortcuts.png`. Pass when both say F8 keeps the bar visible, both list Space for resuming from the Library, and no text mentions an edge arrow.
- [ ] Lane 9. Resize to 540x360 and open **Library & data**. Save `ux10-narrow.png`. Pass when the section scrolls inside the modal and nothing overlaps.
- [ ] Lane 10. Open Settings with a book open. Save `ux10-book.png`. Pass when **Reading** points to **Aa** for the open book.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [ ] Metric. `reader_frame_ms` p95 for `settings`, and the per-stage p95 from `profile_settings_scroll` (settings_profile.rs:98).
- [ ] Probe. Run both probes at trunk, head, trunk, head, trunk, head.
- [ ] Baseline. Record the trunk values first.
- [ ] Rule. Fail when a head p95 exceeds trunk by more than 10 percent.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 3, and 10 screenshots into `target\ui-ux-review\UX-10-review-<slug>.png`.
- [ ] Record a 30 to 60 second video of moving through every Settings section. Save it as `target\ui-ux-review\UX-10-review.mp4`.
- [ ] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [ ] Root's clean verdict at the exact head SHA.
- [ ] Bugbot triage done.
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged.
- [ ] The root appends UX-10 to the stack. The operator lands it bottom-up.

## Close the program

- [ ] Every box above is checked with its evidence.
- [ ] Reply to the operator with the report the execution playbook names. Give links to the stack root and tip, one verdict line per PR, and anything parked with the reason.

## Appendix A. Prototype evidence

The prototype is a throwaway iced 0.14 app in `docs/ui-ux-plan/prototype/`, committed on branch `docs/ui-ux-plan` in the commit that adds this plan. It uses the same pinned iced, tiny-skia, and the repo's `patches/`. It draws a long scrolling page, a floating bar, and two edge strips. The V key cycles three strip looks. To rebuild it, copy the root `Cargo.lock` into the prototype folder and run `cargo build --release --offline --target-dir ..\..\..\target\ui-ux-prototype` from an MSVC x64 prompt. The build finished in about 35 seconds on the planning machine.

The session drove it with a PowerShell script that calls `SetCursorPos`, `mouse_event`, and `CopyFromScreen`. UX-0 turns that script into `scripts/ui-drive.ps1`. The log is `docs/ui-ux-plan/prototype/run-log.txt`.

What the prototype proved, with the log line or screenshot:

- A floating bar and edge strips as `stack!` layers work in iced 0.14. The page under them keeps its position. See `evidence/proto-01-start-bar-visible.png` and `evidence/proto-02-bar-auto-hidden.png`.
- Auto-hide and top-edge reveal work with `window::frames()` subscribed only while a deadline is pending. The log shows `bar auto-hide` and `bar reveal (BarEnter)`.
- A press inside a strip turns the page. The log shows `strip press Right: page 4 -> 5`.
- A strip with `interaction(Pointer)` blocks wheel scrolling of the page under it. In the first run, wheel input over the strip logged no `page scrolled` line, while wheel input over the page logged `page scrolled y=180`. The cause is `iced_widget-0.14.2/src/stack.rs:266-272`, which hides the cursor from lower layers when an upper layer reports a mouse interaction.
- Forwarding the wheel fixes it. With `on_scroll` and `operation::scroll_by`, the log shows `strip wheel forwarded dy=180` followed by `page scrolled y=180`.
- The width formula gives 59px at 844px and 97px at 1384px, per the log lines `resized 844x661 strip=59` and `resized 1384x861 strip=97`. It reaches 128px at 2384px (`strip=128`).
- The strip looks are in `evidence/proto-03-hover-right-v0-gradient.png` (gradient shade and bare chevron), `evidence/proto-04-hover-right-v1-circle.png` (chevron in a faint circle), and `evidence/proto-05-hover-left-v2-flat.png` (flat 3.5 percent shade). `evidence/proto-06-narrow-hover-right.png` shows the 59px strip at 860px width.
- A drop shadow under the bar rendered as a hard dark band in tiny-skia, and a 94 percent opaque bar let page text show through mid-fade. See `evidence/proto-07-narrow-top-reveal.png`. UX-2 therefore uses an opaque bar with a hairline border.

The current app, before any change, is in `evidence/before-reading-1400.png`, `evidence/before-reading-860.png`, `evidence/before-toolbar-hidden-f8.png`, and `evidence/before-library.png`. They were captured from a release build of `main` at 17717f2 with an isolated profile and the demo fixtures.

What stays unproven:

- The final strip look. The operator picks it at the UX-3 review gate from the three variants rendered in the real reader.
- Whether a selection drag stalls while the cursor crosses a strip. The levitated cursor gives the page no position there. UX-3 lane 6 checks it.
- The suspected EPUB progress bug in `capture_progress`. UX-4 writes the test first.
- The 240px and 700ms overscroll threshold. UX-7 lanes 2 and 5 check it with a mouse and a touchpad pattern. The operator tunes it at review.
- Whether `subset-icons.py` reproduces the shipped font byte for byte. UX-0 lane 10 checks it.

## Appendix B. Alternatives rejected

- A bar that pushes the page down when it appears. The operator chose no jumping. It also moves the text under the reader's eyes on every reveal.
- Keeping the `‹` notes tab and hiding it until hover. It would sit inside the right page-turn strip, so one hover would mean two things. The operator approved moving it into the bar.
- Making the strips the whole empty gutter beside the page. The gutter is anywhere from 0 to over 600px wide, so the target would change size with zoom. The operator chose a strip whose width follows the window.
- Strips without a pointer cursor, which would let the wheel pass through on its own. The page's text cursor would then show over the strip, and the strip would not read as clickable. Forwarding the wheel keeps both.
- A translucent bar with backdrop blur, as `design/DESIGN.md` suggests. tiny-skia has no backdrop blur, and plain translucency let text show through in the prototype.
- Turning the page on mouse release inside a strip. A selection drag that ends in the strip would then turn the page.
- Removing only the `Mac` variant. Saved preferences with `"window_controls":"mac"` would fail to load and turn off saving. Deleting the field avoids both.
- Keeping macOS buttons as a hidden option. simPl ships only for Windows, and the option doubles the title-bar layout code in `chrome.rs`.
- Repaginating pages to the viewport height so the wheel never needs to cross a page. It would break the stable page numbers that the README promises and that bookmarks rely on.
- Leaving the document scrollbar on screen at all times. The operator found it noisy while reading. Once the progress line keeps your place, an idle thumb adds nothing.
- Moving the scrollbar rail to hug the paper. The rail is custom-drawn in `document_scroll.rs`, and moving it costs more than letting the strip sit inside it.

## Appendix C. Risks

- Live lanes move the global cursor, so lanes on one desktop must run one at a time. All PRs. The root schedules lanes in series or on separate Windows sessions.
- No CI runs on PRs, because `release.yml` runs only on tags. All PRs. Owners paste `dev.ps1 check` output, and the gates lane reruns it.
- Focus can land on a hidden bar control. UX-2. Bar controls stay in the focus order, and focusing one reveals the bar. Lane 5 checks it.
- A frame subscription that never stops would spin the CPU. UX-2, UX-3, and UX-4. The perf rule on `idle_cpu_ms_per_10s` fails such a head.
- Wheel forwarding may scroll by a different amount than native scrolling. UX-3. The owner matches the scrollable's own line height.
- The PDF Document view has its own scroll and selection in `pdf_reader.rs`. UX-3 and UX-7. Lanes 9 and 10 cover it.
- Regenerating the icon font needs network access and `fontTools`. UX-0, UX-3, and UX-5. The script pins the source blob SHA.
- Ignored preview tests that never set a window style render Mac chrome today, so their output changes. UX-1. The owner reruns `render_reading_comfort_previews` and attaches the new images.
- The bar may not fit one row at 540px before UX-5 folds controls into overflow. UX-2. Lane 9 checks 860px only, and UX-5 lane 9 checks 540px.
- A hidden scrollbar is harder to find. UX-4. The rail stays live while hidden, and the thumb returns on any scroll and on a hover over the right edge. Lanes 11 and 12 check it.
- Bugbot may not be installed on this repository. All PRs. The owner records that once.

## Appendix D. Links and reading list

Read these before editing.

- `design/DESIGN.md`, which already asks for chrome that auto-hides and a slim progress line at the bottom.
- `reader-comfort-review-2026-09-30.md`, which covers fit width, fullscreen, and Escape order.
- `README.md` sections on shortcuts and reading modes, which several PRs must update.
- `crates/iced-shell/src/chrome.rs`, `app.rs` from `view` (:6571) to `paged_book_view` (:6464), `notes_ui.rs` `sidebar` (:1468), and `document_scroll.rs`.
- `iced_widget-0.14.2/src/stack.rs` lines 236 to 280 in the cargo registry, for how layers pass events.

UX-2 and UX-3 run the **how** skill (`~/.claude/skills/how/SKILL.md`) before building, because they add a state machine and a new input layer. Run the **interrogate** skill (`~/.claude/skills/interrogate/SKILL.md`) on both before the code-ready report, because they change the main reading interaction. Every owner keeps a `decisions.tsv` trail per `~/.claude/skills/show-me-your-work/SKILL.md` and returns it uncommitted in its report.
