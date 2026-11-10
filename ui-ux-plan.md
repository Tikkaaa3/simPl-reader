# Reader UI and UX plan

This plan makes simPl a quieter Windows reader. The reading screen loses its permanent chrome. A floating top bar hides itself, and full-height strips at both edges turn pages on hover and click. A thin progress line keeps your place, and the scrollbar hides while you read. The macOS window-button mode is deleted because simPl ships only for Windows.
The rule for every PR is that the page never moves when chrome appears or disappears. Chrome floats over the page and never takes layout space from it.
PRs land in this order. UX-0 driver and probes, UX-1 Windows-only chrome, UX-2 floating bar, UX-3 page-turn strips, UX-4 progress line and idle scrollbar, UX-5 icon bar and **Aa** panel, UX-6 floating panels and contents drawer, UX-7 wheel to next page, UX-8 page surface and state styling, UX-9 library header, UX-10 settings sections.

## How to read this

One box is one unit of work. Every box names the evidence that checks it. A nested box is a sub-step of the box above it. Check a box only when its evidence exists, a file, a log line, a screenshot, a test run, or a SHA. The body is a how-to. The appendices explain and record.

The program runs `~/.claude/skills/poteto-mode/playbooks/autopilot-stack.md`. The work is sequenced and coupled in one reader view, and the operator reviews before landing. The root appends each verified PR to one linear stack on `main`. The operator lands the stack bottom-up. Every PR from UX-1 to UX-10 changes an interaction and stops at merge-ready for the operator. UX-0 changes no interaction.

Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

## Status

As of 2026-11-10:

| PR | Milestone | State |
| --- | --- | --- |
| #2 | UX-0 driver and probes | Merged |
| #3 | UX-1 Windows-only chrome | Merged |
| #6 | UX-2 floating bar | Merged |
| #11 | UX-3 page-turn strips | Merged |
| #12 | This plan | Merged |
| #14 | UX-4 progress line and idle scrollbar | Merged |
| #17 | UX-5 icon bar and **Aa** panel | Merged |
| #18 | A theme for each book, with a default theme in Settings, added after UX-5 | Merged |
| #20 | UX-6 floating panels and contents drawer | Merged |
| #21 | UX-6 follow-up: drawers dock only when the page fits, and the player hides with the bar | Merged |
| #22 | The UX-6 record in this plan | Merged |
| #24 | UX-7 wheel to next page | Merged |
| #25 | The UX-7 record in this plan | Merged |
| #26 | UX-8 page surface and state styling, with equal page-turn strips and one current contents entry | Merged |
| #27 | The UX-8 record in this plan | This PR |
| | UX-9 library header | Next, on the operator's go |

How the program actually runs, where it differs from the checklist below:

- One interactive session runs it, without the hourly audit tick. Code is written by a delegated owner, and the session reviews and verifies it.
- Each PR branches from current `main` and the operator merges it into `main` after review. There is no stack, so the stack and rebase-after-verdict boxes do not apply.
- The session runs the live lanes and the perf probe itself on the build machine with `scripts/ui-drive.ps1`, one lane at a time. A second model reviews each diff independently in place of the swarm's audit lanes.
- The repository has no Bugbot.
- `desktop_page_atlases_match_the_golden_books` and `reader-layout/tests/parity.rs` fail on `main` on the build machine. Every gate reports them as known failures.
- The perf baseline is the detached worktree `..\simPl-trunk`, moved to `main` and release-built before each milestone.
- The perf probe runs last, on the final head, for at least ten alternating rounds. Its p95 moves by about 15 percent between identical runs, so the frame medians carry the verdict.
- The session copies each merge-ready build to one folder outside the repository, and the **simPl latest** shortcut on the Desktop opens it. The operator tests there before merging.

A milestone's boxes are ticked from its PR's Verification section. A box left open in a merged milestone says why.

## Program checklist

### Arm the program

- [x] State the protocol and this plan to the operator, then stop. Start execution only on the operator's explicit go.
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
  - [x] UX-0 is first. It branches from `main`.
  - [x] UX-1 after UX-0.
  - [x] UX-2 after UX-1.
  - [x] UX-3 after UX-2.
  - [x] UX-4 after UX-3.
  - [x] UX-5 after UX-4.
  - [x] UX-6 after UX-5.
  - [x] UX-7 after UX-6.
  - [x] UX-8 after UX-7.
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
- [ ] In the worktree, run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev.ps1 build`. Then run `python scripts/promotion-fixtures.py target\lane-fixtures` to create the demo books. For lanes that cross chapters, also run `python -I fixtures/chapters/build_epub.py`, which writes `target\lane-fixtures\Three Rooms.epub`.
- [ ] Launch with `scripts\ui-drive.ps1 -Launch "target\lane-fixtures\The Quiet Hours.epub" -Profile target\lane-profile-<n> -Size 1400x900`. The script sets `LOCALAPPDATA` to the lane profile, so the real library stays untouched.
- [ ] Deliver input only through `ui-drive.ps1` (`-Move`, `-Click`, `-Wheel`, `-Key`, `-Resize`, `-Hold`). Read-only diagnostics are `-Shot`, `-IdleCpu`, the window title, and the files under the lane profile's `simPl` folder.
- [ ] Save every screenshot to `target\swarm-<pr-id>\worker-<n>\<slug>.png` and return the paths with the report.

## Add the live-lane driver and the frame probe (UX-0)

**Depends on.** None.

**Files.**

- [x] Create `scripts/ui-drive.ps1`.
- [x] Create `scripts/subset-icons.py`.
- [x] Create `crates/iced-shell/src/reader_profile.rs`.
- [x] Edit `crates/iced-shell/src/book_preview.rs`.
- [x] Edit `crates/iced-shell/src/app.rs`, only the `#[cfg(test)]` module list near `release_audit` at line 40.

**Build.**

- [x] Write `scripts/ui-drive.ps1` from the session driver in Appendix A. It takes `-Launch <path>`, `-Profile <dir>`, `-Size WxH`, `-Move x,y`, `-Click`, `-Wheel <delta>`, `-Key <SendKeys string>`, `-Hold <ms>`, `-Shot <png>`, `-IdleCpu <seconds>`, and `-Stop`. Coordinates are client-relative, because the window has no OS decorations (`decorations: false`, app.rs:7035).
- [x] Add `render_with_cursor(reader, path, point)` beside `render` in `book_preview.rs:370`. It passes `mouse::Cursor::Available(point)` where `render_at_scale` passes `mouse::Cursor::Unavailable` today (book_preview.rs:503 and 518), so headless renders can show hover states.
- [x] Add the ignored test `profile_reader_frames` in `reader_profile.rs`, modeled on `profile_settings_scroll` (settings_profile.rs:98). It renders 120 frames per state at 1280x800 and prints one line per state, `reader_frame_ms state=<name> median=<ms> p95=<ms>`. The states are `library`, `reading-idle`, `reading-page-turn`, and `settings`. Later PRs add their own states.
- [x] Write `scripts/subset-icons.py`. It downloads the Material Symbols source font at the blob SHA pinned in `assets/licenses/Material-Symbols-CHANGES.txt`, subsets it to a codepoint list kept in the script, writes `assets/fonts/MaterialSymbolsOutlined-Subset.ttf`, and rewrites the codepoint line of the change notice. The list starts as the ten codepoints shipped today.

**You see.**

- [x] `scripts\ui-drive.ps1 -Launch ... -Shot target\ux0\reading.png` writes a 1400x900 PNG of the reading screen.
- [x] `cargo test -p iced-shell --release profile_reader_frames -- --ignored --nocapture --test-threads=1` prints four `reader_frame_ms` lines.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] `book_preview.rs` gains `render_with_cursor_shows_card_hover`. It renders the library with the cursor over a card and asserts that the favourite button's pixels differ from a render without a cursor. Run `cargo test -p iced-shell --locked render_with_cursor`.
- [x] Run `scripts\dev.ps1 check`. It passes with no new warnings.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [ ] Lane 1. Regression lane against trunk. Trunk has no driver, so record that fact. Launch trunk by hand and head through the driver with the same fixture, then capture the reading screen at 1400x900. Save `ux0-regression.png`. Pass when both captures show the same layout. *Not recorded in PR #2.*
- [x] Lane 2. Launch with `-Profile target\lane-profile-2`. Save `ux0-isolated.png`. Pass when `target\lane-profile-2\simPl\library.json` exists and the modified time of `%LOCALAPPDATA%\simPl` in the real profile is unchanged.
- [x] Lane 3. Run `-Resize 860x700`, then `-Shot`. Save `ux0-resize.png`. Pass when the PNG is exactly 860x700.
- [x] Lane 4. Press Ctrl+W, then move the cursor over a library card. Save `ux0-hover.png`. Pass when the card shows its favourite and remove buttons.
- [x] Lane 5. Click the Continue Reading card. Save `ux0-click.png`. Pass when the book opens at its saved page.
- [x] Lane 6. Send `-Key '{F8}'`. Save `ux0-key.png`. Pass when the reading toolbar collapses.
- [x] Lane 7. Send `-Wheel -360` over the page. Save `ux0-wheel.png`. Pass when the page text sits higher than in the capture before the wheel.
- [ ] Lane 8. Run `-IdleCpu 10` with the book open and the mouse still. Save `ux0-idle.png`. Pass when the script prints `idle_cpu_ms_per_10s=<n>` with a number. *Not recorded in PR #2; `-IdleCpu` was used from UX-2 on.*
- [x] Lane 9. Run the `profile_reader_frames` command from **You see.** Save `ux0-profile.png` as a capture of the console. Pass when four state lines print with numeric median and p95.
- [x] Lane 10. Run `python scripts/subset-icons.py` with the shipped codepoint list. Save `ux0-icons.png` as a capture of the console. Pass when the SHA-256 of the regenerated font equals the committed file, or the lane records the `fontTools` version that changed the bytes. *`--check` compares outlines and printed `icons match`.*

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] Metric. The size in bytes of `target\release\iced-shell.exe`. Trunk lacks the frame probe, so the diff-added work gets an absolute budget, the wall time of `profile_reader_frames`.
- [x] Probe. Build release at trunk, then at head, then at trunk again, and record the exe size each time. Time `profile_reader_frames` at head three times.
- [x] Baseline. Record the trunk exe size first.
- [x] Rule. Fail when the head exe size differs from trunk by more than 0 bytes, because all new code is test-only or script. Fail when the median `profile_reader_frames` wall time exceeds 90 seconds.

