// SPDX-License-Identifier: GPL-3.0-only
package qa_life;

import picodroid.app.Application;
import picodroid.content.Intent;

/**
 * QA 2026-09-13: Activity and Service lifecycle ordering, Intent extras of every supported type,
 * startActivityForResult with a finish() inside onCreate, a Service that is both started and bound
 * from two Activities, and preferences shared across Activities. The App starts ActA; ActA drives
 * the rest and reports.
 */
public class QaLifeApp extends Application {
  static int appCreates = 0;

  @Override
  public void onCreate() {
    appCreates++;
    T.log("App.onCreate");
    T.check("app package name", "qa_life".equals(getPackageName()));
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < 200; i++) {
      sb.append((char) ('A' + i % 26));
    }
    Intent i =
        new Intent(ActA.class)
            .putExtra("n", 42)
            .putExtra("neg", Integer.MIN_VALUE)
            .putExtra("s", "hello")
            .putExtra("empty", "")
            .putExtra("flag", true)
            .putExtra("f2", false)
            .putExtra("long", sb.toString())
            .putExtra("k.with-odd_chars/", "v");
    T.check("extras count", i.getExtras().size() == 8);
    T.check("hasExtra", i.hasExtra("n") && i.hasExtra("empty") && !i.hasExtra("zz"));
    T.check("getIntExtra before start", i.getIntExtra("n", 0) == 42);
    T.check(
        "target class name",
        i.getTargetClassName() != null && i.getTargetClassName().contains("ActA"));
    i.putExtra("n", 43);
    T.check("putExtra replaces", i.getIntExtra("n", 0) == 43 && i.getExtras().size() == 8);
    startActivity(i);
  }
}
