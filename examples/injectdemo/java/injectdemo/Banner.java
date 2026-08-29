// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

/** A plain value with no @Inject constructor — only reachable through DemoModule. */
public final class Banner {
  private final String text;

  Banner(String text) {
    this.text = text;
  }

  public String text() {
    return text;
  }
}