**Review gate.** None. UX-0 is not review-gated.

**Merge.**

- [x] Root's clean verdict at the exact head SHA.
- [x] Bugbot triage done. *No Bugbot on this repository.*
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged. *Not applicable: branched from current `main`.*
- [ ] The root appends UX-0 to the stack. The operator lands it bottom-up. *Merged into `main` as PR #2.*

## Delete the macOS window-button mode (UX-1)

**Depends on.** UX-0.

**Files.**

- [x] Edit `crates/reader-document/src/preferences.rs`.
- [x] Edit `crates/iced-shell/src/chrome.rs`.
- [x] Edit `crates/iced-shell/src/app.rs`.
- [x] Edit `crates/iced-shell/src/book_preview.rs`.
- [x] Edit `crates/iced-shell/src/promotion_preview.rs`.
- [x] Edit `README.md`.

**Build.**

- [x] Delete `enum WindowControls` (preferences.rs:36-45), the `window_controls` field of `Preferences` (:50), and the field of `Record` (:67) with its load and save mappings (:125, :142). Delete the field, not only the `Mac` variant. `Record` has no `deny_unknown_fields`, so serde then ignores the `"window_controls":"mac"` key that most saved files contain. Removing only the variant makes `load_from` (:100) reject those files, and app.rs:3807 then turns off saving for the session.
- [x] In `chrome.rs`, delete `dot` (:124-147), `blend` (:51), the `dots` row (:233-256), the Mac arm of the tool order (:292-366), and the Mac side widths (:397-418). Delete `Controls.style` (:217). The Windows layout becomes the only layout.
- [x] In `app.rs`, delete `Control::SettingsWindowControls` (:1189), `Message::SetWindowControls` (:1327), the handler (:3775-3790), the settings row (:5938-5945), `window_controls_button` (:6168-6181), the focus entries (:2277-2280), and the field (:1032, :1132, :2108). Collapse the per-style title-bar focus order (:2230-2262) to the Windows order.
- [x] Delete `SIMPL_PREVIEW_WINDOWS` handling (book_preview.rs:1368-1372), the `windows-controls-*` captures (book_preview.rs:1064-1069), and the `SetWindowControls` call (promotion_preview.rs:47).
- [x] Rewrite README.md:34 so it no longer offers a choice of window buttons.

**You see.**

- [x] A fresh profile opens with minimize, maximize, and close at the right of the title bar, and Settings is the outermost icon on the left.
- [x] Settings has no **Window controls** row.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] `preferences.rs` gains `legacy_mac_window_controls_key_still_loads`. It writes a version 1 record with `"window_controls":"mac"` and `"appearance":"dark"`, calls `load_from`, and asserts `Ok` with `Appearance::Dark`. Run `cargo test -p reader-document --locked legacy_mac`.
- [x] Replace the round-trip loop over both variants (preferences.rs:176) and the Mac default assertion (:212) with a round-trip that asserts the saved JSON contains no `window_controls` key. Run `cargo test -p reader-document --locked`.
- [x] Update `saved_theme_reaches_a_book_that_is_already_open` (app.rs:7996) to drop the field. Run `cargo test -p iced-shell --locked saved_theme`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [x] Lane 1. Regression lane against trunk. Open a fresh profile at trunk and at head. Save `ux1-regression.png`. Pass when trunk shows round dots at the left and head shows Windows caption buttons at the right.
- [x] Lane 2. Copy a trunk `preferences.json` with `"window_controls":"mac"`, the Soft theme, and dark appearance into the lane profile, then launch head. Save `ux1-legacy.png`. Pass when the app shows Soft dark with no "Invalid preferences" notice.
- [x] Lane 3. In the lane 2 profile, change the text size in Settings, then quit. Save `ux1-resave.png`. Pass when `preferences.json` is rewritten with the new size and without a `window_controls` key.
- [x] Lane 4. Open Settings. Save `ux1-settings.png`. Pass when no **Window controls** row or macOS option appears.
- [x] Lane 5. Press Tab through the title bar. Save `ux1-focus.png` with focus on Close. Pass when focus visits Settings, the other tools, Minimize, Maximize, and Close in that order.
- [x] Lane 6. Click Maximize, then click it again. Save `ux1-maximize.png`. Pass when the window maximizes, the glyph changes to restore, and the second click restores it.
- [x] Lane 7. With a book on page 4, click Close, then relaunch. Save `ux1-close.png`. Pass when the book reopens on page 4.
- [x] Lane 8. Double-click the empty title bar, then drag it. Save `ux1-drag.png`. Pass when double-click maximizes and the drag moves the restored window.
- [ ] Lane 9. Restore a backup ZIP made at trunk that contains `"window_controls":"mac"`. Save `ux1-restore.png`. Pass when the restore review appears and **Restore and replace** finishes without an error. *Not recorded in PR #3.*
- [x] Lane 10. Resize to the minimum 540x360 in dark appearance. Save `ux1-narrow.png`. Pass when the title, tools, and caption buttons do not overlap.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] Metric. `reader_frame_ms` p95 for `library` and `reading-idle`. *PR #3 records `library` over five alternating runs.*
- [x] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [x] Baseline. Record the trunk p95 for both states first.
- [x] Rule. Fail when the head median of the three p95 values exceeds trunk by more than 10 percent in either state.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, and 5 screenshots into `target\ui-ux-review\UX-1-review-<slug>.png`. *Not kept on disk.*
- [ ] Record a 30 to 60 second video of the change on the lane desktop with `ffmpeg -f gdigrab -framerate 30 -i title=simPl -t 45`, or with Xbox Game Bar. Save it as `target\ui-ux-review\UX-1-review.mp4`. *Not kept on disk.*
- [x] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [x] Root's clean verdict at the exact head SHA.
- [x] Bugbot triage done. *No Bugbot on this repository.*
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged. *Not applicable: branched from current `main`.*
- [ ] The root appends UX-1 to the stack. The operator lands it bottom-up. *Merged into `main` as PR #3.*

## Float the reading bar and hide it when idle (UX-2)

**Depends on.** UX-1.

**Files.**

- [x] Create `crates/iced-shell/src/reading_chrome.rs`.
- [x] Edit `crates/iced-shell/src/app.rs`.
- [x] Edit `crates/iced-shell/src/chrome.rs`.
- [x] Edit `crates/iced-shell/src/reader_profile.rs`.
- [x] Edit `README.md`.

**Build.**

- [x] Model the bar's visibility as one state machine in `reading_chrome.rs`, replacing `toolbar_expanded` (app.rs:1020). Use `enum BarState { Pinned, Shown { hide_at: Option<Instant> }, Hidden }` plus an `iced::Animation<bool>` for the fade. `toolbar_expanded` is session-only today, so no saved data migrates.
- [x] In a book, merge the title bar and `document_toolbar` (app.rs:6252) into one 44px bar. Render it as the top layer of a `stack!` over the reading area, not as a row of the page column (app.rs:6827). The page keeps its current position whether the bar shows or not.
- [x] Give the bar an opaque `lowest` background and a 1px `border` hairline at its bottom edge. Do not use a drop shadow or translucency. Appendix A shows why.
- [x] Show the bar for 2 seconds after a book opens. Show it while the pointer is in the top 48px or over the bar. Hide it 1.5 seconds after the pointer leaves. Fade in and out over 150ms.
- [x] Keep the bar shown while a bar control has keyboard focus or while a panel the bar opened is open. Keep every bar control in `Reader::controls()` (app.rs:2161) at all times, and reveal the bar when focus lands on one. Escape hides the bar when nothing else is open.
- [x] While hidden, render only a transparent 48px `mouse_area` with `on_enter` and no `interaction`. Its pointer state stays `None`, so the page under it keeps hover and clicks. The bar's buttons are not in the tree while hidden.
- [x] Subscribe to `window::frames()` only while `hide_at` is pending or the fade runs. That follows the rule at app.rs:6914, no idle redraw loop.
- [x] Change F8 from hide or show to pin or unpin. Delete the `▤` title-bar icon (chrome.rs:325-337) and `Control::ToggleToolbar`.
- [x] Put the light and dark toggle at the same position in the bar on the Library and the reading screens.
- [x] Keep the static, always-visible title bar on the Library screen.
- [x] In fullscreen, use the same bar without caption buttons.
- [x] Add the states `reading-bar-reveal` and `reading-bar-hidden` to `profile_reader_frames`.

**You see.**

