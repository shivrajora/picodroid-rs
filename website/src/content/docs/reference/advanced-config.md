---
title: "Advanced configuration"
description: "Files outside board.toml that change behavior — lv_conf.h, Embed.toml, .actrc, and the scripts/test.sh wrapper."
---

Most builds work without touching anything below — these files are here when you need them. Day-to-day app development needs none of this.

## `lv_conf.h`

LVGL is vendored in `vendor/lvgl/` and configured via the repo-root [`lv_conf.h`](https://github.com/shivrajora/picodroid-rs/blob/main/lv_conf.h). It overrides selected upstream defaults:

| Symbol | Picodroid value | Why |
|---|---|---|
| `LV_COLOR_DEPTH` | `16` | RGB565 framebuffers — matches ST7789 + minifb. |
| `LV_DRAW_SW_SUPPORT_RGB565A8` | `1` | Anti-aliased scaled / rotated images via `ImageView.setScale`. Without it scaled images render aliased — see [LVGL release notes for 9.5.0](https://github.com/lvgl/lvgl/releases/tag/v9.5.0). |
| `LV_USE_PNG` | `0` | PNG decoded at PAPK-pack time, never on-device. See [Bundled image assets](/guides/assets/). |
| `LV_FONT_MONTSERRAT_*` | tuned per-board | Only the sizes the framework actually renders are pulled in. |

Bumping LVGL: vendor at `vendor/lvgl`, then re-vet `lv_conf.h` against `vendor/lvgl/src/lv_conf_template.h`. Anything new defaults to upstream behavior.

## `Embed.toml`

Used by `cargo embed` (probe-rs) for ARM debug. Selects the chip, RTT polling rate, and breakpoint set. Defaults work for testbench RP2040 / RP2350; override only if you're debugging a custom board with a non-standard probe wiring.

```toml
[default.general]
chip = "RP235x"

[default.rtt]
enabled = true
up_mode = "NoBlockSkip"
```

`./scripts/flash.sh` calls `cargo embed` under the hood, so most users never touch this file.

## `.actrc`

Lets you run the GitHub Actions workflows locally via the [`act`](https://github.com/nektos/act) tool, useful for catching workflow bugs before pushing to the repo:

```ini
-P ubuntu-22.04=catthehacker/ubuntu:act-22.04
```

Install `act`:

```bash
brew install act           # macOS
gh extension install nektos/gh-act    # via gh CLI
```

Run the CI workflow locally:

```bash
act -W .github/workflows/ci_checks.yml -j build
```

`.actrc` lives at the repo root so `act` picks it up automatically.

## `scripts/test.sh` (host-target test wrapper)

Bare `cargo test` fails because the workspace's default target is `thumbv6m-none-eabi` — there's no host harness for the firmware crate. `scripts/test.sh` switches the target to the host triple and rebuilds the APK first so any embedded test fixtures are fresh:

```bash
./scripts/test.sh                # all crates
./scripts/test.sh --crate jvm    # just the JVM tests
```

CI uses the same wrapper, so passing locally is a strong predictor of green CI.

If you're chasing a specific failure, `scripts/test.sh -- --nocapture <test_name>` forwards args through to `cargo test` exactly like a normal invocation.

## See also

- [Cargo aliases](/reference/cargo-aliases/) — board-specific build commands.
- [Class-name shrinker](/reference/shrinker/) — release-versioned framework class renaming.
- [Porting guide](/reference/porting-guide/) — board.toml, FreeRTOSConfig, HAL contract.
