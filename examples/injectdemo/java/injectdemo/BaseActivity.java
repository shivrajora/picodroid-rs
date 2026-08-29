// SPDX-License-Identifier: GPL-3.0-only
package injectdemo;

import javax.inject.Inject;
import picodroid.app.Activity;

/** Superclass members are injected first, before the leaf's own. */
public abstract class BaseActivity extends Activity {
  @Inject Clock clock;
}