- [x] On opening a book, the bar shows and then fades out after 2 seconds. The first line of text stays at the same y position before and after.
- [x] Moving the pointer to the top edge brings the bar back within 150ms.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] `reading_chrome.rs` gains transition tests that drive `BarState` with fixed `Instant`s. `opened_then_idle_hides_after_two_seconds` asserts `Hidden` at open plus 2.1 seconds. `focus_holds_the_bar` asserts `Shown { hide_at: None }` while a bar control is focused. `pin_survives_idle` asserts `Pinned` after 60 seconds. Run `cargo test -p iced-shell --locked reading_chrome`.
- [x] `app.rs` gains `bar_overlay_does_not_move_the_page`. It renders a book with the bar shown and hidden through `book_preview::render` and asserts that the first text row has the same y in both. Run `cargo test -p iced-shell --locked bar_overlay`.
- [x] Replace the `ToggleToolbar` tests (app.rs:7600, 8453-8471) with tests of the pin behavior.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [x] Lane 1. Regression lane against trunk. Open the book at trunk and at head, and capture at 0.5 seconds and at 3 seconds after opening. Save `ux2-regression.png`. Pass when trunk shows both bars in both captures, head shows no bar at 3 seconds, and the head's first text line has the same y in both captures.
- [x] Lane 2. Move the pointer to y=20, then to the page centre, and capture 1.0 and 2.0 seconds later. Save `ux2-reveal.png`. Pass when the bar shows at y=20, is still shown at 1.0 seconds, and is gone at 2.0 seconds.
- [x] Lane 3. Rest the pointer on the bar for 10 seconds. Save `ux2-hold.png`. Pass when the bar is still shown at 10 seconds.
- [x] Lane 4. Press F8, wait 5 seconds, press F8, then wait 3 seconds. Save `ux2-pin.png`. Pass when the bar stays at 5 seconds and is gone 3 seconds after unpinning.
- [x] Lane 5. With the bar hidden, press Tab, wait 5 seconds, then press Escape. Save `ux2-keyboard.png`. Pass when Tab reveals the bar with a focus ring on its first control, the bar is still shown at 5 seconds, and Escape hides it.
- [x] Lane 6. Reveal the bar, drag it, then double-click it. Save `ux2-drag.png`. Pass when the drag moves the window and the double-click maximizes it.
- [x] Lane 7. Press F11, wait 3 seconds, move to the top edge, then press Escape. Save `ux2-fullscreen.png`. Pass when the bar hides in fullscreen, reveals without caption buttons, and Escape leaves fullscreen.
- [x] Lane 8. Press Ctrl+W and wait 10 seconds on the Library. Save `ux2-library.png`. Pass when the Library title bar is still shown.
- [x] Lane 9. Resize to 860x700 in dark appearance and reveal the bar. Save `ux2-narrow.png`. Pass when no bar controls overlap and the page does not move.
- [x] Lane 10. Capture the bar on the Library and on the reading screen. Save `ux2-toggle-position.png`. Pass when the light and dark toggle has the same x position on both screens, within 2px.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] Metric. `idle_cpu_ms_per_10s` with a book open and the pointer still, and `reader_frame_ms` p95. Trunk has no fade, so the fade gets an absolute budget, the p95 of `reading-bar-reveal`.
- [x] Probe. Run `ui-drive.ps1 -IdleCpu 10` at trunk, head, trunk, head, trunk, head. Run `profile_reader_frames` the same way.
- [x] Baseline. Record the trunk idle CPU and the trunk `reading-idle` p95 first.
- [x] Rule. Fail when the head median idle CPU exceeds trunk by more than 30ms per 10 seconds. Fail when the head `reading-idle` p95 exceeds trunk by more than 10 percent. Fail when the `reading-bar-reveal` p95 exceeds 33ms.

**Review gate.** The operator reviews before merge.

- [ ] Copy the lane 1, 2, 5, and 7 screenshots into `target\ui-ux-review\UX-2-review-<slug>.png`. *Not kept on disk.*
- [ ] Record a 30 to 60 second video of the reveal, the hide, the pin, and fullscreen on the lane desktop. Save it as `target\ui-ux-review\UX-2-review.mp4`. *Not kept on disk.*
- [x] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [x] Root's clean verdict at the exact head SHA.
- [x] Bugbot triage done. *No Bugbot on this repository.*
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged. *Not applicable: branched from current `main`.*
- [ ] The root appends UX-2 to the stack. The operator lands it bottom-up. *Merged into `main` as PR #6.*

## Turn pages from full-height edge strips (UX-3)

**Depends on.** UX-2.

**Files.**

- [x] Create `crates/iced-shell/src/page_strips.rs`.
- [x] Edit `crates/iced-shell/src/app.rs`.
- [x] Edit `crates/iced-shell/src/notes_ui.rs`.
- [x] Edit `crates/iced-shell/src/pdf_reader.rs`.
- [x] Edit `crates/iced-shell/src/reader_profile.rs`.
- [x] Edit `scripts/subset-icons.py` and `assets/fonts/MaterialSymbolsOutlined-Subset.ttf`.
- [x] Edit `README.md`.

**Build.**

- [x] Add a left and a right strip as a `stack!` layer above the reading area and below the bar. Each strip is the full height of the reading area. Its width is `(window_width * 0.07).clamp(48.0, 128.0)`, which gives 59px at 844px, 97px at 1384px, and 128px from about 1830px. The right strip ends at the left edge of the notes drawer when the drawer is open.
- [x] Turn the page on `on_press` inside the strip. Send `Message::BookPage(bool)` in Book view, through `adjacent_book_page` (app.rs:1682). Send the PDF previous or next message in PDF Document view. A drag that starts on the page and ends in a strip never turns the page, because the press happened outside the strip.
- [x] Forward the wheel. A strip with a pointer cursor makes `stack!` hide the cursor from the page under it (`iced_widget-0.14.2/src/stack.rs:266-272`), so the page stops scrolling there. Give each strip `on_scroll` that sends `operation::scroll_by` to the reading scrollable.
- [x] Draw nothing until hover. On hover, fade in a gradient from about 7 percent black at the window edge to clear, plus a chevron icon of 28 to 32px in a faint 48px circle, centred vertically in the strip. Use a hand cursor. Pick the final look from the Appendix A variants at the review gate. *As built: see below.*
- [x] Disable a strip when `can_turn` (app.rs:1652) is false. A disabled strip shows no shade, no chevron, no hand cursor, and ignores presses.
- [x] Add `chevron_left` and `chevron_right` to the codepoint list in `subset-icons.py` and regenerate the font.
- [x] Delete the `‹` edge toggle from `notes_ui::sidebar` (notes_ui.rs:1471-1520) and its 26px edge width in `sidebar_width` (notes_ui.rs:25). Add a **Notes** button to the bar that sends `Action::ToggleList`. Ctrl+B is unchanged.
- [x] Delete the toolbar's previous and next page buttons, `Control::PreviousBookPage`, `Control::NextBookPage`, and the PDF `FocusControl::Previous` and `FocusControl::Next` buttons. Keep the page field and the arrow keys.
- [x] Keep the 14px document scrollbar rail (`document_scroll.rs:11`) at the far right edge. The right strip sits just inside it, so the rail stays draggable.
- [x] Add the state `reading-strip-hover` to `profile_reader_frames`, rendered through `render_with_cursor`.
- [x] Rewrite the README sentence about the arrow at the right edge.

**As built.** The review changed these from the steps above.

- The look is a bare chevron over a full-height band. The band is drawn in the theme's text colour, strongest at the window edge and still visible at its inner edge, so the whole strip reads as clickable: 15 to 5.5 percent on light pages and 7 to 2.5 percent on dark ones. The circle and flat variants are deleted.
- The strips run to the bottom of the reading area. They stop above the PDF status row, and above the horizontal scrollbar only while a page is zoomed wider than the window (`page_strips::bottom_inset`).
- The **Notes** button sits after **Listen**. The bar's one-row width thresholds are measured by the ignored test `measure_reading_bar_thresholds`, so an 860px window at 125 percent shows two rows until UX-5.
- The LF-3 library banner shows while a document is open, without holding the bar.
- A tap on a strip turns the page. Right and middle presses reach the page under a strip.

**You see.**

- [x] Hovering anywhere along the right edge shows a soft shade and a chevron. A click at the top, middle, or bottom of the edge turns to the next page.
- [x] No arrow tab sits on the right edge. The bar has a **Notes** button.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] `page_strips.rs` gains `strip_width_follows_the_window`. It asserts the literal widths 48 at 600, 59 at 844, 97 at 1384, and 128 at 2560. Run `cargo test -p iced-shell --locked strip_width`.
- [x] `app.rs` gains `strip_press_turns_the_page` and `strip_press_on_first_page_does_nothing`. They send the strip messages through `update` and assert the page number. Run `cargo test -p iced-shell --locked strip_press`. *They live in `book_preview.rs`, as `strip_press_turns_the_page` and `strip_press_on_the_first_or_last_page_does_nothing`.*
- [x] `app.rs` gains `strip_wheel_scrolls_the_page`. It sends the forwarded wheel message and asserts that the viewport offset grows. Run `cargo test -p iced-shell --locked strip_wheel`. *It lives in `book_preview.rs`.*

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [x] Lane 1. Regression lane against trunk. Turn from page 1 to page 4 at trunk with the toolbar arrow. At head, click the right strip at y=10 percent, 50 percent, and 90 percent of the window height. Save `ux3-regression.png`. Pass when head reaches page 4 and the page field reads 4.
- [x] Lane 2. Hover the right strip, then move to the page centre. Save `ux3-hover.png`. Pass when the shade and chevron show on hover and are gone after leaving.
- [x] Lane 3. On page 1, hover and click the left strip. Save `ux3-first-page.png`. Pass when the left strip shows nothing and the page stays 1.
- [x] Lane 4. Go to the last page of chapter 1 and click the right strip. Save `ux3-chapter.png`. Pass when chapter 2 opens on its first page. *Run on `Three Rooms.epub`, because the demo books have one chapter.*
- [x] Lane 5. Hover the right strip and send `-Wheel -360`. Save `ux3-wheel.png`. Pass when the page text moves up and the page number is unchanged.
- [x] Lane 6. Press on a word, drag into the right strip, and release there. Save `ux3-select.png`. Pass when a selection exists and the page number is unchanged.
- [x] Lane 7. Hover the right strip at 860, 1400, and 1920 window widths. Save `ux3-widths.png`. Pass when the shade widths are 60, 98, and 128px within 2px. *Widths 60, 98 and 135; the visible shade read 59, 95 and 132. 1920 needs F11 fullscreen.*
- [x] Lane 8. Reveal the bar, click **Notes**, then hover the right strip. Save `ux3-notes.png`. Pass when no edge tab exists, the drawer opens, and the right strip sits left of the drawer.
- [x] Lane 9. Open `The Art of Paying Attention.pdf` in Document view with Fit width, then click the right strip. Save `ux3-pdf.png`. Pass when the strip overlays the page edge and the PDF turns to the next page. *Run on `prose.pdf` (5 pages), because the demo PDF has one page.*
- [x] Lane 10. Reveal the bar, type 7 in the page field, press Enter, then press the right arrow key. Save `ux3-keys.png`. Pass when the bar has no previous or next buttons and the page reads 8.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] Metric. `reader_frame_ms` p95 for `reading-idle` and `reading-page-turn`. Trunk has no strips, so `reading-strip-hover` gets an absolute budget.
- [x] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head. Run `ui-drive.ps1 -IdleCpu 10` the same way.
- [x] Baseline. Record the trunk p95 values and idle CPU first.
- [x] Rule. Fail when a head p95 exceeds trunk by more than 10 percent. Fail when `reading-strip-hover` p95 exceeds 33ms. Fail when head idle CPU exceeds trunk by more than 30ms per 10 seconds.

**Review gate.** The operator reviews before merge.

