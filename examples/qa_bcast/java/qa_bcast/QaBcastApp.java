// SPDX-License-Identifier: GPL-3.0-only
package qa_bcast;

import picodroid.app.Application;
import picodroid.content.Intent;

/**
 * QA for broadcasts (docs/designs/broadcasts-2026-10.md): the registry's rules, in-app {@code
 * sendBroadcast} to registered and to explicit receivers, delivery order and re-entrancy, the
 * leaked-receiver reap on an Activity's destroy, and the system actions the framework raises. The
 * App starts ActA; ActA drives the rest and reports.
 */
public class QaBcastApp extends Application {
  @Override
  public void onCreate() {
    T.log("App.onCreate");
    startActivity(new Intent(this, ActA.class));
  }
}
