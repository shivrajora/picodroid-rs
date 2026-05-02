// SPDX-License-Identifier: GPL-3.0-only
package collectionsdemo;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import picodroid.app.Application;
import picodroid.util.Log;

public class CollectionsDemo extends Application {
  private static final String TAG = "CollectionsDemo";

  static class Item implements Comparable<Item> {
    final String name;
    final int score;

    Item(String name, int score) {
      this.name = name;
      this.score = score;
    }

    @Override
    public int compareTo(Item other) {
      return this.score - other.score;
    }

    @Override
    public String toString() {
      return name + "(" + score + ")";
    }
  }

  public void onCreate() {
    // Arrays.sort(Object[]) — Java-side mergesort using Comparable.
    Item[] items = {
      new Item("alpha", 30), new Item("bravo", 10), new Item("charlie", 50), new Item("delta", 20),
    };
    Arrays.sort(items);
    Log.i(TAG, "sorted Object[] = " + Arrays.toString(items));

    // Collections.sort(List) — copies into Object[], delegates to Arrays.sort.
    ArrayList<Item> list = new ArrayList<Item>();
    list.add(new Item("zeta", 90));
    list.add(new Item("yota", 5));
    list.add(new Item("xena", 60));
    Collections.sort(list);
    StringBuilder sb = new StringBuilder("sorted List =");
    for (int i = 0; i < list.size(); i++) {
      sb.append(' ');
      sb.append(list.get(i).toString());
    }
    Log.i(TAG, sb.toString());

    // Collections.reverse(List) — in-place swap.
    Collections.reverse(list);
    sb = new StringBuilder("reversed List =");
    for (int i = 0; i < list.size(); i++) {
      sb.append(' ');
      sb.append(list.get(i).toString());
    }
    Log.i(TAG, sb.toString());
  }
}
