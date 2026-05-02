// SPDX-License-Identifier: GPL-3.0-only
package picodroid.concurrent;

final class MainExecutor implements Executor {
  @Override
  public native void execute(Runnable command);
}
