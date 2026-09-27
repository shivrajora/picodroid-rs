// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

import claudeusage.proto.ModelCap;
import claudeusage.proto.ModelShare;
import claudeusage.proto.Today;
import claudeusage.proto.UsageReply;
import claudeusage.proto.Window;
import java.io.IOException;
import java.net.ConnectException;
import java.net.SocketTimeoutException;
import picodroid.net.HttpInputStream;
import picodroid.net.HttpURLConnection;
import picodroid.net.URL;
import picodroid.protobuf.InvalidProtocolBufferException;
import picodroid.util.Log;

/**
 * One blocking GET of the bridge's {@code /u}, asked for as protobuf ({@code Accept:
 * application/x-protobuf}; the schema is {@code proto/usage.proto}, the classes under {@code
 * claudeusage.proto} are generated from it). Every failure maps onto a {@link LinkState} so the UI
 * can say what is wrong rather than just "offline".
 */
public final class UsageFetcher {
  private static final String TAG = UsageService.TAG;

  public static final int PORT = 8787;

  /**
   * No timeout means forever on this platform. Short on purpose: a PC that is switched off answers
   * nothing at all, and this is how long each attempt then blocks.
   */
  private static final int TIMEOUT_MS = 4000;

  static final String ACCEPT = "application/x-protobuf";

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
      conn.setRequestProperty("Accept", ACCEPT);
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
      if (!parse(UsageReply.parseFrom(BUF, 0, total), out)) {
        return LinkState.BAD_DATA;
      }
      return out.ok ? LinkState.OK : LinkState.UPSTREAM;
    } catch (InvalidProtocolBufferException e) {
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

  /**
   * Copies {@code r} into {@code out} with the UI's clamps and caps. False when the payload is not
   * one this app understands (another protocol version).
   */
  static boolean parse(UsageReply r, UsageSnapshot out) {
    if (r.getVersion() != 1) {
      Log.i(TAG, "fetch: unknown payload version " + r.getVersion());
      return false;
    }
    out.bridgeEpochS = r.getBridgeTime();
    out.tzMinutes = r.getTzMinutes();
    out.ok = r.getOk();
    out.err = r.getErr();
    out.ageS = r.getAgeS();
    out.plan = r.getPlan();

    if (r.hasSession()) {
      Window s = r.getSession();
      out.sessionPct = clampPct(s.getPct());
      out.sessionReset = s.getReset();
    }
    if (r.hasWeekly()) {
      Window w = r.getWeekly();
      out.weeklyPct = clampPct(w.getPct());
      out.weeklyReset = w.getReset();
    }
    for (int i = 0; i < r.getModelCapsCount() && out.modelCount < UsageSnapshot.MAX_MODELS; i++) {
      ModelCap row = r.getModelCaps(i);
      int k = out.modelCount++;
      out.modelName[k] = row.getName();
      out.modelPct[k] = clampPct(row.getPct());
      out.modelReset[k] = row.getReset();
    }
    out.ratePerHour = r.getRatePerHour();
    out.etaMinutes = r.getEtaMinutes();

    if (r.hasToday()) {
      Today td = r.getToday();
      out.todayTokensK = td.getTokensK();
      out.todayCents = td.getCents();
      out.todayMessages = td.getMessages();
    }
    if (r.getDayTokensKCount() == UsageSnapshot.DAYS) {
      out.hasHistory = true;
      for (int i = 0; i < UsageSnapshot.DAYS; i++) {
        int v = r.getDayTokensK(i);
        out.dayTokensK[i] = v < 0 ? 0 : v;
      }
      out.dayLetters = r.getDayLetters();
    }
    for (int i = 0; i < r.getMixCount() && out.mixCount < UsageSnapshot.MAX_MODELS; i++) {
      ModelShare row = r.getMix(i);
      int k = out.mixCount++;
      out.mixName[k] = row.getName();
      out.mixPct[k] = clampPct(row.getPct());
    }
    return true;
  }

  private static int clampPct(int v) {
    return v < 0 ? -1 : (v > 100 ? 100 : v);
  }
}
