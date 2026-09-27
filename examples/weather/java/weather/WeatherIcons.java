// SPDX-License-Identifier: GPL-3.0-only
package weather;

import picodroid.graphics.Canvas;
import picodroid.graphics.Color;
import picodroid.graphics.Paint;

/**
 * The condition glyphs, drawn with {@link Canvas} primitives (circles, rounded rectangles and
 * lines) so one drawing serves the 18 px list rows and the 64 px hero alike, and the WMO 4677
 * weather-code table open-meteo reports, reduced to those glyphs and to a short text.
 *
 * <p>A view's recording holds 2 KB of ops at 32 bytes each (a text op adds its bytes), so a glyph
 * is frugal: the sun's rays are two or four lines through the centre with the disc drawn over them,
 * a small glyph (under 28 px) has a two-op cloud and at most four ops in all, the hero at most six.
 * The hourly strip (six columns, two texts and a glyph each) and the seven-day list (three texts, a
 * bar and a glyph per row) then fit in one view apiece.
 */
final class WeatherIcons {
  static final int SUN = 0;
  static final int MOON = 1;
  static final int PARTLY_SUN = 2;
  static final int PARTLY_MOON = 3;
  static final int CLOUD = 4;
  static final int FOG = 5;
  static final int DRIZZLE = 6;
  static final int RAIN = 7;
  static final int SNOW = 8;
  static final int THUNDER = 9;

  private static final int SUN_COLOR = Color.rgb(255, 204, 60);
  private static final int MOON_COLOR = Color.rgb(238, 236, 214);
  private static final int MOON_SHADE = Color.rgb(205, 202, 176);
  private static final int CLOUD_WHITE = Color.rgb(246, 248, 252);
  private static final int CLOUD_GREY = Color.rgb(205, 212, 224);
  private static final int CLOUD_DARK = Color.rgb(150, 160, 178);
  private static final int DROP = Color.rgb(120, 190, 255);
  private static final int FLAKE = Color.rgb(255, 255, 255);
  private static final int BOLT = Color.rgb(255, 222, 80);
  private static final int MIST = Color.rgb(215, 222, 232);

  /** Half the width of a ray, times the disc radius. */
  private static final float RAY = 1.8f;

  private static final float DIAG = 0.707f * RAY;

  private WeatherIcons() {}

  /** The glyph for a WMO code, by day or by night. */
  static int glyph(int code, boolean day) {
    switch (code) {
      case 0:
        return day ? SUN : MOON;
      case 1:
      case 2:
        return day ? PARTLY_SUN : PARTLY_MOON;
      case 3:
        return CLOUD;
      case 45:
      case 48:
        return FOG;
      case 51:
      case 53:
      case 55:
      case 56:
      case 57:
        return DRIZZLE;
      case 61:
      case 63:
      case 65:
      case 66:
      case 67:
      case 80:
      case 81:
      case 82:
        return RAIN;
      case 71:
      case 73:
      case 75:
      case 77:
      case 85:
      case 86:
        return SNOW;
      case 95:
      case 96:
      case 99:
        return THUNDER;
      default:
        return CLOUD;
    }
  }

  /** The WMO 4677 weather codes open-meteo reports, in its own wording. */
  static String text(int code) {
    switch (code) {
      case 0:
        return "Clear";
      case 1:
        return "Mainly clear";
      case 2:
        return "Partly cloudy";
      case 3:
        return "Overcast";
      case 45:
      case 48:
        return "Fog";
      case 51:
      case 53:
      case 55:
        return "Drizzle";
      case 56:
      case 57:
        return "Freezing drizzle";
      case 61:
      case 63:
      case 65:
        return "Rain";
      case 66:
      case 67:
        return "Freezing rain";
      case 71:
      case 73:
      case 75:
        return "Snow";
      case 77:
        return "Snow grains";
      case 80:
      case 81:
      case 82:
        return "Showers";
      case 85:
      case 86:
        return "Snow showers";
      case 95:
        return "Thunderstorm";
      case 96:
      case 99:
        return "Thunderstorm with hail";
      default:
        return "Code " + code;
    }
  }

