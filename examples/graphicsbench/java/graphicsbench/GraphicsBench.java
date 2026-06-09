// SPDX-License-Identifier: GPL-3.0-only
package graphicsbench;

import picodroid.app.Application;
import picodroid.graphics.Color;
import picodroid.graphics.Display;
import picodroid.os.Runtime;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.FrameLayout;
import picodroid.widget.TextView;

/**
 * Graphics-pipeline benchmark — the rendering counterpart to {@code perfbench} (which covers CPU
 * and memory only). Drives the LVGL render pipeline through four workload groups and prints one
 * composite SCORE (lower is better) so graphics optimisations can be tracked across commits — grep
 * for {@code "^.GraphicsBench. SCORE"} in sim output.
 *
 * <p>This is an {@link Application}, not an Activity: {@code run_application} calls {@code
 * onCreate()} synchronously with no async LVGL tick thread and no event loop, so the benchmark
 * self-pumps frames via {@link Display#update()} (one call = advance the LVGL clock 16 ms +
 * rasterise dirty regions + advance property animations one frame) with zero concurrency, fully
 * deterministically, and exits cleanly when {@code onCreate()} returns.
 *
 * <p>Two invariants make the numbers meaningful:
 *
 * <ul>
 *   <li><b>Every timed frame must mutate a visible property</b> before calling {@code update()}.
 *       {@code lv_timer_handler} only rasterises <i>invalidated</i> regions, so an {@code update()}
 *       after no mutation renders nothing and measures only dispatch overhead.
 *   <li><b>The peak-heap column reflects Java {@code View}-wrapper churn only.</b> Native LVGL
 *       widget and pixel-buffer memory lives off the JVM heap that {@link Runtime#usedMemory}
 *       measures, so {@code peak +<kb>} is visible for the create/destroy test and ~0 for the
 *       others. Wall time dominates the score for render-bound tests; the formula matches
 *       perfbench's.
 * </ul>
 *
 * <p>Score formula per test (identical to perfbench):
 *
 * <pre>{@code
 * score = wall_ms + W_GC * gc_cycles + (peak_kb / W_PEAK_DIV)
 * }</pre>
 */
public class GraphicsBench extends Application {
  private static final String TAG = "GraphicsBench";

  // ── scoring weights (identical to perfbench so SCOREs stay comparable in spirit) ──────────────
  private static final long W_GC = 1L; // each GC cycle adds 1 ms-equivalent
  private static final long W_PEAK_DIV = 10L; // 10 KB of peak heap ≈ 1 ms-equivalent

  // ── frame cadence ─────────────────────────────────────────────────────────────────────────────
  private static final int FRAME_MS = 16; // matches Display.update()'s fixed tick step

  // ── workload constants (all tunable; chosen for ~a few seconds total in sim) ───────────────────
  private static final int A_WIDGETS = 40; // (A) TextViews created per round
  private static final int A_ROUNDS = 20; // (A) create/destroy cycles

  private static final int B_WIDGETS = 16; // (B) persistent widget set
  private static final int B_ITERS = 240; // (B) mutation iterations
  private static final int B_FRAME_EVERY = 4; // (B) render every Nth mutation

  // (C) Each tile animates X + ALPHA = 2 of the 16 animation slots, so C_TILES is hard-capped at 8.
  private static final int C_TILES = 8; // (C) concurrently animated tiles
  private static final int C_DURATION_MS = 1600; // (C) animation duration
  private static final int C_FRAMES = C_DURATION_MS / FRAME_MS; // (C) deterministic frame count

  private static final int D_BLOCK = 24; // (D) TextViews re-texted each frame
  private static final int D_FRAMES = 120; // (D) full-screen redraw frames

  // ── persistent UI (one root for the whole run; stage is cleared between workloads) ─────────────
  private Display display;
  private FrameLayout root;
  private FrameLayout stage;

  // Sinks to defeat dead-code elimination.
  static int sinkInt;
  static Object sinkObj;

