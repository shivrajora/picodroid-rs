// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import javax.inject.Inject;

/**
 * All three injection kinds on one plain class: constructor, field, and method. Built from ordinary
 * code through the generated {@code Message_Factory.get()} — the entry point for pulling a graph
 * object outside a framework-owned component.
 */
public class Message {
  private final Clock ctorClock;

  @Inject Clock clock;

  private Greeter greeter;

  @Inject
  public Message(Clock clock) {
    this.ctorClock = clock;
  }

  @Inject
  void setGreeter(Greeter greeter) {
    this.greeter = greeter;
  }

  public boolean fieldsOk() {
    return clock != null && clock == ctorClock;
  }

  public boolean methodOk() {
    return greeter != null && greeter.clock() == clock;
  }
}
