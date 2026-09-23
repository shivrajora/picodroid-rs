// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

import java.io.IOException;
import java.net.ConnectException;
import java.net.SocketTimeoutException;
import picodroid.json.JSONArray;
import picodroid.json.JSONException;
import picodroid.json.JSONObject;
import picodroid.net.HttpInputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.URL;
import picodroid.util.Log;

/**
 * One blocking GET of the bridge's {@code /u}. Every failure maps onto a {@link LinkState} so the
 * UI can say what is wrong rather than just "offline".
 */
public final class UsageFetcher {
  private static final String TAG = UsageService.TAG;

  public static final int PORT = 8787;

  /**
   * No timeout means forever on this platform. Short on purpose: a PC that is switched off answers
   * nothing at all, and this is how long each attempt then blocks.
   */
  private static final int TIMEOUT_MS = 4000;

  /** The bridge caps its reply at 700 bytes; anything that fills this buffer is not ours. */
  private static final int MAX_REPLY_BYTES = 1024;

  /**
   * Shared by construction: one fetch runs at a time, on the poll thread. A fresh buffer per poll
   * would be churn the RP2350 heap does not need.
   */
  private static final byte[] BUF = new byte[MAX_REPLY_BYTES];

  private UsageFetcher() {}

  /** Fills {@code out} and returns {@link LinkState#OK} or {@link LinkState#UPSTREAM}, else why. */
  static LinkState fetch(String url, UsageSnapshot out) {
    HttpURLConnection conn = null;
    try {
      conn = new URL(url).openConnection();
      conn.setConnectTimeout(TIMEOUT_MS);
      conn.setReadTimeout(TIMEOUT_MS);
      conn.connect();
      int code = conn.getResponseCode();
      if (code != 200) {
        Log.i(TAG, "fetch: HTTP " + code);
        return LinkState.BAD_DATA;
      }
      HttpInputStream in = conn.getInputStream();
      int total = 0;
      while (total < BUF.length) {
        int n = in.read(BUF, total, BUF.length - total);
        if (n < 0) {
          break;
        }
        total += n;
      }
      if (total == 0 || total == BUF.length) {
        Log.i(TAG, "fetch: reply of " + total + " bytes");
        return LinkState.BAD_DATA;
      }
      parse(new String(BUF, 0, total), out);
      return out.ok ? LinkState.OK : LinkState.UPSTREAM;
    } catch (JSONException e) {
      Log.i(TAG, "fetch: bad reply: " + e.getMessage());
      return LinkState.BAD_DATA;
    } catch (ConnectException e) {
      // Something answered and refused: the PC is up, the bridge is not running.
      Log.i(TAG, "fetch: refused: " + e.getMessage());
      return LinkState.BRIDGE_DOWN;
    } catch (SocketTimeoutException e) {
      String msg = e.getMessage();
      Log.i(TAG, "fetch: timeout: " + msg);
      // A connect that times out is a host that is not there: off, asleep or the wrong address.
      return msg != null && msg.startsWith("connect") ? LinkState.PC_OFF : LinkState.NO_REPLY;
    } catch (IOException e) {
      Log.i(TAG, "fetch: failed: " + e);
      return LinkState.PC_OFF;
    } catch (RuntimeException e) {
      Log.i(TAG, "fetch: unexpected: " + e);
      return LinkState.BAD_DATA;
    } finally {
      // 16 HTTP handles exist in total; a leak per retry would exhaust them within minutes.
      if (conn != null) {
        conn.disconnect();
      }
    }
  }

  static void parse(String json, UsageSnapshot out) throws JSONException {
    JSONObject o = new JSONObject(json);
    if (o.optInt("v", 0) != 1) {
      throw new JSONException("unknown payload version");
    }
    out.bridgeEpochS = o.optLong("t", 0L);
    out.tzMinutes = o.optInt("tz", 0);
    out.ok = o.optInt("ok", 0) == 1;
    out.err = o.optString("err", "");
    out.ageS = o.optInt("age", -1);
    out.plan = o.optString("plan", "");

    JSONObject s = o.optJSONObject("s");
    if (s != null) {
      out.sessionPct = clampPct(s.optInt("p", -1));
      out.sessionReset = s.optLong("r", 0L);
    }
    JSONObject w = o.optJSONObject("w");
    if (w != null) {
      out.weeklyPct = clampPct(w.optInt("p", -1));
      out.weeklyReset = w.optLong("r", 0L);
    }
    JSONArray wm = o.optJSONArray("wm");
    if (wm != null) {
      for (int i = 0; i < wm.length() && out.modelCount < UsageSnapshot.MAX_MODELS; i++) {
        JSONArray row = wm.optJSONArray(i);
        if (row == null || row.length() < 2) {
          continue;
        }
        int k = out.modelCount++;
        out.modelName[k] = row.optString(0, "?");
        out.modelPct[k] = clampPct(row.optInt(1, 0));
        out.modelReset[k] = row.optLong(2, 0L);
      }
    }
    out.ratePerHour = o.optInt("rate", 0);
    out.etaMinutes = o.optInt("eta", -1);

    JSONObject td = o.optJSONObject("td");
    if (td != null) {
      out.todayTokensK = td.optInt("tok", 0);
      out.todayCents = td.optInt("usd", 0);
      out.todayMessages = td.optInt("msg", 0);
    }
    JSONArray d7 = o.optJSONArray("d7");
    if (d7 != null && d7.length() == UsageSnapshot.DAYS) {
      out.hasHistory = true;
      for (int i = 0; i < UsageSnapshot.DAYS; i++) {
        int v = d7.optInt(i, 0);
        out.dayTokensK[i] = v < 0 ? 0 : v;
      }
      out.dayLetters = o.optString("dl", "");
    }
    JSONArray mix = o.optJSONArray("mix");
    if (mix != null) {
      for (int i = 0; i < mix.length() && out.mixCount < UsageSnapshot.MAX_MODELS; i++) {
        JSONArray row = mix.optJSONArray(i);
        if (row == null || row.length() < 2) {
          continue;
        }
        int k = out.mixCount++;
        out.mixName[k] = row.optString(0, "?");
        out.mixPct[k] = clampPct(row.optInt(1, 0));
      }
    }
  }

  private static int clampPct(int v) {
    return v < 0 ? -1 : (v > 100 ? 100 : v);
  }
}
