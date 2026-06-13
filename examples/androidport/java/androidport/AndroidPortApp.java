// SPDX-License-Identifier: GPL-3.0-only
package androidport;

import android.app.Application;
import android.content.Intent;

/**
 * Acceptance demo for the android.* compatibility layer (compat-aliases + stub jar). Every import
 * is android.*; at build time the stubs make them compile, and class-shrink rewrites the android/*
 * bytecode references to the real picodroid/* classes. Nothing here imports picodroid.*.
 */
public class AndroidPortApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(AndroidPortActivity.class));
  }
}
