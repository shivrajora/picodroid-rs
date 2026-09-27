# weather

A weather app in the shape of the phone ones, for the four-button
`pico_enviro_mon_w` (and any RP2350 Wi-Fi board with a display). Three pages:

- **Today** — the place, the temperature large with the condition glyph beside
  it, the condition, the day's high and low, and the next hours in a strip.
- **7 days** — one row per day: the glyph, the low, a bar spanning the day's
  range across the week's, the high.
- **Details** — feels-like, humidity, wind, pressure, sunrise and sunset, and on
  a board with the Enviro+ sensors the room's own temperature, humidity and
  pressure.

The sky behind everything follows the conditions and the time of day. A and B
turn the page, X refreshes now, Y leaves. The forecast refreshes itself every
fifteen minutes.

Everything comes from one [open-meteo](https://open-meteo.com) request over
HTTPS (docs/designs/tls-2026-09.md): `URL.openConnection()` on an `https` URL,
the runtime's TLS 1.3 client verifying open-meteo's Let's Encrypt chain against
the compiled-in roots. The certificate check needs the wall clock, so the first
fetch anchors it with `SntpClient`.

## The place

The place and the units are build-time constants of the papk, from the
`picodroidBuildConfig` block in `build.gradle.kts` (Android's
`buildConfigField` shape), read from a Gradle property or an environment
variable:

```bash
PICODROID_WEATHER_CITY=Tokyo PICODROID_WEATHER_LATITUDE=35.68 PICODROID_WEATHER_LONGITUDE=139.69 \
  ./scripts/flash.sh --app weather --board pico_enviro_mon_w
PICODROID_WEATHER_UNITS=fahrenheit ./scripts/sim.sh --app weather --board pico_enviro_mon_w
```

The defaults are San Mateo in degrees Celsius and km/h.

## What it exercises

- `HttpsURLConnection` from `URL.openConnection()`, a GET against a public
  RSA-rooted host (open-meteo's chain ends at ISRG Root X1).
- `SntpClient` to anchor `System.currentTimeMillis()` before the handshake.
- `picodroid.json` over a reply with nested objects and arrays of numbers
  (`timeformat=unixtime`, so every time is a `long`).
- `View.onDraw(Canvas)`: the condition glyphs are circles, rounded rectangles
  and lines drawn at any size; the hourly strip and the seven-day list are one
  view each, well inside a view's two-kilobyte op recording.
- `TextView.setTextSize` (the 20 and 64 px faces), `GradientDrawable` as a
  full-screen sky, `Activity.onKeyDown` for the four buttons,
  `ScheduledExecutorService` on the main thread for the refresh, and
  `SensorManager` for the room readings.

## The nightly

`scripts/hil-tests.conf` runs it as a `net` row against the test host's TLS
listener (`scripts/tls-listener.py`), which answers `GET /v1/forecast` with a
canned forecast: `test.env` points the endpoint at the listener and anchors the
simulator's clock, `test.ctrl` waits for the screen to update, turns the pages
and presses Y.
