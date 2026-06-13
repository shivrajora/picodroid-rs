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
| `LV_USE_LODEPNG` / `LV_USE_LIBPNG` | `0` | Both PNG decoders disabled — PNG is decoded at PAPK-pack time, never on-device. See [Bundled image assets](/guides/assets/). |
| `LV_FONT_MONTSERRAT_*` | tuned per-board | Only the sizes the framework actually renders are pulled in. |

Bumping LVGL: vendor at `vendor/lvgl`, then re-vet `lv_conf.h` against `vendor/lvgl/src/lv_conf_template.h`. Anything new defaults to upstream behavior.

## `Embed.toml`

Config for the `cargo embed` probe-rs subcommand, handy for interactive ARM debug sessions. Selects the chip, RTT polling rate, and breakpoint set. Defaults work for testbench RP2040 / RP2350; override only if you're debugging a custom board with a non-standard probe wiring.

```toml
[default.general]
chip = "RP2040"

[default.rtt]
enabled = true
up_mode = "NoBlockSkip"
```

Note that `./scripts/flash.sh` flashes via `cargo run` (the `probe-rs` runner configured in `.cargo/config.toml`), **not** `cargo embed` — so most users never touch this file.

## `.actrc`

Lets you run the GitHub Actions workflows locally via the [`act`](https://github.com/nektos/act) tool, useful for catching workflow bugs before pushing to the repo:

```ini
-P ubuntu-latest=catthehacker/ubuntu:act-22.04
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

Bare `cargo test` fails because the `picodroid` firmware crate is bare-metal (no host test harness) and there is no default cargo target set, so cargo can't pick a host triple on its own. `scripts/test.sh` runs the tests against the host triple and rebuilds the APK first so any embedded test fixtures are fresh:

```bash
./scripts/test.sh                # all crates
./scripts/test.sh --crate jvm    # just the JVM tests
```

CI uses the same wrapper, so passing locally is a strong predictor of green CI.

If you're chasing a specific failure, `scripts/test.sh -- --nocapture <test_name>` forwards args through to `cargo test` exactly like a normal invocation.

## Out-of-tree app builds

App projects under `examples/` build as subprojects of the picodroid repo by default. To build an app that lives outside the repo against a picodroid checkout, point two Gradle properties at it:

```bash
./gradlew :myapp:assemblePapk \
    -Ppicodroid.repoRoot=/path/to/picodroid-rs \
    -Ppicodroid.sdkProjectPath=:sdk
```

- `picodroid.repoRoot` — the picodroid source tree (holds `tools/`, `sdk/`, `scripts/`). Default: the build's root project dir.
- `picodroid.sdkProjectPath` — Gradle path of the `:sdk` project to compile against. Default `:sdk`.

This is path indirection only — there is no published Maven artifact or project template; the app still composes the picodroid build (e.g. via an included build).

## See also

- [Cargo aliases](/reference/cargo-aliases/) — board-specific build commands.
- [Class-name shrinker](/reference/shrinker/) — release-versioned framework class renaming.
- [Porting guide](/reference/porting-guide/) — board.toml, FreeRTOSConfig, HAL contract.