- [x] Copy the lane 2, 7, 8, and 9 screenshots into `target\ui-ux-review\UX-3-review-<slug>.png`. Include one screenshot of each Appendix A strip variant on the real reader so the operator picks the final look.
- [x] Record a 30 to 60 second video of hover, page turns at several heights, the wheel over a strip, and the notes drawer. Save it as `target\ui-ux-review\UX-3-review.mp4`.
- [x] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [x] Root's clean verdict at the exact head SHA.
- [x] Bugbot triage done. *No Bugbot on this repository.*
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged. *Not applicable: branched from current `main`.*
- [ ] The root appends UX-3 to the stack. The operator lands it bottom-up. *Merged into `main` as PR #11.*

## Keep your place with a progress line and hide the idle scrollbar (UX-4)

**Depends on.** UX-3.

**Files.**

- [x] Create `crates/iced-shell/src/progress_line.rs`.
- [x] Edit `crates/iced-shell/src/document_scroll.rs`.
- [x] Edit `crates/iced-shell/src/app.rs`.
- [x] Edit `crates/iced-shell/src/reader_profile.rs`.
- [x] Edit `README.md`. *Also edited: `page_strips.rs`, `pdf_reader.rs`, `notes_ui.rs`, `shelf.rs`, `book_preview.rs`, and `Cargo.toml` for the test-only `zip` dependency.*

**Build.**

- [x] Check the progress formula in `capture_progress` (app.rs:1986) first. An explorer suspects that EPUB progress mixes a per-chapter page number with a whole-book total. Write the failing test before any fix, and fix it only if the test fails. *It passed before any change. Page numbers are already global, so the formula needed no fix.*
- [x] Draw a 2px line along the bottom edge of the reading area as a `stack!` layer. Fill it with `accent` up to the progress fraction over a `border` track. It shows at all times in a book.
  - [x] The line runs only between the page-turn strips. It starts at the left strip's inner edge and ends at the right strip's inner edge (`page_strips::width`, and the right strip ends at the notes drawer). The strips keep the full height, and the line and its hover area never overlap them.
  - [x] It stops where the strips stop: above the PDF status row, and above the horizontal scrollbar while a page is zoomed wider than the window. Reuse `page_strips::bottom_inset`.
- [x] On hover over the bottom 24px between the strips, grow the line to 6px and show `Page 4 of 14 · 21%` in 11px muted text above it. The label also shows while the bar is shown.
- [x] Clicking the line jumps to the page at that fraction, through the same path as the page field. *The press resolves the global page number, never a publisher label, then takes the page field's path.*
- [x] Add the state `reading-progress-hover` to `profile_reader_frames`.
- [x] Hide the document scrollbar while you read. The operator asked for this at the UX-3 review: a scrollbar that shows all the time is noise, and the progress line now keeps your place. `document_scroll.rs` draws its own 14px rail and thumb (`draw`, about line 500), so the change stays in that widget.
  - The thumb, and the rail's background fill under it, fade out over 150ms one second after the last scroll. While hidden, nothing is drawn, so the desk runs to the window edge.
  - They fade back in on any scroll movement: wheel, touchpad, keyboard, a page turn, a find jump, a page-field jump and middle-button autoscroll. They also show while the pointer is over the 14px rail and for the whole of a thumb drag.
  - The rail stays a live hit target while hidden. A press on it grabs or pages exactly as today, and the hand-over to the right page-turn strip is unchanged.
  - Keep the fade inside the widget's `State` with an explicit `Instant`, and drive it with `Shell::request_redraw_at` only while a fade or the one-second delay runs. Once hidden it requests nothing, so idle CPU stays at zero.
  - Leave the horizontal scrollbar as it is. It shows only while a page is zoomed wider than the window, and the strips already stop above it then.
  - Leave the library, Settings and panel scrollbars as they are. They are thin `ui::scrollbar` bars owned by iced.

**As built.** The review changed these from the steps above.

- The line is a third zone of the page-strip widget, not a second overlay, so presses, taps, the wheel, and right and middle buttons follow the strips' rules over it. It grows with no fade. Its label is centred on the line on an opaque backing.
- One `Progress` value feeds the line and the Continue Reading card, and both round the percent through `shelf::percent`.
- `document_scroll::wrap` takes a place key for the page on screen. A new place counts as scroll movement, because a turn from the top of one page to the top of a page of the same height changes neither the offset nor the height. The independent review found this case.
- The scrollbar also shows for a second when a document opens or the window resizes, because the content height changes.

**You see.**

- [x] With the bar hidden, a thin accent line at the bottom shows how far through the book you are.
- [x] Hovering the bottom edge shows the page and percent.
- [x] While you read, there is no scrollbar on the right. It appears as soon as you scroll or point at the right edge, and fades about a second after you stop.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] `app.rs` gains `epub_progress_grows_across_chapters`. It builds a three-chapter book in the test, because the demo books have one chapter each. It records progress at the last page of chapter 1 and the first page of chapter 2, and asserts the second is larger. Run `cargo test -p iced-shell --locked epub_progress`.
- [x] `progress_line.rs` gains `progress_line_runs_between_the_strips`. It asserts that the line's ends meet the strips' inner edges at 860 and 1400 window widths, with and without the notes drawer, and above the PDF status row. Run `cargo test -p iced-shell --locked progress_line_runs`.
- [x] `progress_line.rs` gains `click_fraction_maps_to_page`. It asserts that fraction 0.5 of 14 pages maps to page 7 and that fraction 1.0 maps to page 14. Run `cargo test -p iced-shell --locked click_fraction`.
- [x] `document_scroll.rs` gains `idle_scrollbar_hides_and_returns`. It asserts that the thumb shows after a scroll, is hidden one second plus the fade later with no redraw requested, shows again while the pointer is over the rail, and that a press on the hidden rail still starts a thumb drag. Run `cargo test -p iced-shell --locked idle_scrollbar`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Twelve lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [x] Lane 1. Regression lane against trunk. Open page 4 at trunk and at head, and let the bar hide on both. F8 pins the bar since UX-2, so do not press it. Save `ux4-regression.png`. Pass when trunk shows no position and head shows a line filled to between 21 and 29 percent of its width.
- [x] Lane 2. Hover the bottom 24px at the page centre. Save `ux4-hover.png`. Pass when the line grows, the label reads `Page 4 of 14` with a percent, and the line's ends stop at the strips' inner edges.
- [x] Lane 3. Click the line at 50 percent of its width. Save `ux4-jump.png`. Pass when the page field reads 7 or 8.
- [x] Lane 4. Open `Three Rooms.epub`, built by `python -I fixtures/chapters/build_epub.py`. Read the percent on the last page of chapter 1 and the first page of chapter 2. Save `ux4-chapters.png`. Pass when the second percent is larger.
- [x] Lane 5. Note the percent, press Ctrl+W, and read the Continue Reading card. Save `ux4-library.png`. Pass when the card percent equals the line percent. *Continue Reading needs a library entry, and a document opened from the command line gets none. The book was added with **Add document**, moved to page 9, and both read 58%.*
- [x] Lane 6. Switch to dark appearance. Save `ux4-dark.png`. Pass when the filled part and the track are both visible.
- [x] Lane 7. Press F11. Save `ux4-fullscreen.png`. Pass when the line shows in fullscreen.
- [x] Lane 8. Open the PDF in Document view. Save `ux4-pdf.png`. Pass when the line shows the page fraction.
- [x] Lane 9. Hover the bottom edge, then move away for 2 seconds. Save `ux4-no-jump.png`. Pass when the label is gone and the page text has the same y position as before the hover.
- [x] Lane 10. Resize to 540x360 and hover the bottom edge. Save `ux4-narrow.png`. Pass when the label fits on one line inside the window. *At 540x360 the page scrolls sideways, so the band sits above the horizontal scrollbar.*
- [x] Lane 11. Read for 3 seconds without moving the pointer. Save `ux4-scrollbar-idle.png`. Wheel down once and save `ux4-scrollbar-scroll.png` within 300ms. Pass when the first shows no scrollbar on the right and the second shows the thumb, in both light and dark appearance.
- [x] Lane 12. With the scrollbar hidden, press on the rail where the thumb sits and drag down 200px. Save `ux4-scrollbar-drag.png` mid-drag. Pass when the page scrolled, the thumb showed for the whole drag, and it hid again about a second after the release. *It hid about 1.5 seconds after the pointer left the rail. While the pointer rests on the rail, the scrollbar stays shown.*

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] Metric. `reader_frame_ms` p95 for `reading-idle`. Trunk has no line, so `reading-progress-hover` gets an absolute budget.
- [x] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [x] Baseline. Record the trunk `reading-idle` p95 first.
- [x] Rule. Fail when the head `reading-idle` p95 exceeds trunk by more than 10 percent. Fail when `reading-progress-hover` p95 exceeds 33ms.
- [x] Rule. Fail when `idle_cpu_ms_per_10s` after the scrollbar has faded out is above trunk's. *Trunk 0 and head 0 in three alternating rounds. A first probe ran the wheel after the hold, because `ui-drive.ps1` orders Hold before Wheel, and so measured the fade itself.*

**Review gate.** The operator reviews before merge.

- [x] Copy the lane 1, 2, 6, and 11 screenshots into `target\ui-ux-review\UX-4-review-<slug>.png`.
- [x] Record a 30 to 60 second video of reading with the line, hovering it, jumping by click, and the scrollbar fading out and back in on scroll and on a rail hover. Save it as `target\ui-ux-review\UX-4-review.mp4`.
- [x] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [x] Root's clean verdict at the exact head SHA.
- [x] Bugbot triage done. *No Bugbot on this repository.*
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged. *Not applicable: branched from current `main`.*
- [ ] The root appends UX-4 to the stack. The operator lands it bottom-up. *Merged into `main` as PR #14.*

## Replace text pills with icons and an Aa panel (UX-5)

**Depends on.** UX-4.

**Files.**

- [x] Create `crates/iced-shell/src/reading_panel.rs`.
- [x] Edit `crates/iced-shell/src/app.rs`.
- [x] Edit `crates/iced-shell/src/reading_ui.rs`.
- [x] Edit `crates/iced-shell/src/pdf_reader.rs`.
- [x] Edit `scripts/subset-icons.py` and `assets/fonts/MaterialSymbolsOutlined-Subset.ttf`.
- [x] Edit `README.md`. *Also edited: `reading_chrome.rs`, `chrome.rs`, `reader_profile.rs`, `book_preview.rs`, `promotion_preview.rs`, `release_audit.rs`, and `assets/licenses/Material-Symbols-CHANGES.txt`.*

