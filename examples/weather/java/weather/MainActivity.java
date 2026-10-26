// SPDX-License-Identifier: GPL-3.0-only
package weather;

import java.io.IOException;
import java.io.InputStream;
import javax.net.ssl.SSLHandshakeException;
import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.TimeUnit;
import picodroid.content.Context;
import picodroid.graphics.Color;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.hardware.Sensor;
import picodroid.hardware.SensorEvent;
import picodroid.hardware.SensorEventListener;
import picodroid.hardware.SensorManager;
import picodroid.json.JSONException;
import picodroid.net.ConnectivityManager;
import picodroid.net.HttpURLConnection;
import picodroid.net.Network;
import picodroid.net.URL;
import picodroid.os.Bundle;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.view.Gravity;
import picodroid.view.KeyEvent;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * A weather app in the shape of the phone ones. Today: the place, the temperature large with the
 * condition beside it, the high and low, and the next hours in a strip. The week: one row per day
 * with a bar spanning the day's range across the week's. Details: feels-like, humidity, wind,
 * pressure, sunrise and sunset, and on a board with the Enviro+ sensors the room's own readings.
 * The sky behind it all follows the conditions and the time of day.
 *
 * <p>Everything comes from one open-meteo request over HTTPS (docs/designs/tls-2026-09.md) when the
 * network comes up and every {@link #REFRESH_MINUTES} minutes after, on a thread of its own; the
 * handshake needs the wall clock, which the platform's time service anchors
 * (docs/designs/time-service-2026-10.md), so the app never sets it. Four buttons: A and B turn the
 * page, X refreshes now, Y leaves.
 *
 * <p>The place and the units are build-time constants ({@code picodroidBuildConfig} in
 * build.gradle.kts): {@code PICODROID_WEATHER_CITY=Tokyo PICODROID_WEATHER_LATITUDE=35.68
 * PICODROID_WEATHER_LONGITUDE=139.69 ./scripts/flash.sh --app weather --board pico_enviro_mon_w};
 * {@code PICODROID_WEATHER_UNITS=fahrenheit} for degrees Fahrenheit and mph.
 */
public class MainActivity extends Activity implements SensorEventListener {
  private static final String TAG = "Weather";

  private static final int TIMEOUT_MS = 10000;
  private static final int MAX_REPLY_BYTES = 4096;
  private static final int REFRESH_MINUTES = 15;
  private static final int PAGES = 3;
  private static final int PAGE_TODAY = 0;
  private static final int PAGE_WEEK = 1;
  private static final int PAGE_DETAILS = 2;
  private static final int PAD = 8;
  private static final int HEADER = 24;
  private static final int HERO_ICON = 64;

  /** Space between the lines of a page; the details page spaces its lines wider. */
  private static final int GAP = 2;

  private static final int DETAILS_GAP = 6;

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
  private ScheduledExecutorService timer;
  private ConnectivityManager connectivity;
  private SensorManager sensors;

  /**
   * Fetches when the network comes up, at start and after every drop; says so when it goes. The
   * framework calls it on the main thread.
   */
  private final ConnectivityManager.NetworkCallback networkCallback =
      new ConnectivityManager.NetworkCallback() {
        @Override
        public void onAvailable(Network network) {
          refresh();
        }

        @Override
        public void onLost(Network network) {
          setStatus(getString(R.string.status_no_network));
        }
      };

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
    root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    // A weather display is glanced at, not touched: hold the panel on.
    root.setKeepScreenOn(true);
    root.setPadding(PAD, 6, PAD, 6);
    sky(NIGHT_CLEAR_TOP, NIGHT_CLEAR_BOTTOM);

    buildToday();
    buildWeek();
    buildDetails();
    // One page shows at a time and takes all the height the hints leave.
    for (int i = 0; i < PAGES; i++) {
      root.addView(
          pages[i], new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
    }

    TextView hints = new TextView(this);
    hints.setText(getString(R.string.hints));
    hints.setTextColor(TEXT_DIM);
    hints.setSingleLine();
    root.addView(hints, below(GAP, ViewGroup.LayoutParams.WRAP_CONTENT));

    setContentView(root);
    showPage(PAGE_TODAY);
    startSensors();
    Log.i(TAG, "ready");

    setStatus(getString(R.string.status_connecting));
    connectivity = (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
    connectivity.registerDefaultNetworkCallback(networkCallback);
    timer = Executors.mainScheduledExecutor();
    timer.scheduleAtFixedRate(() -> refresh(), REFRESH_MINUTES, REFRESH_MINUTES, TimeUnit.MINUTES);
  }

  @Override
  public void onDestroy() {
    if (timer != null) {
      timer.shutdownNow();
      timer = null;
    }
    connectivity.unregisterNetworkCallback(networkCallback);
    sensors.unregisterListener(this);
    super.onDestroy();
  }

  // ── The pages ──────────────────────────────────────────────────────────

  /** A view's place in a column: {@code gap} below the one above it. */
  private static LinearLayout.LayoutParams below(int gap, int width) {
    LinearLayout.LayoutParams lp =
        new LinearLayout.LayoutParams(width, ViewGroup.LayoutParams.WRAP_CONTENT);
    lp.topMargin = gap;
    return lp;
  }

  /** A page: a header row (title left, status right) over its content, hidden until shown. */
  private LinearLayout newPage(int index, String title) {
    LinearLayout p = new LinearLayout(this);
    p.setOrientation(LinearLayout.VERTICAL);

    LinearLayout header = new LinearLayout(this);
    header.setOrientation(LinearLayout.HORIZONTAL);
    header.setGravity(Gravity.CENTER_VERTICAL);
    TextView t = new TextView(this);
    t.setText(title);
    t.setTextSize(20);
    t.setTextColor(TEXT);
    t.setSingleLine();
    header.addView(t, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
    TextView s = new TextView(this);
    s.setTextColor(TEXT_DIM);
    s.setSingleLine();
    header.addView(s);
    p.addView(header, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, HEADER));

    statusViews[index] = s;
    pages[index] = p;
    return p;
  }

  private void buildToday() {
    LinearLayout p = newPage(PAGE_TODAY, BuildConfig.CITY);

    // The temperature takes the row beside the icon; the row is as tall as the taller of the two.
    LinearLayout hero = new LinearLayout(this);
    hero.setOrientation(LinearLayout.HORIZONTAL);
    hero.setGravity(Gravity.CENTER_VERTICAL);
    tempView = new TextView(this);
    tempView.setText(getString(R.string.temperature_unknown));
    tempView.setTextSize(64);
    tempView.setIncludeFontPadding(false);
    tempView.setTextColor(TEXT);
    tempView.setSingleLine();
    hero.addView(
        tempView, new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f));
    icon = new ConditionIcon(this, HERO_ICON);
    // wrap_content: the icon says how large it is (ConditionIcon.onMeasure).
    hero.addView(
        icon,
        new LinearLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT));
    p.addView(hero, below(GAP, ViewGroup.LayoutParams.MATCH_PARENT));

    conditionView = new TextView(this);
    conditionView.setText(getString(R.string.waiting));
    conditionView.setTextColor(TEXT);
    conditionView.setSingleLine();
    p.addView(conditionView, below(GAP, ViewGroup.LayoutParams.WRAP_CONTENT));

    highLowView = new TextView(this);
    highLowView.setTextColor(TEXT_DIM);
    highLowView.setSingleLine();
    p.addView(highLowView, below(GAP, ViewGroup.LayoutParams.WRAP_CONTENT));

    hourly = new HourlyStrip(this);
    p.addView(hourly, below(GAP, ViewGroup.LayoutParams.MATCH_PARENT));
  }

  private void buildWeek() {
    LinearLayout p = newPage(PAGE_WEEK, getString(R.string.page_week));
    daily = new DailyList(this);
    p.addView(daily, below(GAP, ViewGroup.LayoutParams.MATCH_PARENT));
  }

  private void buildDetails() {
    LinearLayout p = newPage(PAGE_DETAILS, getString(R.string.page_details));
    p.setGravity(Gravity.LEFT);
    feelsView = detailLine(p);
    humidityView = detailLine(p);
    windView = detailLine(p);
    pressureView = detailLine(p);
    sunView = detailLine(p);
    indoorView = detailLine(p);
    indoorView.setText(getString(R.string.indoor_none));
  }

  private TextView detailLine(LinearLayout page) {
    TextView v = new TextView(this);
    v.setTextColor(TEXT);
    v.setSingleLine();
    page.addView(v, below(DETAILS_GAP, ViewGroup.LayoutParams.WRAP_CONTENT));
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
        new GradientDrawable(GradientDrawable.Orientation.TOP_BOTTOM, new int[] {top, bottom}));
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

  /**
   * One request in flight at a time, and none without a network (the callback fetches when one
   * comes up); the network work runs on its own thread.
   */
  private void refresh() {
    if (busy) {
      return;
    }
    if (connectivity.getActiveNetwork() == null) {
      Log.i(TAG, "fetch failed: no network");
      setStatus(getString(R.string.status_no_network));
      return;
    }
    busy = true;
    setStatus(getString(R.string.status_updating));
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
      deliver(f, 0);
    } catch (SSLHandshakeException e) {
      Log.i(TAG, "fetch failed: tls: " + e.getMessage());
      deliver(null, R.string.status_tls_refused);
    } catch (IOException e) {
      Log.i(TAG, "fetch failed: " + e.getMessage());
      deliver(null, R.string.status_offline);
    } catch (JSONException e) {
      Log.i(TAG, "fetch failed: bad reply: " + e.getMessage());
      deliver(null, R.string.status_bad_reply);
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
      InputStream in = c.getInputStream();
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
  private void deliver(Forecast f, int failure) {
    getMainExecutor()
        .execute(
            () -> {
              busy = false;
              if (f != null) {
                apply(f);
              } else {
                setStatus(getString(failure));
              }
            });
  }

  private void apply(Forecast f) {
    skyFor(f);
    tempView.setText(getString(R.string.temperature, f.temperature));
    icon.set(f.weatherCode, f.isDay);
    conditionView.setText(WeatherIcons.text(f.weatherCode));
    highLowView.setText(f.days > 0 ? getString(R.string.high_low, f.dayHigh[0], f.dayLow[0]) : "");
    hourly.set(f);
    daily.set(f);
    feelsView.setText(getString(R.string.feels_like, f.feelsLike));
    humidityView.setText(getString(R.string.humidity, f.humidity));
    windView.setText(getString(FAHRENHEIT ? R.string.wind_mph : R.string.wind_kmh, f.windSpeed));
    pressureView.setText(getString(R.string.pressure, f.pressure));
    sunView.setText(getString(R.string.sun, f.sunrise, f.sunset));
    updateIndoor();
    setStatus(Forecast.clock(System.currentTimeMillis() / 1000, f.utcOffsetSeconds));
    Log.i(TAG, "updated");
  }

  // ── Room readings, on a board that has the sensors ──────────────────────

  private void startSensors() {
    sensors = (SensorManager) getSystemService(Context.SENSOR_SERVICE);
    SensorManager sm = sensors;
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
        getString(
            R.string.indoor,
            Forecast.round(t),
            Forecast.round(roomHumidity),
            Forecast.round(roomPressure));
    if (!text.equals(indoorText)) {
      indoorText = text;
      indoorView.setText(text);
    }
  }
}
