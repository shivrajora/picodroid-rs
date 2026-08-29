// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import javax.inject.Inject;

/** Unscoped: a fresh Greeter per injection site, all sharing the singleton Clock. */
public class Greeter {
  private final Clock clock;

  @Inject
  public Greeter(Clock clock) {
    this.clock = clock;
  }

  public Clock clock() {
    return clock;
  }

  public String greet(String who) {
    return "hello " + who + " @clock#" + clock.id();
  }
}