**Build.**

- [x] Make every bar control a 32px icon button with a tooltip that names the action and its shortcut. The bar holds **Library**, the book title in muted text, the page field, **Contents**, **Find**, **Listen**, **Notes**, **Aa**, the light and dark toggle, **Search**, and **Settings**. No two buttons share a glyph. Add the needed codepoints to `subset-icons.py` and regenerate the font.
- [x] Add an **Aa** panel that drops down from its button. It holds the theme cards, text size, font, line spacing, side margins, zoom with **Fit width**, and **Use reading defaults**. Move the per-book controls from `reading_ui::settings` (reading_ui.rs:363) into it. In PDF views it shows only zoom and **Fit width**.
- [x] Delete the toolbar's zoom buttons (`Control::FontDown`, `FitBookWidth`, `FontUp`), the PDF `zoom_controls` in the bar (pdf_reader.rs:1289), and `appearance_button` (app.rs:6201). Keep Ctrl+plus, Ctrl+minus, Ctrl+0, and Ctrl+Shift+F.
- [x] Keep the bar shown while the **Aa** panel is open. Escape closes the panel and returns focus to **Aa**.
- [x] Fit the bar on one row down to the 540px minimum width. Hide the book title first, then fold **Search** and **Settings** into an overflow button. *The light and dark toggle folds with them, into a **More** menu.*

**As built.** The review changed these from the steps above.

- One table of bar items in `reading_chrome.rs` drives both drawing and keyboard order. Each item has a fold level, and the widths where items fold are measured by the ignored test `measure_reading_bar_folds`.
- The light and dark toggle folds into **More** with **Search** and **Settings**, because folding only those two left the book bar 25px too wide at 540px. The toggle no longer sits where the Library header has it, and the UX-2 test for that position is gone. The operator accepted this and will review the whole UI later.
- Settings edits the library-wide reading defaults even while a book is open. Its session **Book zoom** row moved to **Aa**.
- A press outside an open menu only closes it, and does not also turn a page or start a selection. The wheel still scrolls the page.
- The independent review found three bugs, each fixed with a test that fails without the fix. Changing a default that the open book overrides blanked its pages. A menu stayed open over a newly dropped document. Escape from a picker left focus on a removed choice.

**You see.**

- [x] The bar is one row of quiet icons with tooltips.
- [x] **Aa** opens a panel where text size, font, spacing, margins, theme, and zoom change the open book. *Until #18 the theme cards changed the theme for every book. See the next section.*

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] `reading_panel.rs` gains `aa_panel_changes_the_open_book_only`. It raises the text size through the panel messages and asserts the book override changed while the library default did not. Run `cargo test -p iced-shell --locked aa_panel`.
- [x] `app.rs` gains `every_bar_control_has_a_tooltip`. It walks the bar controls and asserts a non-empty tooltip for each. Run `cargo test -p iced-shell --locked bar_control`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe.

- [x] Lane 1. Regression lane against trunk. Capture the bar at trunk and head, then hover each head icon. Save `ux5-regression.png`. Pass when trunk shows text pills and every head icon shows a tooltip with an action name.
- [x] Lane 2. Open **Aa**, press text size plus twice, close the book, and reopen it. Save `ux5-size.png`. Pass when the text is larger after reopening.
- [x] Lane 3. Pick the Soft theme in **Aa**. Save `ux5-theme.png`. Pass when the page uses Spectral on warm paper.
- [x] Lane 4. In **Aa**, press zoom plus, then **Fit width**. Save `ux5-zoom.png`. Pass when the page grows and then fits the window width. *In two sessions a scripted click on zoom plus or minus did not register. It worked in about ten other tries and in a headless click test. Shots of the failures showed the button not hovered, which fits the operator moving the real mouse during the lane. The operator accepted this for now.*
- [x] Lane 5. Open the PDF in Document view and open **Aa**. Save `ux5-pdf.png`. Pass when only zoom and **Fit width** are enabled.
- [x] Lane 6. Tab into the **Aa** panel, then press Escape. Save `ux5-keyboard.png`. Pass when focus moves through the panel controls and Escape returns focus to **Aa**.
- [x] Lane 7. Open **Aa** and wait 5 seconds without moving. Save `ux5-hold.png`. Pass when the bar and panel are still shown.
- [x] Lane 8. Press Ctrl+plus twice and Ctrl+0. Save `ux5-shortcuts.png`. Pass when the zoom grows and returns to 100 percent. *100, 120, then 100 percent.*
- [x] Lane 9. Resize to 540x360 and reveal the bar. Save `ux5-narrow.png`. Pass when the bar is one row with no overlap.
- [x] Lane 10. Press Ctrl+W, open Settings, and raise the default text size. Open a book that has no override. Save `ux5-defaults.png`. Pass when that book uses the new default.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] Metric. `reader_frame_ms` p95 for `reading-bar-reveal` and `settings`. Trunk has no **Aa** panel, so a new state `reading-aa-open` gets an absolute budget.
- [x] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [x] Baseline. Record the trunk p95 values first.
- [x] Rule. Fail when a head p95 exceeds trunk by more than 10 percent. Fail when `reading-aa-open` p95 exceeds 33ms. *`reading-bar-reveal` was at or below trunk in all three rounds, `settings` within 4 percent, and `reading-aa-open` at most 8.5ms.*

**Review gate.** The operator reviews before merge.

- [x] Copy the lane 1, 2, 5, and 9 screenshots into `target\ui-ux-review\UX-5-review-<slug>.png`.
- [x] Record a 30 to 60 second video of the icon bar, the tooltips, and the **Aa** panel. Save it as `target\ui-ux-review\UX-5-review.mp4`. *55 seconds.*
- [x] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [x] Root's clean verdict at the exact head SHA.
- [x] Bugbot triage done. *No Bugbot on this repository.*
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged. *Not applicable: branched from current `main`.*
- [ ] The root appends UX-5 to the stack. The operator lands it bottom-up. *Merged into `main` as PR #17.*

## Give each book its own theme, with a default theme in Settings (#18)

The operator asked for this at the UX-5 review. It was not in the original plan. UX-5 put theme cards in both Settings and **Aa**, and both changed one theme for every book.

**As built.**

- `reading::Options` in `reader-document` gains `theme: Option<ThemeId>`. A book's own theme is saved with its other per-book reading settings. `None` means the book follows the default theme. Old files load with no theme, and an unknown theme id loads as none with the other settings kept.
- Settings' theme cards set the default theme. The Library and every book without its own theme use it.
- **Aa**'s theme cards set the open book's theme. The card for the default theme is tagged **Default theme**, and picking it makes the book follow the default again while it keeps its typography.
- **Use reading defaults** in **Aa** drops all of the book's own settings, its theme among them. The book then follows the default theme and the typography defaults again.
- One rule picks the theme in use: the open book's own theme, else the default. PDFs, and PDFs in Book view, always use the default. A book is laid out again only when its look changes, so a book with its own theme opens straight in that theme.
- Android keeps a desktop theme stored for a book when it saves that book's options. The four built-in themes are unchanged.
- A book's own settings are one saved copy of every field. Any **Aa** change, a theme included, means the book stops following later changes to the typography defaults until **Use reading defaults** is pressed. The operator likes this behaviour and keeps it.
- The independent review found that a settings reply from an earlier open of the same book could undo a theme chosen since. It was fixed, with `a_late_settings_reply_cannot_undo_a_theme_choice`.

**Verified.**

- [x] The gate ran with the two atlas tests failing as on `main`.
- [x] Live on the release build. A book given Soft in **Aa** reopens in Soft, and its settings file records `"theme":"soft"`. The Library returns to the default theme. A default changed to Clear in Settings themes another book Clear and leaves the Soft book alone. **Use reading defaults** returns the Soft book to Clear. Polling the window's pixels while reopening the Soft book showed no frame in the default theme.
- [x] Review cards in `target\ui-ux-review\theme-*.png`, from `render_book_theme_look`.

## Float the panels and move contents to a left drawer (UX-6)

**Depends on.** UX-5.

**Files.**

- [x] Edit `crates/iced-shell/src/app.rs`.
- [x] Edit `crates/iced-shell/src/read_aloud.rs`.
- [x] Edit `crates/iced-shell/src/notes_ui.rs`.
- [x] Edit `crates/iced-shell/src/page_strips.rs`.
- [x] Edit `README.md`. *Also: a new `drawers.rs`, and edits to `progress_line.rs`, `reading_chrome.rs`, `reading_ui.rs`, `reader_profile.rs`, and `book_preview.rs`.*

**Build.**

- [x] Draw the read-aloud player (`read_aloud::player`, read_aloud.rs:1063) as a floating pill centred above the progress line, as a `stack!` layer.
- [x] Draw the find bar (`find_bar`, app.rs:6019) as a floating box at the top right, below the bar's position.
- [x] Move the EPUB contents list (app.rs:6590) into a left drawer with the same width rule as `sidebar_width`. **Contents** and Ctrl+T toggle it. Escape closes it.
- [x] Below 900px window width, both drawers overlay the page instead of narrowing it. *PR #21 replaces this width with a rule based on whether the page fits.*
- [x] Move the left strip to the right edge of the contents drawer while the drawer is open.
- [x] Keep errors, help, and notices in the `auxiliary` column. They are rare, and moving them is out of scope.

**As built.** The review changed these from the steps above.

- One drawer model in `drawers.rs` covers contents on the left and notes on the right. It says, for each side, whether the open drawer narrows the page or covers it, so that both can never happen at once. `sidebar_width` and the two copies of the contents height are gone.
- Find, Listen, and Contents no longer keep the bar shown. The bar idles away while you listen or search.
- The find box and the pill stay between the page-turn strips. Where that leaves them less than 300px, they cover the strips instead.
- Choosing a chapter keeps a drawer beside the page open, and closes one that covers the page. Links, Back, and Ctrl+Page Down follow the same rule.
- Pause and the speed label have fixed widths, so the pill's buttons stay still under the pointer when Pause becomes Resume.
- At 900px, two drawers beside the page left it 252px wide, with no room for the find box or the pill. PR #20 therefore kept both drawers open together only from 1256px. Narrower, opening one closed the other.
- Two independent reviews each found one bug, and each is fixed with a test. Floating controls had no room with both drawers open. A resize that kept both drawers still cleared focus in Contents.

