package imagedemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.util.Log;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/** Smoke test for bundled image assets in the papk. */
public class ImageDemoActivity extends Activity {
  public void onCreate() {
    Log.i("ImageDemo", "loading bundled asset 'logo.png'");

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
    image.setImageSource("logo.png");
    image.setScaleType(ImageView.SCALE_FIT_CENTER);
    root.addView(image);

    setContentView(root);
    Log.i("ImageDemo", "ImageDemo ready");
  }
}
