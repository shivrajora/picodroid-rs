// SPDX-License-Identifier: GPL-3.0-only
package weather;

import java.io.IOException;
import javax.net.ssl.SSLHandshakeException;
import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.Thread;
import picodroid.concurrent.TimeUnit;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.json.JSONException;
import picodroid.net.HttpInputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.NetworkInfo;
import picodroid.net.SntpClient;
import picodroid.net.URL;
import picodroid.os.Bundle;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.view.Gravity;
import picodroid.view.KeyEvent;
import picodroid.view.View;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * A weather app in the shape of the phone ones. Today: the place, the temperature large with the
 * condition beside it, the high and low, and the next hours in a strip. The week: one row per day
 * with a bar spanning the day's range across the week's. Details: feels-like, humidity, wind,
 * pressure, sunrise and sunset, and on a board with the Enviro+ sensors the room's own readings.
 * The sky behind it all follows the conditions and the time of day.
 *
 * <p>Everything comes from one open-meteo request over HTTPS (docs/designs/tls-2026-09.md) every
 * {@link #REFRESH_MINUTES} minutes, on a thread of its own; the handshake needs the wall clock, so
 * the first fetch anchors it with {@link SntpClient} first. Four buttons: A and B turn the page, X
 * refreshes now, Y leaves.
 *
 * <p>The place and the units are build-time constants ({@code picodroidBuildConfig} in
 * build.gradle.kts): {@code PICODROID_WEATHER_CITY=Tokyo PICODROID_WEATHER_LATITUDE=35.68
 * PICODROID_WEATHER_LONGITUDE=139.69 ./scripts/flash.sh --app weather --board pico_enviro_mon_w};
 * {@code PICODROID_WEATHER_UNITS=fahrenheit} for degrees Fahrenheit and mph.
 */
public class MainActivity extends Activity implements SensorEventListener {
  private static final String TAG = "Weather";

  /** How long to wait for the network before giving up (WiFi join + DHCP). */
  private static final int NETWORK_WAIT_MS = 30000;

  private static final int TIMEOUT_MS = 10000;
  private static final int MAX_REPLY_BYTES = 4096;
  private static final int REFRESH_MINUTES = 15;
  private static final int PAGES = 3;
  private static final int PAGE_TODAY = 0;
  private static final int PAGE_WEEK = 1;
  private static final int PAGE_DETAILS = 2;
  private static final int PAD = 8;
  private static final int HEADER = 24;
  private static final int STATUS_WIDTH = 64;
  private static final int LINE = 16;
  private static final int HERO_ICON = 64;
  private static final String DEGREE = "°";

  static final boolean FAHRENHEIT = BuildConfig.UNITS.equals("fahrenheit");

  private static final int TEXT = Color.rgb(255, 255, 255);
  private static final int TEXT_DIM = Color.rgb(214, 222, 236);

  // Skies, top and bottom of the gradient.
  private static final int DAY_CLEAR_TOP = Color.rgb(58, 140, 232);
  private static final int DAY_CLEAR_BOTTOM = Color.rgb(140, 196, 250);
  private static final int DAY_CLOUD_TOP = Color.rgb(96, 110, 132);
  private static final int DAY_CLOUD_BOTTOM = Color.rgb(156, 168, 186);
  private static final int DAY_RAIN_TOP = Color.rgb(58, 70, 98);
  private static final int DAY_RAIN_BOTTOM = Color.rgb(104, 120, 148);
  private static final int DAY_SNOW_TOP = Color.rgb(124, 146, 178);
  private static final int DAY_SNOW_BOTTOM = Color.rgb(196, 208, 226);
  private static final int NIGHT_CLEAR_TOP = Color.rgb(14, 24, 62);
  private static final int NIGHT_CLEAR_BOTTOM = Color.rgb(46, 60, 114);
  private static final int NIGHT_CLOUD_TOP = Color.rgb(26, 32, 50);
  private static final int NIGHT_CLOUD_BOTTOM = Color.rgb(60, 68, 92);

  private LinearLayout root;
  private final LinearLayout[] pages = new LinearLayout[PAGES];
  private final TextView[] statusViews = new TextView[PAGES];
  private TextView tempView;
  private TextView conditionView;
  private TextView highLowView;
  private ConditionIcon icon;
  private HourlyStrip hourly;
  private DailyList daily;
  private TextView feelsView;
  private TextView humidityView;
  private TextView windView;
  private TextView pressureView;
  private TextView sunView;
  private TextView indoorView;

  private int page;
  private boolean busy;
  private boolean clockSynced;
  private ScheduledExecutorService timer;
  private int skyTop;
  private int skyBottom;

  /** Latest room readings, once a board with the sensors has reported one. */
  private boolean haveReadings;

  private float roomTemperature;
  private float roomHumidity;
  private float roomPressure;
  private String indoorText = "";

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    int width = getDisplay().getWidth();
    int height = getDisplay().getHeight();
    int content = width - 2 * PAD;
    int pageHeight = height - 2 * 6 - LINE - 2;

    root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(width, height);
    // A weather display is glanced at, not touched: hold the panel on.
    root.setKeepScreenOn(true);
    root.setPadding(PAD, 6, PAD, 6);
    root.setSpacing(2);
    sky(NIGHT_CLEAR_TOP, NIGHT_CLEAR_BOTTOM);

    buildToday(content, pageHeight);
    buildWeek(content, pageHeight);
    buildDetails(content, pageHeight);
    for (int i = 0; i < PAGES; i++) {
      root.addView(pages[i]);
    }

    TextView hints = new TextView(this);
    hints.setText("A/B:Page  X:Refresh  Y:Exit");
    hints.setTextColor(TEXT_DIM);
    hints.setSingleLine();
    root.addView(hints);

    setContentView(root);
    showPage(PAGE_TODAY);
    startSensors();
    Log.i(TAG, "ready");

    refresh();
    timer = Executors.mainScheduledExecutor();
    timer.scheduleAtFixedRate(() -> refresh(), REFRESH_MINUTES, REFRESH_MINUTES, TimeUnit.MINUTES);
  }

  @Override
  public void onDestroy() {
    if (timer != null) {
      timer.shutdownNow();
      timer = null;
    }
    super.onDestroy();
  }

  // ── The pages ──────────────────────────────────────────────────────────

  /** A page: a header row (title left, status right) over its content, hidden until shown. */
  private LinearLayout newPage(int index, int width, int height, String title) {
    LinearLayout p = new LinearLayout(this);
    p.setOrientation(LinearLayout.VERTICAL);
    p.setSize(width, height);
    p.setSpacing(2);

    LinearLayout header = new LinearLayout(this);
    header.setOrientation(LinearLayout.HORIZONTAL);
    header.setSize(width, HEADER);
    header.setGravity(Gravity.CENTER_VERTICAL);
    TextView t = new TextView(this);
    t.setText(title);
    t.setTextSize(20);
    t.setTextColor(TEXT);
    t.setSingleLine();
    t.setSize(width - STATUS_WIDTH, HEADER);
    header.addView(t);
    TextView s = new TextView(this);
    s.setTextColor(TEXT_DIM);
    s.setSingleLine();
    s.setGravity(Gravity.RIGHT);
    s.setSize(STATUS_WIDTH, LINE);
    header.addView(s);
    p.addView(header);

    statusViews[index] = s;
    pages[index] = p;
    return p;
  }

  private void buildToday(int width, int height) {
    LinearLayout p = newPage(PAGE_TODAY, width, height, BuildConfig.CITY);

    LinearLayout hero = new LinearLayout(this);
    hero.setOrientation(LinearLayout.HORIZONTAL);
    hero.setGravity(Gravity.CENTER_VERTICAL);
    tempView = new TextView(this);
    tempView.setText("--" + DEGREE);
    tempView.setTextSize(64);
    tempView.setIncludeFontPadding(false);
    tempView.setTextColor(TEXT);
    tempView.setSingleLine();
    // The 64 px face is taller than the icon: size the row to it.
    int heroHeight = Math.max(HERO_ICON, tempView.getLineHeight());
    tempView.setSize(width - HERO_ICON, heroHeight);
    hero.setSize(width, heroHeight);
    hero.addView(tempView);
    icon = new ConditionIcon(this, HERO_ICON);
    hero.addView(icon);
    p.addView(hero);

    conditionView = new TextView(this);
    conditionView.setText("Waiting for the forecast");
    conditionView.setTextColor(TEXT);
    conditionView.setSingleLine();
    p.addView(conditionView);

    highLowView = new TextView(this);
    highLowView.setTextColor(TEXT_DIM);
    highLowView.setSingleLine();
    p.addView(highLowView);

    hourly = new HourlyStrip(this, width);
    p.addView(hourly);
  }

  private void buildWeek(int width, int height) {
    LinearLayout p = newPage(PAGE_WEEK, width, height, "7 days");
    daily = new DailyList(this, width);
    p.addView(daily);
  }

  private void buildDetails(int width, int height) {
    LinearLayout p = newPage(PAGE_DETAILS, width, height, "Details");
    p.setGravity(Gravity.LEFT);
    p.setSpacing(6);
    feelsView = detailLine(p);
    humidityView = detailLine(p);
    windView = detailLine(p);
    pressureView = detailLine(p);
    sunView = detailLine(p);
    indoorView = detailLine(p);
    indoorView.setText("Indoor  no sensors on this board");
  }

  private TextView detailLine(LinearLayout page) {
    TextView v = new TextView(this);
    v.setTextColor(TEXT);
    v.setSingleLine();
    page.addView(v);
    return v;
  }

  private void showPage(int index) {
    page = index;
    for (int i = 0; i < PAGES; i++) {
      pages[i].setVisibility(i == index ? View.VISIBLE : View.GONE);
    }
  }

  private void setStatus(String s) {
    for (int i = 0; i < PAGES; i++) {
      statusViews[i].setText(s);
    }
  }

  /** The gradient behind everything; re-applied only when it changes. */
  private void sky(int top, int bottom) {
    if (top == skyTop && bottom == skyBottom) {
      return;
    }
    skyTop = top;
    skyBottom = bottom;
    root.setBackground(
        new GradientDrawable().setGradient(top, bottom, GradientDrawable.Orientation.TOP_BOTTOM));
  }

  private void skyFor(Forecast f) {
    int glyph = WeatherIcons.glyph(f.weatherCode, f.isDay);
    if (!f.isDay) {
      boolean clear = glyph == WeatherIcons.MOON || glyph == WeatherIcons.PARTLY_MOON;
      sky(
          clear ? NIGHT_CLEAR_TOP : NIGHT_CLOUD_TOP,
          clear ? NIGHT_CLEAR_BOTTOM : NIGHT_CLOUD_BOTTOM);
      return;
    }
    switch (glyph) {
      case WeatherIcons.SUN:
      case WeatherIcons.PARTLY_SUN:
        sky(DAY_CLEAR_TOP, DAY_CLEAR_BOTTOM);
        break;
      case WeatherIcons.SNOW:
        sky(DAY_SNOW_TOP, DAY_SNOW_BOTTOM);
        break;
      case WeatherIcons.DRIZZLE:
      case WeatherIcons.RAIN:
      case WeatherIcons.THUNDER:
        sky(DAY_RAIN_TOP, DAY_RAIN_BOTTOM);
        break;
      default:
        sky(DAY_CLOUD_TOP, DAY_CLOUD_BOTTOM);
        break;
    }
  }

  // ── Buttons ────────────────────────────────────────────────────────────

  @Override
  public boolean onKeyDown(int keyCode, KeyEvent event) {
    switch (keyCode) {
      case KeyEvent.KEYCODE_DPAD_UP:
        showPage((page + PAGES - 1) % PAGES);
        return true;
      case KeyEvent.KEYCODE_DPAD_DOWN:
        showPage((page + 1) % PAGES);
        return true;
      case KeyEvent.KEYCODE_DPAD_CENTER:
        refresh();
        return true;
      default:
        return super.onKeyDown(keyCode, event);
    }
  }

  // ── The fetch ──────────────────────────────────────────────────────────

  /** One request in flight at a time; the network work runs on its own thread. */
  private void refresh() {
    if (busy) {
      return;
    }
    busy = true;
    setStatus("Updating");
    new Thread(() -> fetchOnThread(), "weather-http").start();
  }

  /**
   * The forecast endpoint: {@code BuildConfig.API_URL}, or when the nightly sets it to {@code
   * test}, the test host's TLS listener (scripts/tls-listener.py), which answers with a canned
   * forecast.
   */
  private static String url() {
    String base =
        BuildConfig.API_URL.equals("test")
            ? "https://" + NetTestConfig.HOST + ":8443/v1/forecast"
            : BuildConfig.API_URL;
    return base
        + "?latitude="
        + BuildConfig.LATITUDE
        + "&longitude="
        + BuildConfig.LONGITUDE
        + "&current=temperature_2m,relative_humidity_2m,apparent_temperature,is_day"
        + ",weather_code,surface_pressure,wind_speed_10m"
        + "&hourly=temperature_2m,weather_code,is_day"
        + "&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset"
        + "&timezone=auto&timeformat=unixtime&forecast_days="
        + Forecast.DAYS
        + "&forecast_hours="
        + Forecast.HOURS
        + (FAHRENHEIT ? "&temperature_unit=fahrenheit&wind_speed_unit=mph" : "");
  }

  private void fetchOnThread() {
    int waited = 0;
    while (!NetworkInfo.isConnected() && waited < NETWORK_WAIT_MS) {
      SystemClock.sleep(500);
      waited += 500;
    }
    if (!NetworkInfo.isConnected()) {
      Log.i(TAG, "fetch failed: no network");
      post(null, "No network");
      return;
    }
    syncClockOnce();
    long t0 = SystemClock.elapsedRealtime();
    try {
      Forecast f = Forecast.parse(get(url()));
      long took = SystemClock.elapsedRealtime() - t0;
      Log.i(
          TAG,
          "forecast: "
              + BuildConfig.CITY
              + " "
              + f.temperature
              + (FAHRENHEIT ? "F " : "C ")
              + WeatherIcons.text(f.weatherCode)
              + ", "
              + f.hours
              + " h, "
              + f.days
              + " d, in "
              + took
              + " ms");
      post(f, null);
    } catch (SSLHandshakeException e) {
      Log.i(TAG, "fetch failed: tls: " + e.getMessage());
      post(null, "TLS refused");
    } catch (IOException e) {
      Log.i(TAG, "fetch failed: " + e.getMessage());
      post(null, "Offline");
    } catch (JSONException e) {
      Log.i(TAG, "fetch failed: bad reply: " + e.getMessage());
      post(null, "Bad reply");
    }
  }

  /** Certificate validity is checked against the wall clock: anchor it before the first request. */
  private void syncClockOnce() {
    if (clockSynced) {
      return;
    }
    SntpClient client = new SntpClient();
    if (client.requestTime("pool.ntp.org", 3000)) {
      long now = client.getNtpTime() + SystemClock.elapsedRealtime() - client.getNtpTimeReference();
      SystemClock.setCurrentTimeMillis(now);
      Log.i(TAG, "ntp: synced");
      clockSynced = true;
    } else {
      Log.i(TAG, "ntp: no reply (the wall clock may already be set)");
    }
  }

  private static String get(String url) throws IOException {
    HttpURLConnection c = new URL(url).openConnection();
    try {
      c.setConnectTimeout(TIMEOUT_MS);
      c.setReadTimeout(TIMEOUT_MS);
      c.connect();
      int code = c.getResponseCode();
      if (code != HttpURLConnection.HTTP_OK) {
        throw new IOException("HTTP " + code);
      }
      HttpInputStream in = c.getInputStream();
      byte[] buf = new byte[MAX_REPLY_BYTES];
      int total = 0;
      int n;
      while (total < buf.length && (n = in.read(buf, total, buf.length - total)) > 0) {
        total += n;
      }
      return new String(buf, 0, total);
    } finally {
      c.disconnect();
    }
  }

  /** Hands the outcome to the main thread: a forecast, or the status to show without one. */
  private void post(Forecast f, String failure) {
    Executors.mainExecutor()
        .execute(
            () -> {
              busy = false;
              if (f != null) {
                apply(f);
              } else {
                setStatus(failure);
              }
            });
  }

  private void apply(Forecast f) {
    skyFor(f);
    tempView.setText(f.temperature + DEGREE);
    icon.set(f.weatherCode, f.isDay);
    conditionView.setText(WeatherIcons.text(f.weatherCode));
    highLowView.setText(
        f.days > 0 ? "H:" + f.dayHigh[0] + DEGREE + "  L:" + f.dayLow[0] + DEGREE : "");
    hourly.set(f);
    daily.set(f);
    feelsView.setText("Feels like  " + f.feelsLike + DEGREE);
    humidityView.setText("Humidity  " + f.humidity + "%");
    windView.setText("Wind  " + f.windSpeed + (FAHRENHEIT ? " mph" : " km/h"));
    pressureView.setText("Pressure  " + f.pressure + " hPa");
    sunView.setText("Sunrise " + f.sunrise + "   Sunset " + f.sunset);
    updateIndoor();
    setStatus(Forecast.clock(System.currentTimeMillis() / 1000, f.utcOffsetSeconds));
    Log.i(TAG, "updated");
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
        roomTemperature = event.values[0];
        haveReadings = true;
        break;
      case Sensor.TYPE_RELATIVE_HUMIDITY:
        roomHumidity = event.values[0];
        break;
      case Sensor.TYPE_PRESSURE:
        roomPressure = event.values[0];
        break;
      default:
        break;
    }
    updateIndoor();
  }

  @Override
  public void onAccuracyChanged(Sensor sensor, int accuracy) {}

  /** "Indoor 23° • 45% • 1013 hPa", rewritten only when a rounded value moves. */
  private void updateIndoor() {
    if (!haveReadings) {
      return;
    }
    float t = FAHRENHEIT ? roomTemperature * 9 / 5 + 32 : roomTemperature;
    String text =
        "Indoor  "
            + Forecast.round(t)
            + DEGREE
            + " • "
            + Forecast.round(roomHumidity)
            + "% • "
            + Forecast.round(roomPressure)
            + " hPa";
    if (!text.equals(indoorText)) {
      indoorText = text;
      indoorView.setText(text);
    }
  }
}
