// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import javax.inject.Inject;
import javax.inject.Singleton;

/** App-scoped: every injection site sees the one instance, so {@code id()} is always 1. */
@Singleton
public class Clock {
  private static int created;
  private final int id;

  @Inject
  public Clock() {
    id = ++created;
  }

  public int id() {
    return id;
  }
}
