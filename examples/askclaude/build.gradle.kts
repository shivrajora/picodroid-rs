plugins {
    id("picodroid-papk")
}

// The nightly's canned Messages endpoint lives on the test host
// (API_URL=test in test.env); NetTestConfig.HOST names that host.
picodroidNetTest {
    enabled = true
}

// Build-time constants (Android's buildConfigField shape): the API key and the
// model. Pass the key per build — it is baked into the papk, so use a
// workspace-scoped, spend-capped key and never commit one:
//   PICODROID_ANTHROPIC_API_KEY=sk-ant-... ./scripts/flash.sh --app askclaude --board pico_display2_w
// The nightly points API_URL at the test host's TLS listener (test.env).
picodroidBuildConfig {
    fieldFromProperty("ANTHROPIC_API_KEY", "picodroidAnthropicApiKey", "PICODROID_ANTHROPIC_API_KEY", "")
    fieldFromProperty("MODEL", "picodroidAskClaudeModel", "PICODROID_ASKCLAUDE_MODEL", "claude-opus-5")
    fieldFromProperty("API_URL", "picodroidAskClaudeUrl", "PICODROID_ASKCLAUDE_URL", "https://api.anthropic.com/v1/messages")
}