  @Override
  public void onCreate() {
    // run_application does not pre-init the display, so bring up LVGL first. Idempotent.
    display = Display.getInstance();

    root = new FrameLayout();
    root.setSize(display.getWidth(), display.getHeight());
    root.setBackgroundColor(Color.BLACK);
    stage = new FrameLayout();
    stage.setSize(display.getWidth(), display.getHeight());
    root.addView(stage);
    display.setContentView(root);
    display.update(); // first paint — clean baseline on screen

    Log.i(TAG, "=== graphicsbench (LVGL render pipeline composite score) ===");

    Log.i(TAG, "--- widget lifecycle group ---");
    long lifeScore = 0L;
    lifeScore += runTest("widget_churn", () -> benchWidgetChurn());

    Log.i(TAG, "--- property bridge group ---");
    long propScore = 0L;
    propScore += runTest("prop_throughput", () -> benchPropThroughput());

    Log.i(TAG, "--- animation group ---");
    long animScore = 0L;
    animScore += runFrameTest("anim_fps", C_FRAMES, () -> benchAnimation());

    Log.i(TAG, "--- text & fill group ---");
    long fillScore = 0L;
    fillScore += runFrameTest("text_fill", D_FRAMES, () -> benchTextFill());

    long total = lifeScore + propScore + animScore + fillScore;
    Log.i(
        TAG,
        "SUBSCORE lifecycle="
            + lifeScore
            + " property="
            + propScore
            + " animation="
            + animScore
            + " textfill="
            + fillScore);
    Log.i(TAG, "SCORE " + total);
    Log.i(TAG, "=== PASSED ===");
  }

  /**
   * Instruments {@code t.run()} with timing, GC counters, and peak-heap delta, prints a one-line
   * per-test report, and returns the composite per-test score. Copied verbatim from {@code
   * PerfBench.runTest} so the scoring stays identical.
   */
  static long runTest(String name, TestCase t) {
    Runtime.resetGcStats();
    long usedBefore = Runtime.usedMemory();
    Runtime.resetPeakMemory();

    long startNs = SystemClock.elapsedRealtimeNanos();
    t.run();
    long wallNs = SystemClock.elapsedRealtimeNanos() - startNs;

    int gcCount = Runtime.gcCount();
    long gcTimeNs = Runtime.gcTimeNanos();
    long peakBytes = Runtime.peakMemory() - usedBefore;
    if (peakBytes < 0L) {
      peakBytes = 0L;
    }
    long peakKb = peakBytes / 1024L;
    long wallMs = wallNs / 1000000L;
    long gcTimeMs = gcTimeNs / 1000000L;

    long score = wallMs + (W_GC * (long) gcCount) + (peakKb / W_PEAK_DIV);
    Log.i(
        TAG,
        name
            + ": wall "
            + String.valueOf(wallMs)
            + " ms (gc "
            + String.valueOf(gcTimeMs)
            + " ms / "
            + String.valueOf(gcCount)
            + " cyc), peak +"
            + String.valueOf(peakKb)
            + " KB -> score "
            + String.valueOf(score));
    return score;
  }

  /**
   * Frame-pumped variant of {@link #runTest}: same instrumentation and identical score formula, but
   * the caller passes the number of frames {@code t.run()} renders so a derived rate can be
   * appended. The reported value is <b>render throughput</b> (frames the engine actually rasterised
   * per wall-second), not a real-time framerate — {@code update()} advances a fixed 16 ms tick, so
   * the frame count is deterministic and only the wall time varies with raster cost.
   */
  static long runFrameTest(String name, int frames, TestCase t) {
    Runtime.resetGcStats();
    long usedBefore = Runtime.usedMemory();
    Runtime.resetPeakMemory();

    long startNs = SystemClock.elapsedRealtimeNanos();
    t.run();
    long wallNs = SystemClock.elapsedRealtimeNanos() - startNs;

    int gcCount = Runtime.gcCount();
    long gcTimeNs = Runtime.gcTimeNanos();
    long peakBytes = Runtime.peakMemory() - usedBefore;
    if (peakBytes < 0L) {
      peakBytes = 0L;
    }
    long peakKb = peakBytes / 1024L;
    long wallMs = wallNs / 1000000L;
    long gcTimeMs = gcTimeNs / 1000000L;

    long score = wallMs + (W_GC * (long) gcCount) + (peakKb / W_PEAK_DIV);
    long fps = (wallMs > 0L) ? ((long) frames * 1000L / wallMs) : 0L;
    Log.i(
        TAG,
        name
            + ": wall "
            + String.valueOf(wallMs)
            + " ms (gc "
            + String.valueOf(gcTimeMs)
            + " ms / "
            + String.valueOf(gcCount)
            + " cyc), peak +"
            + String.valueOf(peakKb)
            + " KB -> score "
            + String.valueOf(score)
            + " ("
            + String.valueOf(frames)
            + " frames, "
            + String.valueOf(fps)
            + " fps)");
    return score;
  }

  /** Empty the stage and flush the now-cleared screen, so each test starts from a clean slate. */
  private void clearStage() {
    stage.removeAllViews();
    display.update();
  }

