// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

/** An interface binding: no @Inject constructor possible, so DemoModule provides it. */
public interface Greeting {
  String greet(String who);
}
