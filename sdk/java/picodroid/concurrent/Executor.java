// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

public interface Executor {
  void execute(Runnable command);
}
