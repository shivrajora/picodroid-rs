package listdemo;

import java.util.ArrayList;
import picodroid.app.Application;
import picodroid.util.Log;

public class ListDemo extends Application {
  private static final String TAG = "ListDemo";

  public void onCreate() {
    run();
  }

  public static void run() {
    // ── String list ───────────────────────────────────────────────────────────
    ArrayList list = new ArrayList();
    list.add("alpha");
    list.add("beta");
    list.add("gamma");

    Log.i(TAG, "size=" + list.size()); // 3
    Log.i(TAG, "isEmpty=" + list.isEmpty()); // false

    // NOTE: capture results before concatenation (+) to avoid shared-buffer
    // interference with the compiler-generated StringBuilder.
    String item = (String) list.get(1);
    Log.i(TAG, "get(1)=" + item); // beta

    String old = (String) list.set(0, "ALPHA");
    Log.i(TAG, "set(0) old=" + old); // alpha

    String removed = (String) list.remove(2);
    Log.i(TAG, "remove(2)=" + removed); // gamma

    Log.i(TAG, "size after remove=" + list.size()); // 2
    Log.i(TAG, "contains ALPHA=" + list.contains("ALPHA")); // true
    Log.i(TAG, "contains gamma=" + list.contains("gamma")); // false

    list.clear();
    Log.i(TAG, "size after clear=" + list.size()); // 0

    // ── Integer autoboxing ────────────────────────────────────────────────────
    ArrayList<Integer> nums = new ArrayList<Integer>();
    nums.add(10);
    nums.add(20);
    nums.add(30);

    Log.i(TAG, "nums size=" + nums.size()); // 3

    int n = nums.get(0);
    Log.i(TAG, "nums.get(0)=" + n); // 10

    Log.i(TAG, "contains 20=" + nums.contains(20)); // true
    Log.i(TAG, "contains 99=" + nums.contains(99)); // false

    nums.remove(1);
    int numsSize = nums.size();
    Log.i(TAG, "nums size after remove=" + numsSize); // 2

    // ── Boolean autoboxing ────────────────────────────────────────────────────
    ArrayList<Boolean> flags = new ArrayList<Boolean>();
    flags.add(true);
    flags.add(false);
    flags.add(true);

    boolean f0 = flags.get(0);
    boolean f1 = flags.get(1);
    Log.i(TAG, "flags.get(0)=" + f0); // true
    Log.i(TAG, "flags.get(1)=" + f1); // false
  }
}
