// SPDX-License-Identifier: GPL-3.0-only
package pagerdemo;

import picodroid.app.Activity;
import picodroid.app.Fragment;
import picodroid.app.FragmentManager;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.TimeUnit;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.FragmentStateAdapter;
import picodroid.widget.FrameLayout;
import picodroid.widget.ViewPager2;

/**
 * Three fragments in a {@code <ViewPager2>} from XML with a dots row driven by {@code
 * OnPageChangeCallback}, the Android shape. The script, advanced from the callbacks: a smooth turn
 * (SETTLING, selected, the old page destroyed, scrolled, IDLE), instant turns, a page's saved state
 * coming back with it, then {@code recreate()} and a covering Activity under "don't keep
 * activities", after each of which the pager comes back on the same page with every page's state.
 * Last, two swipes from test.ctrl (or a finger on a touch board): left turns to page 1, right back
 * to page 0.
 */
public class PagerDemoActivity extends Activity {
  static final String TAG = "PagerDemo";
  static final int PAGES = 3;
  static final int[] lastVisits = new int[PAGES];
  static final int[] destroyed = new int[PAGES];

  private static final int DOT_ON = 0xFFFFB300;
  private static final int DOT_OFF = 0xFF404040;

  /** The script's position; static so it survives the Activity's re-creation. */
  private static int step;

  private static int failures;
  private static boolean sawSettling;
  private static int scrolled;
  private static boolean scrolledBad;

  private ViewPager2 pager;
  private final FrameLayout[] dots = new FrameLayout[PAGES];

  private final ViewPager2.OnPageChangeCallback callback =
      new ViewPager2.OnPageChangeCallback() {
        @Override
        public void onPageSelected(int position) {
          Log.i(TAG, "selected " + position);
          for (int i = 0; i < PAGES; i++) {
            dots[i].setBackgroundColor(i == position ? DOT_ON : DOT_OFF);
          }
          onSelected(position);
        }

        @Override
        public void onPageScrollStateChanged(int state) {
          Log.i(TAG, "state " + stateName(state));
          if (state == ViewPager2.SCROLL_STATE_SETTLING) {
            sawSettling = true;
          }
          if (state == ViewPager2.SCROLL_STATE_IDLE && step == 1) {
            step = 2;
            later(() -> pager.setCurrentItem(2, false));
          }
        }

        @Override
        public void onPageScrolled(int position, float positionOffset, int positionOffsetPixels) {
          Log.i(TAG, "scrolled " + position + " " + positionOffsetPixels);
          scrolled++;
          if (positionOffset != 0f || positionOffsetPixels != 0) {
            scrolledBad = true;
          }
        }
      };

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    setContentView(R.layout.activity_main);
    pager = findViewById(R.id.pager);
    dots[0] = findViewById(R.id.dot0);
    dots[1] = findViewById(R.id.dot1);
    dots[2] = findViewById(R.id.dot2);
    pager.registerOnPageChangeCallback(callback);
    if (savedInstanceState != null) {
      pager.restoreState(savedInstanceState.getBundle("pager"));
    }
    pager.setAdapter(
        new FragmentStateAdapter(this) {
          @Override
          public int getItemCount() {
            return PAGES;
          }

          @Override
          public Fragment createFragment(int position) {
            return PageFragment.newInstance(position);
          }
        });
    check("nothing selected before the first tick", savedInstanceState != null || step == 0);
    if (savedInstanceState == null) {
      Log.i(TAG, "ready");
    }
  }

  @Override
  protected void onSaveInstanceState(Bundle outState) {
    super.onSaveInstanceState(outState);
    outState.putBundle("pager", pager.saveState());
  }

  private Fragment current() {
    FragmentManager fm = getSupportFragmentManager();
    return fm.findFragmentByTag("f" + pager.getCurrentItem());
  }

  private void onSelected(int position) {
    switch (step) {
      case 0:
        check(
            "page 0 resumed on the first layout",
            position == 0 && lastVisits[0] == 1 && current() != null && current().isResumed());
        step = 1;
        later(() -> pager.setCurrentItem(1, true));
        break;
      case 1:
        check(
            "smooth turn: settling, old page destroyed, new page resumed",
            position == 1
                && sawSettling
                && pager.getScrollState() == ViewPager2.SCROLL_STATE_SETTLING
                && destroyed[0] == 1
                && lastVisits[1] == 1);
        break; // the IDLE after the fade advances the script
      case 2:
        check(
            "instant turn: no settling",
            position == 2
                && pager.getScrollState() == ViewPager2.SCROLL_STATE_IDLE
                && destroyed[1] == 1);
        step = 3;
        later(() -> pager.setCurrentItem(0, false));
        break;
      case 3:
        check(
            "page 0 came back with its state",
            position == 0 && lastVisits[0] == 2 && pager.getCurrentItem() == 0);
        check("one scrolled per turn, zero offset", scrolled == 3 && !scrolledBad);
        step = 4;
        later(() -> pager.setCurrentItem(1, false));
        break;
      case 4:
        check("back on page 1 with its state", position == 1 && lastVisits[1] == 2);
        step = 5;
        later(() -> recreate());
        break;
      case 5:
        check(
            "recreate restored the page and its state",
            position == 1 && pager.getCurrentItem() == 1 && lastVisits[1] == 3);
        Log.i(TAG, "restored item=1 via recreate");
        step = 6;
        later(() -> startActivity(new Intent(CoverActivity.class)));
        break;
      case 6:
        check("reclaim restored the page and its state", position == 1 && lastVisits[1] == 4);
        Log.i(TAG, "restored item=1 via reclaim");
        step = 7;
        later(() -> pager.setCurrentItem(0, false));
        break;
      case 7:
        check("page 0 state survived both", position == 0 && lastVisits[0] == 3);
        pager.setOffscreenPageLimit(1);
        check(
            "getters",
            pager.getOffscreenPageLimit() == 1
                && pager.isUserInputEnabled()
                && pager.getOrientation() == ViewPager2.ORIENTATION_HORIZONTAL
                && pager.getAdapter() != null);
        step = 8;
        Log.i(TAG, "ready for swipe left");
        break;
      case 8:
        check("a swipe left turned to page 1", position == 1 && lastVisits[1] == 5);
        step = 9;
        Log.i(TAG, "ready for swipe right");
        break;
      case 9:
        check("a swipe right turned back to page 0", position == 0 && lastVisits[0] == 4);
        if (failures == 0) {
          Log.i(TAG, "=== ALL PASSED ===");
        } else {
          Log.i(TAG, "=== FAILED: " + failures + " ===");
        }
        step = 10;
        finish();
        break;
      default:
        break;
    }
  }

  private static void check(String what, boolean ok) {
    if (!ok) {
      failures++;
      Log.i(TAG, "FAIL: " + what);
    }
  }

  private static String stateName(int state) {
    switch (state) {
      case ViewPager2.SCROLL_STATE_SETTLING:
        return "SETTLING";
      case ViewPager2.SCROLL_STATE_DRAGGING:
        return "DRAGGING";
      default:
        return "IDLE";
    }
  }

  /** The main-thread deadline table, the SDK's delayed-work shape. */
  private static final ScheduledExecutorService TIMER =
      Executors.newSingleThreadScheduledExecutor();

  /** A little later, on the main thread: the pager's own ticks must run first. */
  private static void later(Runnable r) {
    TIMER.schedule(r, 60, TimeUnit.MILLISECONDS);
  }
}
