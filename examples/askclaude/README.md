# askclaude

Ask Claude from a Pico. A and B pick one of four prompts, X sends it to the
Claude Messages API over HTTPS, the reply fills the screen and the status line
shows the round trip and the token usage. On a board with the Enviro+ sensors
the prompt carries the room readings, so the question becomes "how comfortable
is it in here?". Y clears the screen.

It is the showcase for the runtime's TLS (docs/designs/tls-2026-09.md): the
Anthropic API has no cleartext fallback, its chain is verified against the
compiled-in root store, and the wall clock the certificate check needs comes
from an SNTP round trip the app makes before its first request.

## Building with a key

The API key and the model are build-time constants of the papk, from the
`picodroidBuildConfig` block in `build.gradle.kts` (Android's
`buildConfigField` shape), read from a Gradle property or an environment
variable:

```bash
PICODROID_ANTHROPIC_API_KEY=sk-ant-api03-... ./scripts/flash.sh --app askclaude --board pico_display2_w
PICODROID_ANTHROPIC_API_KEY=sk-ant-api03-... ./scripts/sim.sh --app askclaude --board pico_display2_w
```

The key is baked into the app and sits in flash in the clear, so anyone with
the board or the `build/` directory can read it. Mint it in the Console for a
workspace of its own with a spend limit, and revoke it when the board leaves
your desk. Never commit one. `PICODROID_ASKCLAUDE_MODEL` overrides the model
(default `claude-opus-5`).

Without a key the status line says so and X does nothing.

## What it exercises

- `HttpsURLConnection` from `URL.openConnection()`, a POST with
  `setFixedLengthStreamingMode`, request headers, `getErrorStream()` for the
  API's error JSON.
- `SntpClient` to anchor `System.currentTimeMillis()` before the handshake
  (certificate validity is checked against it; the runtime refuses to
  handshake with the clock unset).
- `picodroid.json` to build the request and read `content[0].text` and
  `usage` from the reply.
- One request in flight, on its own `Thread`; the reply lands on the main
  thread through `Executors.mainExecutor()`.

## The nightly

`scripts/hil-tests.conf` runs it as a `net` row against the test host's TLS
listener (`scripts/tls-listener.py`), which answers `POST /v1/messages` with a
canned reply for any key: `test.env` sets a placeholder key and points the
endpoint at the listener, `test.ctrl` presses X once the screen is up.
