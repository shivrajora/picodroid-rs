// SPDX-License-Identifier: GPL-3.0-only
package askclaude;

import java.io.IOException;
import javax.net.ssl.SSLHandshakeException;
import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.Thread;
import picodroid.graphics.Color;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.json.JSONArray;
import picodroid.json.JSONException;
import picodroid.json.JSONObject;
import picodroid.net.HttpInputStream;
import picodroid.net.HttpOutputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.URL;
import picodroid.os.Bundle;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.view.KeyEvent;
import picodroid.view.ViewGroup;
import picodroid.widget.LinearLayout;
import picodroid.widget.ScrollView;
import picodroid.widget.TextView;

/**
 * Ask Claude from a Pico: A/B pick a prompt, X sends it to the Messages API over HTTPS, the reply
 * fills the screen, Y clears it — and Y with nothing to clear leaves the app (the Activity default,
 * which is also how the nightly row ends). On a board with sensors (Enviro+) the prompt carries the
 * room readings.
 *
 * <p>The API key and the model are build-time constants ({@code picodroidBuildConfig} in
 * build.gradle.kts): {@code PICODROID_ANTHROPIC_API_KEY=sk-ant-… ./scripts/flash.sh --app askclaude
 * --board pico_display2_w}. Use a workspace-scoped, spend-capped key — it is baked into the papk.
 */
public class MainActivity extends Activity implements SensorEventListener {
  private static final String TAG = "AskClaude";

  private static final String[] PROMPTS = {
    "In one sentence: what is a TLS handshake?",
    "Give me one surprising fact about the RP2350 microcontroller.",
    "Write a haiku about a tiny screen on a desk.",
    "Suggest a name for a two-inch desk display that talks to you.",
  };
  private static final int MAX_TOKENS = 150;
  private static final int TIMEOUT_MS = 20000;
  private static final int MAX_REPLY_BYTES = 4096;

  private TextView promptView;
  private TextView replyView;
  private TextView statusView;
  private int promptIndex;
  private boolean busy;
  private boolean hasReply;

  /** Latest room readings, once a board with the sensors has reported one. */
  private boolean haveReadings;

  private float temperature;
  private float humidity;
  private float pressure;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(6, 4, 6, 4);
    root.setSpacing(4);

    TextView title = new TextView(this);
    title.setText("Ask Claude");
    title.setTextColor(Color.rgb(255, 200, 80));
    root.addView(title);