  /**
   * Draws the glyph for {@code code} inside an {@code s} by {@code s} box centred on ({@code cx},
   * {@code cy}). Leaves the paint filled, in the last colour used.
   */
  static void draw(Canvas c, Paint p, int code, boolean day, float cx, float cy, float s) {
    boolean small = s < 28;
    p.setStyle(Paint.Style.FILL);
    switch (glyph(code, day)) {
      case SUN:
        sun(c, p, cx, cy, s * 0.30f, small);
        break;
      case MOON:
        moon(c, p, cx, cy, s * 0.30f, small);
        break;
      case PARTLY_SUN:
        if (small) {
          disc(c, p, cx + s * 0.16f, cy - s * 0.16f, s * 0.19f, SUN_COLOR);
        } else {
          sun(c, p, cx + s * 0.16f, cy - s * 0.16f, s * 0.19f, true);
        }
        cloud(c, p, cx - s * 0.04f, cy + s * 0.08f, s * 0.80f, CLOUD_WHITE, small);
        break;
      case PARTLY_MOON:
        moon(c, p, cx + s * 0.16f, cy - s * 0.16f, s * 0.19f, true);
        cloud(c, p, cx - s * 0.04f, cy + s * 0.08f, s * 0.80f, CLOUD_GREY, small);
        break;
      case CLOUD:
        cloud(c, p, cx, cy, s, CLOUD_GREY, small);
        break;
      case FOG:
        cloud(c, p, cx, cy - s * 0.16f, s * 0.80f, CLOUD_GREY, small);
        mist(c, p, cx, cy, s, small);
        break;
      case DRIZZLE:
        cloud(c, p, cx, cy - s * 0.12f, s * 0.80f, CLOUD_GREY, small);
        drops(c, p, cx, cy, s, 0.12f, small);
        break;
      case RAIN:
        cloud(c, p, cx, cy - s * 0.12f, s * 0.80f, CLOUD_DARK, small);
        drops(c, p, cx, cy, s, 0.24f, small);
        break;
      case SNOW:
        cloud(c, p, cx, cy - s * 0.12f, s * 0.80f, CLOUD_WHITE, small);
        flakes(c, p, cx, cy, s, small);
        break;
      case THUNDER:
        cloud(c, p, cx, cy - s * 0.14f, s * 0.80f, CLOUD_DARK, small);
        bolt(c, p, cx, cy, s, small);
        break;
      default:
        break;
    }
  }

  private static void disc(Canvas c, Paint p, float cx, float cy, float r, int color) {
    p.setStyle(Paint.Style.FILL);
    p.setColor(color);
    c.drawCircle(cx, cy, r, p);
  }

  /**
   * A disc of radius {@code r} over lines through its centre: two diagonals (four rays) when small,
   * four lines (eight rays) otherwise. Three or five ops.
   */
  private static void sun(Canvas c, Paint p, float cx, float cy, float r, boolean small) {
    p.setColor(SUN_COLOR);
    p.setStrokeWidth(Math.max(1.5f, r * 0.22f));
    p.setStrokeCap(Paint.Cap.ROUND);
    float d = r * DIAG;
    c.drawLine(cx - d, cy - d, cx + d, cy + d, p);
    c.drawLine(cx - d, cy + d, cx + d, cy - d, p);
    if (!small) {
      float a = r * RAY;
      c.drawLine(cx - a, cy, cx + a, cy, p);
      c.drawLine(cx, cy - a, cx, cy + a, p);
    }
    disc(c, p, cx, cy, r, SUN_COLOR);
  }

  /** A pale disc with one crater, or three. */
  private static void moon(Canvas c, Paint p, float cx, float cy, float r, boolean small) {
    disc(c, p, cx, cy, r, MOON_COLOR);
    p.setColor(MOON_SHADE);
    c.drawCircle(cx - r * 0.30f, cy - r * 0.20f, r * 0.26f, p);
    if (!small) {
      c.drawCircle(cx + r * 0.38f, cy + r * 0.28f, r * 0.15f, p);
      c.drawCircle(cx - r * 0.12f, cy + r * 0.48f, r * 0.12f, p);
    }
  }