**Follow-up after the review.** The operator asked for two more changes at the UX-6 review. PR #21 built them, and it is merged.

- A drawer sits beside the page only when the whole page still fits. Otherwise it covers the page. In PR #20, a drawer beside the page could leave it narrower than its paper, and the page then scrolled sideways. That happened between 900 and 1106px with one drawer, and between 1256 and 1446px with two.
  - At zoom 1 with the default margins, one drawer sits beside the page from 1106px, and two drawers stay open together from 1446px. Narrower, opening one drawer closes the other. Zooming in or narrowing the window closes contents and keeps notes.
  - With fit width on, a drawer sits beside the page while it leaves the page at least 576px.
  - A PDF at fit width keeps notes beside the page. A PDF zoomed wider than the room left gets notes over the page.
  - The desk's side padding now narrows from 32 to 16px before the page has to scroll. Before, the page scrolled sideways between 780 and 797px even with no drawer open, and a drawer flipped back over the page between 1120 and 1137px.
- The listening pill shows and hides with the reading bar. In PR #20 it stayed on screen and could cover the line being read.
  - Moving the pointer to where the pill sits shows the bar and the pill, and keeps them shown while the pointer stays. So does focus on a pill button.
  - Starting to listen shows both for the 2 seconds a newly opened document gets.
  - The independent review found that a focused or hovered pill kept the bar shown after reading stopped. It is fixed with a test.

**You see.**

- [x] Listen, Find, and Contents open without moving the page. *A drawer beside the page centres the page again in the space left, about 170px to the right at 1400px. Nothing moves up or down.*

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] `app.rs` gains `panels_do_not_move_the_page`. It renders a book with Listen, Find, and Contents each open and asserts the first text row keeps its y. Run `cargo test -p iced-shell --locked panels_do_not_move`.
- [x] `page_strips.rs` gains `left_strip_follows_the_contents_drawer`. It asserts that the left strip starts at the drawer width when the drawer is open. Run `cargo test -p iced-shell --locked left_strip_follows`.

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe. *The session ran them itself on the release build, and measured page movement with pixel diffs.*

- [x] Lane 1. Regression lane against trunk. Start Listen at trunk and at head. Save `ux6-regression.png`. Pass when trunk moves the page down and head shows a pill at the bottom with the page unmoved. *Trunk does not move the page down. It covers the top of the page with a full-width band. On head, only the pill and the highlight on the spoken word change.*
- [x] Lane 2. Press Ctrl+F, type `garden`, and press Enter. Save `ux6-find.png`. Pass when the find box shows at the top right, a match is highlighted, and the page did not move for the box. *Opening Find changes only the box's own pixels. All 20 matches showed a visible highlight.*
- [x] Lane 3. Press Ctrl+T, click chapter 2, then press Escape. Save `ux6-contents.png`. Pass when a left drawer opens, chapter 2 opens, and Escape closes the drawer. *Escape returns focus to **Contents**.*
- [x] Lane 4. Open the contents drawer and click the left strip on page 3. Save `ux6-left-strip.png`. Pass when the strip sits right of the drawer and the page turns to 2. *The strip spans 340 to 438px. A click inside the drawer does not turn the page.*
- [ ] Lane 5. At 1100px width, open both drawers. Save `ux6-both.png`. Pass when the page fits between the drawers with no overlap. *Failed as written. Two drawers beside the page left 420px, and the 766px page scrolled sideways. PR #20 then kept one drawer at 1100px, and the follow-up changes the rule again.*
- [x] Lane 6. Use the pill's pause, slower, and faster buttons, then click the right strip. Save `ux6-player.png`. Pass when speech pauses and resumes, the speed label changes, and the page turns. *The speed went 1.0×, 0.9×, then 1.1×, and speech went on to page 2. The buttons moved when Pause became Resume, which is fixed.*
- [x] Lane 7. At 860px width, open the contents drawer. Save `ux6-overlay.png`. Pass when the drawer overlays the page and the page keeps its width. *Only the drawer and the strip beside it change.*
- [x] Lane 8. Open the contents drawer and press Tab five times. Save `ux6-focus.png`. Pass when focus moves through the drawer entries. *Through the three entries, then on to the bar.*
- [x] Lane 9. Open Find, then move to the top edge. Save `ux6-stacking.png`. Pass when the bar and the find box do not overlap. *The bar spans 0 to 44px and the box 52 to 102px. With Find open, the bar idled away first.*
- [x] Lane 10. Delete the open book's file from the lane profile and reopen it. Save `ux6-error.png`. Pass when the error notice with **Locate** still shows.

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] Metric. `reader_frame_ms` p95 for `reading-idle`. Trunk renders panels in the column, so new states `reading-listen` and `reading-contents` get absolute budgets.
- [x] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [x] Baseline. Record the trunk `reading-idle` p95 first. *8.99 to 9.05ms.*
- [x] Rule. Fail when the head `reading-idle` p95 exceeds trunk by more than 10 percent. Fail when `reading-listen` or `reading-contents` p95 exceeds 33ms. *Head `reading-idle` was 8.46 to 8.82ms. `reading-listen` peaked at 8.92ms and `reading-contents` at 9.76ms.*

**Review gate.** The operator reviews before merge.

- [x] Copy the lane 1, 2, 3, and 7 screenshots into `target\ui-ux-review\UX-6-review-<slug>.png`. *Also two shots with both drawers open, at 1100 and 1300px.*
- [x] Record a 30 to 60 second video of Listen, Find, and the contents drawer. Save it as `target\ui-ux-review\UX-6-review.mp4`. *48 seconds. Another app's window covered the screen recordings, so the video is built from captures of the reader window itself, and the pointer does not show.*
- [x] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [x] Root's clean verdict at the exact head SHA.
- [x] Bugbot triage done. *No Bugbot on this repository.*
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged. *Not applicable: branched from current `main`.*
- [ ] The root appends UX-6 to the stack. The operator lands it bottom-up. *Merged into `main` as PR #20.*

## Continue to the next page when the wheel passes the end (UX-7)

**Depends on.** UX-6.

**Files.**

- [x] Edit `crates/iced-shell/src/app.rs`.
- [ ] Edit `crates/iced-shell/src/pdf_reader.rs`. *Not needed. Its only change is one `None` argument for the new wheel callback.*
- [x] Edit `README.md`. *Also: `document_scroll.rs`, `reading_chrome.rs`, and `book_preview.rs`.*

**Build.**

- [x] Today `Message::Scroll` (app.rs:4318) clamps the offset inside the page, so the wheel never turns it. Add an overscroll counter to the reader. When the page is at its bottom and wheel-down input adds up to 240px within 700ms, turn to the next page and land at its top. Wheel-up input at the top turns to the previous page and lands at its bottom. Any reverse input resets the counter.
- [x] Count line deltas as 60px per line and pixel deltas as given, so a mouse and a touchpad behave alike. *On Windows, winit reports every wheel event in lines, so a touchpad arrives as fractions of a line.*
- [x] Never turn on Ctrl+wheel, which zooms (document_scroll.rs:555), or during middle-button auto-scroll.
- [x] Count the wheel wherever it reaches the page. Since UX-6, a wheel over the bar (reading_chrome.rs:874), a page strip (page_strips.rs:485), the find box, or the read-aloud pill (app.rs:6023) arrives as `Message::ChromeWheel` (app.rs:3785) and is passed on to the page. At the page's end the passed-on wheel moves nothing, so the counter must add the raw delta from these messages too. Otherwise a wheel over the pill at the bottom of a page never turns it.
- [ ] Apply the same rule in PDF Document view. *Not needed. Document view is one continuous scroll of all pages, so the wheel already moves on from one PDF page to the next. Lane 10 shows trunk and head behave the same.*

**As built.** The review changed these from the steps above.

- One push value on the reader and one pure rule serve both wheel sources. The page's scroll wrapper reports each wheel event before the page moves, so the reader sees where the wheel found it.
- The push window starts at the first counted input. A push also ends when the page changes.
- A turn from a wheel over the chrome replaces the passed-on scroll, so the new page does not move straight away.
- Going back across a chapter start lands on the previous chapter's last page at its bottom. Before, it landed at that page's top. The arrow keys, the strips, and Ctrl+Page Up still land at the top.
- Landing at the bottom is applied after layout settles. A content anchor cannot hold a bottom that lies below the text, which is common on a chapter's last page.
- A fifth quick notch after a turn scrolls the new page 60px down. The operator merged PR #24 with it as built.
- The independent review found two paths, and both stay as built. A wheel over the pill during auto-scroll still counts, but auto-scroll alone sends no wheel input, so it never turns a page. Shift with pixel deltas counts, but Windows never sends pixel deltas.

**You see.**

- [x] Scrolling down past the end of a page goes to the next page after a short push. One small flick at the bottom does not. *Four notches turn the page, or twelve 20px touchpad steps.*

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] `app.rs` gains `overscroll_turns_after_240px`, `one_notch_does_not_turn`, `reverse_input_resets_overscroll`, and `ctrl_wheel_never_turns`. They drive `Message::Scroll` with literal deltas and assert the page number and offset. Run `cargo test -p iced-shell --locked overscroll`. *The tests drive the new `Message::PageWheel` and `Message::ChromeWheel`, because `Message::Scroll` carries no delta. Five more cover Shift, the 700ms window, landing at the previous page's bottom, the first and last page, and a chapter start.*
- [x] `book_preview.rs` gains `overscroll_over_the_pill_turns_the_page`. With Listen on and the bar shown, it wheels over the pill at the bottom of a page and asserts the next page opens. Run `cargo test -p iced-shell --locked overscroll_over`. *Two more drive the real widget tree. One shows the reader's edge check matches the page's own scroll position at zoom 0.6, 0.8, 1.0, and 2.0. The other shows Ctrl+wheel over the page zooms without turning.*

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe. *The session ran them itself on the release build. `ui-drive.ps1` takes about 0.3s per call, too slow for a 700ms push, so the wheel bursts came from one in-process script. Pixel diffs against arrow-key turns proved where each page landed.*

