# Handover: upstreaming the cyw43-driver `bsscfg:event_msgs` fix (NET-3)

Everything is prepared; what remains is refreshing against upstream `main`,
pushing, and opening the PR — outward-facing steps deliberately left manual.
Written to be executed cold.

## Context

`vendor/cyw43-driver` is the `picodroid` branch of `shivrajora/cyw43-driver`
— upstream `georgerobotics/cyw43-driver` at `055d642` plus one squashed
commit (`120081e`) carrying four `PICODROID`-marked patches in
`src/cyw43_ll.c`:

1. **`bsscfg:event_msgs` index fix** (~line 1915) — the one worth
   upstreaming. `cyw43_ll_bus_init` sends the event-mask iovar through a
   shared scratch buffer but never initialises the 4-byte bsscfg index that
   precedes the mask, so the mask is applied to whatever stale index an
   earlier ioctl left behind. On our boot sequence async join events never
   arrived and a join could never complete. Universal bug, one-line fix.
2. ioctl error-status logging (~line 870) — possible *second* PR, see below.
3. `STATUS_ENABLE` in the gSPI bus-config word (~line 1444) — port-specific,
   not for upstreaming.
4. F2 boot-gate widened to 3000 iterations (~line 1708) — port-specific.

## Current prepared state

Branch **`upstream-bsscfg-event-msgs`** in `vendor/cyw43-driver`, commit
`d03e19c`, based directly on `055d642` (the fork's upstream base). It
contains only patch 1, with the `PICODROID` marker removed and a commit
message written for an outside audience. Verify it's still there:

```bash
cd vendor/cyw43-driver
git log --oneline -1 upstream-bsscfg-event-msgs   # d03e19c cyw43_ll: zero the bsscfg index ...
git show upstream-bsscfg-event-msgs --stat        # 1 file changed, 6 insertions(+)
```

## Pre-flight (do these before pushing)

1. **Refresh against upstream `main`.** The base is upstream HEAD as of the
   2026-08 fork; upstream may have moved:

   ```bash
   cd vendor/cyw43-driver
   git remote add upstream https://github.com/georgerobotics/cyw43-driver.git 2>/dev/null
   git fetch upstream
   git log --oneline 055d642..upstream/main -- src/cyw43_ll.c
   ```

   If that range is non-empty, rebase the branch
   (`git rebase upstream/main upstream-bsscfg-event-msgs`) and re-check the
   hunk applies where expected (search `Clear all async events`). Also
   search upstream's open PRs/issues for `event_msgs` / `bsscfg` in case
   someone beat us to it — if so, drop our branch and note the upstream PR
   in `docs/networking-followups-2026-08.md` NET-3 instead.
2. **Check contribution requirements.** The vendored tree has no
   CONTRIBUTING.md; check the GitHub repo for a CONTRIBUTING file or CLA
   requirement at submission time and follow it.

## Submit

```bash
cd vendor/cyw43-driver
git push fork upstream-bsscfg-event-msgs
gh pr create --repo georgerobotics/cyw43-driver \
  --head shivrajora:upstream-bsscfg-event-msgs \
  --title "cyw43_ll: zero the bsscfg index in the event_msgs iovar payload"
```

Suggested PR body:

> `cyw43_ll_bus_init` sends `bsscfg:event_msgs` through the shared scratch
> buffer but never initialises the 4-byte bsscfg index that precedes the
> event mask, so the mask is applied to whatever index an earlier ioctl left
> in the buffer. Whether async join events (`EV_AUTH`, `EV_LINK`,
> `EV_PSK_SUP`, …) arrive at all then depends on the preceding boot
> sequence — on our (FreeRTOS+TCP, non-lwIP) boot sequence they never
> arrived and a join could never complete.
>
> Zero the index explicitly, matching the `{ u32 bsscfg_idx; u8 mask[N] }`
> layout the firmware expects. Found on a Pico 2 W (CYW43439, fw 7.95.49);
> likely latent on any port depending on prior buffer contents.

## After it merges

At the next fork rebase, drop the fork's equivalent hunk (the `PICODROID`
comment above `cyw43_put_le32(buf + 18, 0)` marks it) and take upstream's.
The other three markers remain, so the build guard in
`build_support/network.rs` (asserts `PICODROID` appears in `cyw43_ll.c`)
still passes. After any fork rebase, re-validate on hardware: flash netdemo
with creds and confirm join + DHCP + echo (recipe in
`docs/networking-followups-2026-08.md`, validation notes).

## Optional second PR: ioctl error-status logging

Patch 2 replaces upstream's commented-out TODO ("need to handle errors and
pass them up") with a `CYW43_WARN` on non-zero ioctl status. It's how NET-1's
BCME -5 rejections became visible at all — upstream silently discards
firmware errors, so stock Pico W setups likely hit the same rejections
invisibly. Propose it only if the first PR lands and the maintainers seem
receptive; expect pushback on log volume (offering it gated behind
`CYW43_VERBOSE_DEBUG` is the likely compromise). Not bundled with the first
PR on purpose: the bsscfg fix should not be held hostage to a logging debate.