    // The prompt wraps at the panel's width (two lines at most) and the reply takes the rest of
    // the column: the root is the window, whichever board this is.
    promptView = new TextView(this);
    promptView.setTextColor(Color.rgb(200, 200, 200));
    promptView.setMaxLines(2);
    root.addView(
        promptView,
        new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));

    ScrollView scroll = new ScrollView(this);
    replyView = new TextView(this);
    replyView.setText("A/B: pick a prompt   X: ask   Y: clear");
    replyView.setSize(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT);
    scroll.addView(replyView);
    root.addView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));

    statusView = new TextView(this);
    statusView.setTextColor(Color.rgb(120, 200, 120));
    statusView.setSingleLine();
    root.addView(statusView);

    setContentView(root);
    showPrompt();
    setStatus(BuildConfig.ANTHROPIC_API_KEY.isEmpty() ? "no API key: see README" : "ready");
    startSensors();
    Log.i(TAG, "ready");
  }

  /**
   * The Messages endpoint: {@code BuildConfig.API_URL}, or when the nightly sets it to {@code
   * test}, the test host's TLS listener (scripts/tls-listener.py), which answers with a canned
   * reply.
   */
  private static String apiUrl() {
    if (BuildConfig.API_URL.equals("test")) {
      return "https://" + NetTestConfig.HOST + ":8443/v1/messages";
    }
    return BuildConfig.API_URL;
  }

  @Override
  public boolean onKeyDown(int keyCode, KeyEvent event) {
    switch (keyCode) {
      case KeyEvent.KEYCODE_DPAD_UP:
        promptIndex = (promptIndex + PROMPTS.length - 1) % PROMPTS.length;
        showPrompt();
        return true;
      case KeyEvent.KEYCODE_DPAD_DOWN:
        promptIndex = (promptIndex + 1) % PROMPTS.length;
        showPrompt();
        return true;
      case KeyEvent.KEYCODE_DPAD_CENTER:
        ask();
        return true;
      case KeyEvent.KEYCODE_BACK:
        if (!hasReply) {
          return super.onKeyDown(keyCode, event); // nothing to clear: leave, as BACK does
        }
        replyView.setText("");
        hasReply = false;
        setStatus("ready");
        return true;
      default:
        return super.onKeyDown(keyCode, event);
    }
  }

  private void showPrompt() {
    promptView.setText((promptIndex + 1) + "/" + PROMPTS.length + "  " + PROMPTS[promptIndex]);
  }

  private void setStatus(String s) {
    statusView.setText(s);
  }

  /** One request in flight at a time; the network work runs on its own thread. */
  private void ask() {
    if (busy) {
      return;
    }
    if (BuildConfig.ANTHROPIC_API_KEY.isEmpty()) {
      Log.i(TAG, "no API key (PICODROID_ANTHROPIC_API_KEY at build time)");
      setStatus("no API key: see README");
      return;
    }
    busy = true;
    final String prompt = buildPrompt();
    setStatus("asking...");
    Log.i(TAG, "ask: " + prompt);
    new Thread(() -> request(prompt), "askclaude-http").start();
  }

  /** The chosen prompt, with the room readings when the board measures them. */
  private String buildPrompt() {
    String prompt = PROMPTS[promptIndex];
    if (haveReadings) {
      prompt =
          "My room reads "
              + (int) temperature
              + " C, "
              + (int) humidity
              + "% humidity, "
              + (int) pressure
              + " hPa. "
              + prompt;
    }
    return prompt;
  }

  private void request(String prompt) {
    String status;
    String reply;
    try {
      JSONObject body = new JSONObject();
      body.put("model", BuildConfig.MODEL);
      body.put("max_tokens", MAX_TOKENS);
      JSONArray messages = new JSONArray();
      JSONObject user = new JSONObject();
      user.put("role", "user");
      user.put("content", prompt);
      messages.put(user);
      body.put("messages", messages);
      byte[] bytes = body.toString().getBytes();

      HttpURLConnection c = new URL(apiUrl()).openConnection();
      try {
        c.setConnectTimeout(TIMEOUT_MS);
        c.setReadTimeout(TIMEOUT_MS);
        c.setRequestMethod("POST");
        c.setDoOutput(true);
        c.setFixedLengthStreamingMode(bytes.length);
        c.setRequestProperty("x-api-key", BuildConfig.ANTHROPIC_API_KEY);
        c.setRequestProperty("anthropic-version", "2023-06-01");
        c.setRequestProperty("content-type", "application/json");
        long t0 = SystemClock.elapsedRealtime();
        c.connect();
        HttpOutputStream out = c.getOutputStream();
        out.write(bytes);
        int code = c.getResponseCode();
        String json = readAll(code < 400 ? c.getInputStream() : c.getErrorStream());
        long took = SystemClock.elapsedRealtime() - t0;
        if (code == HttpURLConnection.HTTP_OK) {
          JSONObject msg = new JSONObject(json);
          reply = msg.getJSONArray("content").getJSONObject(0).getString("text");
          JSONObject usage = msg.getJSONObject("usage");
          status =
              "ok in "
                  + took
                  + " ms, "
                  + usage.getInt("input_tokens")
                  + " in / "
                  + usage.getInt("output_tokens")
                  + " out";
          Log.i(TAG, "reply: " + reply);
          Log.i(TAG, "usage: " + status);
        } else {
          reply = errorMessage(json, code);
          status = "HTTP " + code;
          Log.i(TAG, "http " + code + ": " + reply);
        }
      } finally {
        c.disconnect();
      }
    } catch (SSLHandshakeException e) {
      reply = "TLS: " + e.getMessage();
      status = "handshake failed";
      Log.i(TAG, "tls: " + e.getMessage());
    } catch (IOException e) {
      reply = "network: " + e.getMessage();
      status = "request failed";
      Log.i(TAG, "io: " + e.getMessage());
    } catch (JSONException e) {
      reply = "bad reply: " + e.getMessage();
      status = "bad reply";
      Log.i(TAG, "json: " + e.getMessage());
    }
    final String finalReply = reply;
    final String finalStatus = status;
    Executors.mainExecutor()
        .execute(
            () -> {
              replyView.setText(finalReply);
              hasReply = true;
              setStatus(finalStatus);
              busy = false;
            });
  }

  private static String readAll(HttpInputStream in) throws IOException {
    if (in == null) {
      return "";
    }
    byte[] buf = new byte[MAX_REPLY_BYTES];
    int total = 0;
    int n;
    while (total < buf.length && (n = in.read(buf, total, buf.length - total)) > 0) {
      total += n;
    }
    return new String(buf, 0, total);
  }

  /** The API's `{"error": {"type", "message"}}` body, or the bare status. */
  private static String errorMessage(String json, int code) {
    try {
      JSONObject err = new JSONObject(json).getJSONObject("error");
      return err.getString("type") + ": " + err.getString("message");
    } catch (JSONException e) {
      return "HTTP " + code;
    }
  }

  // ── Room readings, on a board that has the sensors ──────────────────────

  private void startSensors() {
    SensorManager sm = SensorManager.getInstance();
    Sensor t = sm.getDefaultSensor(Sensor.TYPE_AMBIENT_TEMPERATURE);
    Sensor h = sm.getDefaultSensor(Sensor.TYPE_RELATIVE_HUMIDITY);
    Sensor p = sm.getDefaultSensor(Sensor.TYPE_PRESSURE);
    if (t != null) {
      sm.registerListener(this, t, SensorManager.SENSOR_DELAY_NORMAL);
    }
    if (h != null) {
      sm.registerListener(this, h, SensorManager.SENSOR_DELAY_NORMAL);
    }
    if (p != null) {
      sm.registerListener(this, p, SensorManager.SENSOR_DELAY_NORMAL);
    }
  }

  @Override
  public void onSensorChanged(SensorEvent event) {
    switch (event.sensor.getType()) {
      case Sensor.TYPE_AMBIENT_TEMPERATURE:
        temperature = event.values[0];
        haveReadings = true;
        break;
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        humidity = event.values[0];
        break;
      case Sensor.TYPE_PRESSURE:
        pressure = event.values[0];
        break;
      default:
        break;
    }
  }

  @Override
  public void onAccuracyChanged(Sensor sensor, int accuracy) {}
}
