package picodroid.widget;

import picodroid.view.View;

public class ImageView extends View {
  public ImageView() {
    super(nativeCreate());
  }

  private static native int nativeCreate();

  public native void setImageSource(String path);
}
