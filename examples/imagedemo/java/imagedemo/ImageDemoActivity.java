// SPDX-License-Identifier: GPL-3.0-only
package imagedemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/** Smoke test for bundled image assets in the papk. */
public class ImageDemoActivity extends Activity {
  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    // AssetConstants is generated at build time from this app's assets/ dir;
    // AssetConstants.LOGO == "logo.png" (compile-checked, no stringly-typed
    // asset name to typo).
    Log.i("ImageDemo", "loading bundled asset '" + AssetConstants.LOGO + "'");

    LinearLayout root = new LinearLayout(this);
    root.setOrientation(LinearLayout.VERTICAL);
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView(this);
    title.setText("Image asset demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    ImageView image = new ImageView(this);
    image.setImageSource(AssetConstants.LOGO);
    image.setScaleType(ImageView.SCALE_FIT_CENTER);
    // 150 px: title, this and the 60 px box below fit a 240-tall panel (160 overflowed it by 6).
    root.addView(image, new LinearLayout.LayoutParams(150, 150));

    // Unscaled, centered (Android ScaleType.CENTER): a 60px box around the
    // intrinsic-size logo, clipping if the asset is larger.
    ImageView centered = new ImageView(this);
    centered.setImageSource(AssetConstants.LOGO);
    centered.setScaleType(ImageView.SCALE_CENTER);
    root.addView(centered, new LinearLayout.LayoutParams(60, 60));
    Log.i("ImageDemo", "scale-center applied");

    setContentView(root);
    Log.i("ImageDemo", "ImageDemo ready");
  }
}
