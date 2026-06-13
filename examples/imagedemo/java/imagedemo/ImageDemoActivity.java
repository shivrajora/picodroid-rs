// SPDX-License-Identifier: GPL-3.0-only
package imagedemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/** Smoke test for bundled image assets in the papk. */
public class ImageDemoActivity extends Activity {
  @Override
  public void onCreate() {
    // AssetConstants is generated at build time from this app's assets/ dir;
    // AssetConstants.LOGO == "logo.png" (compile-checked, no stringly-typed
    // asset name to typo).
    Log.i("ImageDemo", "loading bundled asset '" + AssetConstants.LOGO + "'");

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(320, 240);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Image asset demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    ImageView image = new ImageView();
    image.setSize(160, 160);
    image.setImageSource(AssetConstants.LOGO);
    image.setScaleType(ImageView.SCALE_FIT_CENTER);
    root.addView(image);

    // Unscaled, centered (Android ScaleType.CENTER): a 60px box around the
    // intrinsic-size logo, clipping if the asset is larger.
    ImageView centered = new ImageView();
    centered.setSize(60, 60);
    centered.setImageSource(AssetConstants.LOGO);
    centered.setScaleType(ImageView.SCALE_CENTER);
    root.addView(centered);
    Log.i("ImageDemo", "scale-center applied");

    setContentView(root);
    Log.i("ImageDemo", "ImageDemo ready");
  }
}