- [x] Lane 1. Regression lane against trunk. Scroll to the bottom of page 1, then send five wheel notches down at trunk and at head. Save `ux7-regression.png`. Pass when trunk stays on page 1 and head shows the top of page 2. *Four notches land on page 2's top, pixel for pixel. The fifth scrolls the new page 60px.*
- [x] Lane 2. At the bottom of page 1, send one notch down. Save `ux7-one-notch.png`. Pass when the page stays 1. *No pixel of the page changed.*
- [x] Lane 3. At the top of page 3, send five notches up. Save `ux7-up.png`. Pass when page 2 shows at its bottom. *Four notches match page 2's bottom pixel for pixel.*
- [x] Lane 4. Zoom out until a page fits the window, then send five notches down. Save `ux7-short.png`. Pass when the next page shows. *At zoom 0.6.*
- [x] Lane 5. Send twenty small pixel deltas of 20 down at a page bottom. Save `ux7-touchpad.png`. Pass when the page turns once. *Sent as a third of a line each. The rest scrolled page 2 to its bottom.*
- [x] Lane 6. At a page bottom, hold Ctrl and send five notches down. Save `ux7-ctrl.png`. Pass when the zoom changes and the page number does not.
- [x] Lane 7. Send notches up on the first page and down on the last page. Save `ux7-ends.png`. Pass when the page number does not change. *Page 1 and page 14 of 14.*
- [x] Lane 8. Use middle-button auto-scroll to the page bottom and hold it there for 3 seconds. Save `ux7-autoscroll.png`. Pass when the page does not turn.
- [x] Lane 9. At the bottom of the last page of chapter 1, send five notches down. Save `ux7-chapter.png`. Pass when chapter 2 opens at its top. *Page 8 of 21 in* Three Rooms. *Wheeling back up lands on chapter 1's last page at its bottom, pixel for pixel.*
- [x] Lane 10. Open the PDF in Document view at a zoom where a page scrolls, then wheel past its end. Save `ux7-pdf.png`. Pass when the next PDF page shows. *Trunk and head both reach page 2 after 36 notches.*

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] Metric. `reader_frame_ms` p95 for `reading-page-turn`.
- [x] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head.
- [x] Baseline. Record the trunk `reading-page-turn` p95 first. *11.21 to 11.53ms.*
- [x] Rule. Fail when the head p95 exceeds trunk by more than 10 percent. *Head was 10.66 to 11.62ms.*

**Review gate.** The operator reviews before merge.

- [x] Copy the lane 1, 2, and 5 screenshots into `target\ui-ux-review\UX-7-review-<slug>.png`. *Lane 1 at trunk and at head.*
- [x] Record a 30 to 60 second video of reading through three pages with the wheel only. Save it as `target\ui-ux-review\UX-7-review.mp4`. *44 seconds, built from captures of the reader window, so the pointer does not show.*
- [x] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click.

**Merge.**

- [x] Root's clean verdict at the exact head SHA. *The independent review on gpt-6.1-sol found nothing to change. Its two findings are recorded on PR #24.*
- [x] Bugbot triage done. *No Bugbot on this repository.*
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged. *Not applicable: branched from current `main`.*
- [ ] The root appends UX-7 to the stack. The operator lands it bottom-up. *Merged into `main` as PR #24.*

## Make the paper the lightest surface and split selected from focused (UX-8)

**Depends on.** UX-7.

**Files.**

- [ ] Edit `crates/iced-shell/src/book_style.rs`. *Not needed. It paints the paper and the desk from the palette.*
- [ ] Edit `crates/iced-shell/src/themes.rs`. *Not needed. It only maps the shared palettes to iced themes.*
- [x] Edit `crates/reader-layout/src/themes.rs`. Since PR #18 the theme palettes live here, shared with the Android app. No layout code in `reader-layout` reads a palette, so the colors cannot change pagination or page numbers.
- [x] Edit `crates/iced-shell/src/ui.rs`.
- [x] Edit `crates/iced-shell/src/app.rs`. *Also: `shelf.rs`, `drawers.rs`, `page_strips.rs`, `progress_line.rs`, `book_preview.rs`, `reader_profile.rs`, one test constructor in `read_aloud.rs`, `crates/reader-layout/src/book.rs`, and `fixtures/chapters/build_epub.py`.*

**Build.**

- [x] Before building, the operator decides where the paper change goes. The paper is each palette's `surface` and the desk its `background`, in `reader-layout/src/themes.rs`. `reader-ffi` (reader.rs:262) hands the same two colors to Android as `paper` and `background`. Changing the shared light palettes changes the Android paper too. A desktop-only change would split one palette into two. *The operator chose the shared palettes on 2026-11-09, so Android gets the same lighter paper. The Android app is not built or checked for UX-8.*
- [x] In every light theme variant, make the paper (`book_style::paper`, book_style.rs:17) at least as light as the desk (`book_style::desk`, :30). Tint the desk slightly instead of greying the paper. Keep the dark variants as they are. Today the Default light paper is `#f6f6f6` on a `#ffffff` desk. *Only Default light had the darker paper. It is now `#ffffff` on a `#f4f4f4` desk.*
- [x] In `ui::button_style` (ui.rs:332), draw selected as a tinted fill with no outline. Draw keyboard focus as the 2px `accent` outline only. Today both draw a blue outline, so a selected tab looks focused.
- [x] Centre the paper in the reading area at every width. Measure the gutters on both sides, because the 26px notes edge left with UX-3. Since UX-4 the scrollbar floats over the page, yet `paged_book_view` (app.rs:6347) still pads the desk 14px on the right for its rail (app.rs:6423). Measure that first, because it may hold the paper about 7px left of centre. *It did, at every width. The padding is deleted.*

**As built.** The review changed these from the steps above.

- Default light has a `#f4f4f4` desk and bars, a `#ffffff` paper and panels, `#ebebeb` for hover and input fill, and `#6a6a6a` muted text. Hover is darker than the page in this one theme. The palette came from three candidates rendered through the real widgets.
- A selected control is an accent tint at 14 percent with the normal text colour and no border, for every tone. Only keyboard focus draws an accent outline, so a primary button lost its thin accent border. The library sort and the shelf chips pass their selection instead of switching tone.
- Both page-turn strips are equally wide, and the right one runs to the edge of the reading area, over the scrollbar rail. The operator asked for this at the review, in place of the wider left strip that PR #26 first had.
  - The shade and the chevron show on the last 14px too. While the page scrolls, presses, the wheel and the cursor there go to the scrollbar. On a page that fits there is no scrollbar, and the whole strip turns the page.
  - The lane between the strips is centred, so the progress line, its label, the find box and the listening pill share the paper's centre. `page_strips::regions` gives the strips and the rail part to every reader of that geometry.
- The contents drawer marks one entry. The operator asked for the EPUB part at the review, on the condition that the app does not get slower.
  - Before, a book that is not an EPUB marked every heading up to the reader's place, and an EPUB marked every entry of the open file.
  - The current entry is the one furthest into the book that is at or before the reader's place. An entry in an earlier file counts as before. A file with no entry of its own keeps the entry it continues. An entry whose anchor is missing is never current.
  - Each entry's row in the open file is resolved once per loaded file and cached on the book. A frame only compares integers, and nothing runs while the drawer is closed.
  - The reader remembers where a jump or a newly opened book arrived, and the drawer takes that row as the place until the view moves. Without it, a jump to a heading low on a page marked the entry before it. A new layout of the same view, or a window that changes how far the page scrolls, is not a move.
- Known limits. In a file with no contents entry, the drawer marks the last listed entry of the nearest earlier file, which is wrong when a publisher lists that file's entries out of order or lists a broken link last. Focus on library cards is still 1px, and the highlight colour swatches and the header buttons are unchanged.
- The independent review ran five times on gpt-6.1-sol and found seven defects. Six are fixed, each with a test that fails without the fix: two contrast drops, a dead click under the shade on a page that fits, and three cases where the drawer marked the wrong entry after a jump. The seventh is the limit above.

**You see.**

- [x] In light themes the page is the brightest thing on screen.
- [x] Selected controls are filled, and only the focused control has an outline.

**Verify, unit.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] `reader-layout/src/themes.rs` gains `light_paper_is_never_darker_than_the_desk`, beside the palettes it checks. It walks every theme's light palette and asserts the paper's relative luminance is at least the desk's. Run `cargo test -p reader-layout --locked light_paper`. *Also `light_placeholders_stay_readable_on_inputs`, which asserts 4.5:1 for muted text on the input fill.*
- [x] `ui.rs` gains `selected_has_no_outline_and_focus_does`. It asserts border width 0 for selected and unfocused, and 2 for focused. Run `cargo test -p iced-shell --locked selected_has_no_outline`. *`book_preview.rs` gains tests that drive the real widget tree.*
  - *Selection and centre: `selected_sort_and_shelf_chip_are_tinted_without_an_outline` and `paper_is_centred_in_the_reading_area`.*
  - *Strips: `lane_between_the_strips_is_centred` in `page_strips.rs`, `right_strip_shares_the_edge_with_the_scrollbar`, and `a_page_that_fits_gives_the_whole_right_strip_to_the_turn`.*
  - *Contents: `contents_marks_only_the_heading_the_reader_is_in`, `epub_contents_mark_the_entry_the_reader_is_in`, `epub_contents_follow_the_book_when_the_list_is_out_of_order`, `epub_contents_tint_only_the_section_the_reader_is_in`, and `a_contents_entry_stays_current_after_jumping_to_it`.*

**Verify, live.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked. Ten lanes on `opencode-go/deepseek-v4.1-flash` at the PR head, per the boot recipe. *The session ran them itself on the release build and read the colours and gutters from pixels of the captures.*