  /**
   * A rounded base with one puff, or two, {@code s} wide and about 0.56 {@code s} tall, its base
   * centred at ({@code cx}, {@code cy} + 0.12 {@code s}).
   */
  private static void cloud(
      Canvas c, Paint p, float cx, float cy, float s, int color, boolean small) {
    p.setStyle(Paint.Style.FILL);
    p.setColor(color);
    c.drawRoundRect(cx - s * 0.42f, cy, cx + s * 0.42f, cy + s * 0.24f, s * 0.12f, s * 0.12f, p);
    c.drawCircle(cx + s * 0.08f, cy - s * 0.08f, s * 0.24f, p);
    if (!small) {
      c.drawCircle(cx - s * 0.18f, cy + s * 0.02f, s * 0.18f, p);
    }
  }

  /** Slanted strokes under a cloud, {@code len} (a fraction of {@code s}) long: two or three. */
  private static void drops(
      Canvas c, Paint p, float cx, float cy, float s, float len, boolean small) {
    p.setColor(DROP);
    p.setStrokeWidth(Math.max(1.5f, s * 0.06f));
    p.setStrokeCap(Paint.Cap.ROUND);
    float top = cy + s * 0.20f;
    for (int i = small ? 0 : -1; i <= 1; i++) {
      float x = cx + (small ? i * 2 - 1 : i) * s * 0.20f;
      c.drawLine(x + s * 0.03f, top, x - s * 0.03f, top + s * len, p);
    }
  }

  /** Flakes under a cloud: two or three. */
  private static void flakes(Canvas c, Paint p, float cx, float cy, float s, boolean small) {
    p.setStyle(Paint.Style.FILL);
    p.setColor(FLAKE);
    float r = Math.max(1.5f, s * 0.05f);
    c.drawCircle(cx - s * 0.20f, cy + s * 0.30f, r, p);
    c.drawCircle(cx + s * 0.20f, cy + s * 0.30f, r, p);
    if (!small) {
      c.drawCircle(cx, cy + s * 0.40f, r, p);
    }
  }

  /** A lightning bolt under a cloud: two strokes, or three. */
  private static void bolt(Canvas c, Paint p, float cx, float cy, float s, boolean small) {
    p.setColor(BOLT);
    p.setStrokeWidth(Math.max(1.5f, s * 0.07f));
    p.setStrokeCap(Paint.Cap.ROUND);
    c.drawLine(cx + s * 0.08f, cy + s * 0.12f, cx - s * 0.06f, cy + s * 0.30f, p);
    if (small) {
      c.drawLine(cx - s * 0.06f, cy + s * 0.30f, cx + s * 0.04f, cy + s * 0.48f, p);
      return;
    }
    c.drawLine(cx - s * 0.06f, cy + s * 0.30f, cx + s * 0.06f, cy + s * 0.30f, p);
    c.drawLine(cx + s * 0.06f, cy + s * 0.30f, cx - s * 0.08f, cy + s * 0.48f, p);
  }

  /** Bands of mist under a cloud, two or three, the lower ones shorter. */
  private static void mist(Canvas c, Paint p, float cx, float cy, float s, boolean small) {
    p.setColor(MIST);
    p.setStrokeWidth(Math.max(1.5f, s * 0.06f));
    p.setStrokeCap(Paint.Cap.ROUND);
    c.drawLine(cx - s * 0.34f, cy + s * 0.16f, cx + s * 0.34f, cy + s * 0.16f, p);
    c.drawLine(cx - s * 0.22f, cy + s * 0.29f, cx + s * 0.28f, cy + s * 0.29f, p);
    if (!small) {
      c.drawLine(cx - s * 0.34f, cy + s * 0.42f, cx + s * 0.18f, cy + s * 0.42f, p);
    }
  }
}