  // ════════════════════════════════════════════════════════════════════════════════════════════
  // (A) Widget lifecycle — mass create + render + bulk destroy.
  // ════════════════════════════════════════════════════════════════════════════════════════════

  private void benchWidgetChurn() {
    for (int round = 0; round < A_ROUNDS; round++) {
      for (int i = 0; i < A_WIDGETS; i++) {
        TextView tv = new TextView();
        tv.setText("w" + i);
        tv.setTextColor(Color.WHITE);
        tv.setPosition((i % 8) * 40, (i / 8) * 24); // spread so each invalidates a distinct region
        stage.addView(tv);
        sinkObj = tv;
      }
      display.update(); // rasterise the freshly added tree
      stage.removeAllViews(); // destroy native widgets + Java wrappers
      display.update(); // repaint the freed area
    }
    clearStage();
  }

  // ════════════════════════════════════════════════════════════════════════════════════════════
  // (B) Property / bridge throughput — high-volume setters on a fixed widget set.
  // ════════════════════════════════════════════════════════════════════════════════════════════

  private void benchPropThroughput() {
    TextView[] tvs = new TextView[B_WIDGETS];
    for (int i = 0; i < B_WIDGETS; i++) {
      TextView tv = new TextView();
      tv.setText("p" + i);
      tv.setPosition((i % 4) * 70, (i / 4) * 50);
      stage.addView(tv);
      tvs[i] = tv;
    }
    display.update();
    for (int it = 0; it < B_ITERS; it++) {
      TextView tv = tvs[it % B_WIDGETS];
      tv.setText("v" + it);
      tv.setBackgroundColor(Color.argb(255, it & 0xFF, (it * 3) & 0xFF, (it * 7) & 0xFF));
      tv.setPosition((it % 4) * 70 + (it & 7), (it / 4 % 4) * 50);
      tv.setAlpha((float) ((it % 8) + 1) / 8.0f);
      if (it % B_FRAME_EVERY == 0) {
        display.update(); // amortised render: bridge-dispatch-dominated, with real raster mixed in
      }
    }
    display.update();
    clearStage();
  }

  // ════════════════════════════════════════════════════════════════════════════════════════════
  // (C) Animation / FPS — concurrent property animations, deterministic frame pump.
  // ════════════════════════════════════════════════════════════════════════════════════════════

  private void benchAnimation() {
    int w = display.getWidth();
    View[] tiles = new View[C_TILES];
    for (int i = 0; i < C_TILES; i++) {
      FrameLayout tile =
          new FrameLayout(); // FrameLayout: LinearLayout would re-layout + clobber pos
      tile.setSize(28, 28);
      tile.setBackgroundColor(Color.rgb(40 + i * 24, 120, 200));
      tile.setPosition(4, i * 26);
      stage.addView(tile);
      tiles[i] = tile;
    }
    display.update();
    // 8 tiles × {X, ALPHA} = 16 slots, exactly the animation-slot cap — none dropped.
    for (int i = 0; i < C_TILES; i++) {
      tiles[i].animate().x(4, w - 32).alpha(1.0f, 0.2f).setDuration(C_DURATION_MS).start();
    }
    // Pump exactly duration/16 frames; each tick advances every slot 16 ms + renders.
    for (int f = 0; f < C_FRAMES; f++) {
      display.update();
    }
    clearStage();
  }

  // ════════════════════════════════════════════════════════════════════════════════════════════
  // (D) Text & fill / redraw — full-screen background flips + label re-text each frame.
  // ════════════════════════════════════════════════════════════════════════════════════════════

  private void benchTextFill() {
    TextView[] block = new TextView[D_BLOCK];
    for (int i = 0; i < D_BLOCK; i++) {
      TextView tv = new TextView();
      tv.setText("row" + i);
      tv.setTextColor(Color.WHITE);
      tv.setPosition((i % 4) * 78, (i / 4) * 32);
      stage.addView(tv);
      block[i] = tv;
    }
    display.update();
    for (int f = 0; f < D_FRAMES; f++) {
      // Fill: flip the full-screen background → invalidates the whole screen → full raster + flush.
      int c = ((f & 1) == 0) ? Color.rgb(10, 10, 40) : Color.rgb(40, 10, 10);
      root.setBackgroundColor(c);
      // Text: re-text a block of labels → glyph raster on top of the fill.
      for (int i = 0; i < D_BLOCK; i++) {
        block[i].setText("f" + f + "_" + i);
      }
      display.update();
    }
    clearStage();
  }
}