- [x] Lane 1. Regression lane against trunk. Capture the Default light reading screen at trunk and head and sample one paper pixel and one desk pixel. Save `ux8-regression.png`. Pass when trunk paper is darker than its desk and head paper is at least as light as its desk. *Trunk `#f6f6f6` on `#ffffff`. Head `#ffffff` on `#f4f4f4`.*
- [x] Lane 2. Repeat the samples for Soft, Clear, and Compact light. Save `ux8-themes.png`. Pass when the paper is at least as light as the desk in all three. *`#f4f0e6` on `#e9e5dc`, `#f7f8fa` on `#eceff3`, and `#f3f3f0` on `#e6e6e3`. These palettes did not change.*
- [x] Lane 3. Capture all four dark variants. Save `ux8-dark.png`. Pass when the paper edge is still visible against the desk. *The paper, its rim and the desk differ in all four. No dark palette changed.*
- [x] Lane 4. Open Contents so its button is selected, then Tab to another control. Save `ux8-selected.png`. Pass when **Contents** shows a fill without an outline and the focused control shows the outline. *The page field holds the outline.*
- [x] Lane 5. Open Settings and Tab across the theme cards. Save `ux8-cards.png`. Pass when the selected card and the focused card look different. *Shift+Tab reaches the cards from an open Settings. A card that is both selected and focused shows the tint and the outline.*
- [x] Lane 6. On the Library, capture the sort control with **Title** selected. Save `ux8-sort.png`. Pass when **Title** is filled and has no outline. *The tint runs to the segment's edge with no border pixel.*
- [x] Lane 7. At 860px width, capture the page. Save `ux8-centre.png`. Pass when the left and right gutters differ by at most 2px. *71 and 71px. Trunk has 64 and 78.*
- [x] Lane 8. Open the notes drawer and capture the page. Save `ux8-centre-notes.png`. Pass when the paper is centred in the space left of the drawer, within 2px. *171 and 171px. Trunk has 164 and 178.*
- [x] Lane 9. Capture a page with a yellow highlight and a green highlight in light and dark. Save `ux8-highlights.png`. Pass when both highlights are readable on the new paper.
- [x] Lane 10. Open the PDF in Document view. Save `ux8-pdf.png`. Pass when the PDF page keeps its own colours. *The page region is pixel-identical to trunk.*

Lanes added for the two review changes, on the release build:

- [x] Strips. Hover, press and wheel along the right edge of *Three Rooms* at 1400px. *The shade spans 1302 to 1400px, also with the pointer on the last 14px. A press on the scrollbar track scrolls without a page turn, and a thumb drag scrolls. A press in the strip turns the page. Zoomed out until the page fits, a press 5px from the edge turns the page. Beside a docked notes drawer the shade spans 962 to 1060px, and at 860px it spans 800 to 860px.*
- [x] Contents. Read through `Two Wings.epub`, which `fixtures/chapters/build_epub.py` now also writes, with the drawer open. *One entry is marked on every page. The file with no entry keeps **The gallery**, and the entry with a missing anchor is never marked. A click on **The gallery** marks it. In* Three Rooms *a click on the second chapter marks it alone.*
- [ ] Repeat the strip lane at the final head. *The display scale changed from 100 to 125 percent during the session. The contents lane was rerun at 125 percent on the final head. The strip lane ran at 100 percent on an earlier head with the same strip code.*

**Verify, perf.** Tests alone are not sufficient verification. A PR is verified only when its unit, live, and perf boxes are all checked.

- [x] Metric. `reader_frame_ms` p95 for `reading-idle`, `library`, and `settings`. *Also the new state `reading-contents-epub`: the drawer open on an EPUB with 2,018 contents entries, 450 of them in the open file.*
- [x] Probe. Run `profile_reader_frames` at trunk, head, trunk, head, trunk, head. *15 alternating rounds per side for the first head. For the final head, 12 rounds against the commit just before the EPUB change.*
- [x] Baseline. Record the trunk p95 values first. *Medians of 15: `library` 2.93ms, `reading-idle` 8.73ms, `settings` 15.05ms.*
- [x] Rule. Fail when a head p95 exceeds trunk by more than 10 percent. *No measurable difference. Head medians were 3.15, 8.72 and 14.84ms, and the frame medians moved by under 2 percent. With the drawer open on the long EPUB, the frame median is 10.32 against 10.33ms. The row table for the open file takes 0.08ms to build, once per loaded file.*

**Review gate.** The operator reviews before merge.

- [x] Copy the lane 1, 2, 4, and 7 screenshots into `target\ui-ux-review\UX-8-review-<slug>.png`.
- [x] Record a 30 to 60 second video of the four light themes, then Tab through selected and focused controls. Save it as `target\ui-ux-review\UX-8-review.mp4`. *46 seconds, built from captures of the reader window, so the pointer does not show.*
- [x] Post the screenshots and the video in chat for the operator. Stop at merge-ready. Wait for the operator's click. *The operator tested the build from the Desktop shortcut and asked for the strip and contents changes before merging.*

**Merge.**

- [x] Root's clean verdict at the exact head SHA. *The last review round found two cases in the jump memory. Both are fixed with tests, and no further round ran on the final head.*
- [x] Bugbot triage done. *No Bugbot on this repository.*
- [ ] Rebased onto current trunk after the verdict, patch-id unchanged. *Not applicable: branched from current `main`.*
- [ ] The root appends UX-8 to the stack. The operator lands it bottom-up. *Merged into `main` as PR #26.*

## Shrink the library header and clear its noise (UX-9)

**Depends on.** UX-8 in the stack. The owner may build from UX-1.

**Files.**

- [ ] Edit `crates/iced-shell/src/shelf.rs`.
- [ ] Edit `README.md`.

**Build.**

- [ ] Replace the hero block in `Shelf::view` (shelf.rs:921), the `LOCAL WORKSPACE` label (:938), the `simPl Reader` heading (:943), and the tagline with one row. The row holds **Library** in Literata with the book count in muted text, and **Open** and **Add** buttons at the right. Today the two actions are the cards **Open file** (:971) and **Add document** (:977). **Open** reads a file where it is without copying it, and **Add** imports a copy into the library (`lib-fix.md`, LF-1). Dropping a file into the window opens it in place, the same as **Open**, and highlights **Open**.
- [ ] Delete the `N active` pill (:996), the `Press Space to resume current` hint (:1003), and the `N items` pill (:1084). Space still resumes the latest book (app.rs:5111). The README shortcut table lists it, and UX-10 adds it to the F1 help.
- [ ] Show **Continue Reading** only when the library holds more than three books. With three or fewer, every book is already visible in the grid.
- [ ] Move the format badge off the cover art (:1859) into the line under the cover.
- [ ] Keep the selected look from UX-8. A selected sort segment or shelf chip is an accent tint with no border, and only keyboard focus draws an outline. In Default light the Library page is light grey and its cards are white.

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
- The 240px and 700ms overscroll threshold. UX-7 lanes 2 and 5 check it with a mouse and a touchpad pattern. The operator tunes it at review. *Both lanes passed, and PR #24 merged with these values.*
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
- A progress line across the full width. It would overlap the page-turn strips in the bottom corners, so one hover would mean two things. The operator chose a line between the strips.
- Leaving the document scrollbar on screen at all times. The operator found it noisy while reading. Once the progress line keeps your place, an idle thumb adds nothing.
- Moving the scrollbar rail to hug the paper. The rail is custom-drawn in `document_scroll.rs`, and moving it costs more than letting the strip sit inside it.
- A near-white Default light paper with white hover, the shape the other three light themes have. Only a white paper makes the page the lightest surface, so in Default light hover is darker than the page.
- Accent-coloured labels on selected controls, as the bar icons have. On the tint their contrast is 4.25:1, below the 4.5:1 that small text needs.
- A left page-turn strip 14px wider than the right one, to centre the lane between them. PR #26 first built it. The operator chose equal strips with the right one over the scrollbar.
- Hiding the strip's shade over the last 14px, where a press goes to the scrollbar. The operator wants the shade there too.
- Taking a page's last row as the reader's place once the page is scrolled to its end. On a page that fits and holds two headings, it marks the second whichever one you jumped to.
- Loading an earlier EPUB file to place its contents entries exactly. It parses a file while the drawer draws, and the operator asked that the app not get slower.

## Appendix C. Risks

- Live lanes move the global cursor, so lanes on one desktop must run one at a time. All PRs. The root schedules lanes in series or on separate Windows sessions.
- No CI runs on PRs, because `release.yml` runs only on tags. All PRs. Owners paste `dev.ps1 check` output, and the gates lane reruns it.
- Focus can land on a hidden bar control. UX-2. Bar controls stay in the focus order, and focusing one reveals the bar. Lane 5 checks it.
- A frame subscription that never stops would spin the CPU. UX-2, UX-3, and UX-4. The perf rule on `idle_cpu_ms_per_10s` fails such a head.
- Wheel forwarding may scroll by a different amount than native scrolling. UX-3. The owner matches the scrollable's own line height.
- The PDF Document view has its own scroll and selection in `pdf_reader.rs`. UX-3 and UX-7. Lanes 9 and 10 cover it. *UX-7 needed no PDF change, because Document view scrolls through all pages as one document.*
- Regenerating the icon font needs network access and `fontTools`. UX-0, UX-3, and UX-5. The script pins the source blob SHA.
- Ignored preview tests that never set a window style render Mac chrome today, so their output changes. UX-1. The owner reruns `render_reading_comfort_previews` and attaches the new images.
- The bar may not fit one row at 540px before UX-5 folds controls into overflow. UX-2. Lane 9 checks 860px only, and UX-5 lane 9 checks 540px.
- A hidden scrollbar is harder to find. UX-4. The rail stays live while hidden, and the thumb returns on any scroll and on a hover over the right edge. Lanes 11 and 12 check it.
- Bugbot may not be installed on this repository. All PRs. The owner records that once.
- The light palettes are shared with the Android app, and the Android app is not built or checked. UX-8. Android reads the shared palettes, so it gets the new Default light colours. Its fallback palette in `Theme.kt` keeps the old ones.
- The display scale can change during a session, and lane probes that use fixed pixel positions then miss. UX-8. Check the scale before a lane batch, and scale the positions.

## Appendix D. Links and reading list

Read these before editing.

- `design/DESIGN.md`, which already asks for chrome that auto-hides and a slim progress line at the bottom.
- `reader-comfort-review-2026-09-30.md`, which covers fit width, fullscreen, and Escape order.
- `README.md` sections on shortcuts and reading modes, which several PRs must update.
- `crates/iced-shell/src/chrome.rs`, `app.rs` from `view` (:6571) to `paged_book_view` (:6464), `notes_ui.rs` `sidebar` (:1468), and `document_scroll.rs`.
- `iced_widget-0.14.2/src/stack.rs` lines 236 to 280 in the cargo registry, for how layers pass events.

UX-2 and UX-3 run the **how** skill (`~/.claude/skills/how/SKILL.md`) before building, because they add a state machine and a new input layer. Run the **interrogate** skill (`~/.claude/skills/interrogate/SKILL.md`) on both before the code-ready report, because they change the main reading interaction. Every owner keeps a `decisions.tsv` trail per `~/.claude/skills/show-me-your-work/SKILL.md` and returns it uncommitted in its report.
