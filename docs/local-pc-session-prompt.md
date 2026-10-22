# Local PC Session Prompt — AetherCast

**Written:** 2026-10-22, against `main` at `b02e3ae`.
**How to use:** on the PC with the Wi-Fi radio, `git pull`, then paste
everything below the line into the local opencode session.

---

You are picking up AetherCast on the machine that actually has the Wi-Fi
radio. The previous session ran on a VPS with **no wireless adapter**, so every
hardware claim is still unverified. Your job is to verify it, honestly.

## 0. Sync first

- `cd` into the repo, then `git fetch --all --prune && git status --short --branch`.
- `main` should be at `b02e3ae` or newer. If behind: `git pull --ff-only`.
- Confirm `gh auth status` is logged in. Do not commit anything until the tree
  is clean and you are on `main`.

## 1. Read before acting (source of truth is on disk)

Read, in this order:

1. `JOURNAL.md` — the two newest entries (2026-10-22), especially "Handoff".
2. `docs/pc-test-checklist.md` — the run book you are about to execute.
3. `docs/architecture.md`, `docs/compatibility.md`, `README.md`.
4. `gh issue list --state open` — open: #7, #12, #13, #18, #19, #20.

What the previous session did (verified there, details in the journal):

- Fixed the wpa_supplicant `EACCES` error path and added fake-socket tests for
  the control protocol.
- Made the daemon actually emit `SessionStateChanged`; the contract and client
  already expected it.
- Added `docs/pc-test-checklist.md`, `docs/protocols/wfd-sink-observed.md`,
  refreshed architecture/roadmap, and redacted personal data from docs.
- 46 daemon tests pass; fmt and clippy are clean; `spike/19` and `spike/04`
  pass. No physical-sink cast has ever been attempted.

## 2. Run the hardware checklist, in order

Execute `docs/pc-test-checklist.md` items 1 through 9, one at a time.
After each item: record the exact command, the output, and PASS/FAIL, then
report back before moving on. Do not batch them.

Environment notes:

- Find the interface with `iw dev`; do not assume `wlan0`.
- On a single-radio adapter the P2P group must share the station's channel:
  take the frequency from `aethercastd doctor` and pass `--freq <mhz>` to
  `p2p group` / `cast --group`.
- Do not run a cast over SSH on the same Wi-Fi you are connected through;
  use ethernet or a local terminal.
- For audio, get the monitor source name from `pactl list short sources`.

## 3. Evidence rules (non-negotiable)

- Every claim must come from a command you ran or a file you captured.
- Save outputs and screenshots under a local path (not in the repo) and
  reference them; if something could not be observed, write `NOT OBSERVED`
  and why.
- **Never** paste or commit the real SSID, BSSID, MAC addresses, private IPs,
  hostnames, or device serial numbers. Redact before writing docs or issues.
- If a step fails, do not "fix" it by editing the checklist to match. Diagnose,
  file an issue with the command and output, and continue only if the next
  step is independent.
- If you cannot test something (no sink, no 5 GHz adapter), say so plainly.

## 4. Record the results

- Add dated rows to `docs/compatibility.md` with adapter, driver, sink, result,
  and a link to the evidence.
- Append a new dated entry to the top of `JOURNAL.md`: what was run, what
  passed, what failed, what it means, and what is next.
- Update `docs/pc-test-checklist.md` only to correct commands that are actually
  wrong, and note why in the commit message.
- Close #12 only if a physical cast succeeded, with the evidence in the
  comment. Keep #13 open unless you actually change the PCR PID.
- If you find new problems, open issues; label hardware-dependent ones
  `needs hardware test`.

## 5. Then continue with the hardware-free backlog

If hardware blocks you (no sink, wrong adapter), do these instead, one unit at
a time:

1. **#19** — make `StopCast` actually stop the pipeline (cancellation flag in
   `Manager`, checked by `cast::run`; update state to `stopped`, which now
   emits a signal, then remove the record).
2. **#20** — make `recover --json` emit only JSON, and stop
   `media test --udp --json` reporting a file that was never written.
3. **#13** — only after reading the spec citations in the issue; a decision
   may be "accept it", recorded in the issue, rather than code.

## 6. Git and GitHub workflow for every unit

- Branch from `main`: `fix/...`, `test/...`, `docs/...`, `feat/...`.
- **Every commit must be signed off:** `git commit -s`. CI enforces DCO and
  will reject unsigned commits. Never force-push or rewrite history.
- Stage explicit files, not `git add -A`.
- Before each commit: `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`,
  `cargo test --workspace --exclude aethercast-client`, and
  `xvfb-run -a cargo test -p aethercast-client` if the client changed.
- Push, open a PR explaining what/why/how tested, wait for
  `gh pr checks <n> --watch`, then squash-merge and delete the branch.
- If a change needs a physical sink to be meaningful, put **needs hardware
  test** in the PR title or body and leave it unmerged until I confirm.

## 7. Stop and report

After each checklist item or code unit: save the files, update the journal,
and report done / next / blockers. Do not invent results to keep going.
