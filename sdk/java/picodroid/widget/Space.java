// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

/**
 * Mirrors {@code android.widget.Space}: a view that draws nothing and takes room. With a weight in
 * a {@link LinearLayout} it pushes its neighbours apart — a header's title to the left and its icon
 * to the right, whatever the panel's width — which is how a layout written once stays right on
 * every board. In a layout file it is {@code <Space>}; a plain {@code View} is the same thing with
 * a background.
 */
public class Space extends View {
  public Space(Context context) {
    super(context);
  }
}
