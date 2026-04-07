package picodroid.widget;

import picodroid.view.View;

public class SeekBar extends View {
  private Runnable onSeekBarChangeListener;

  public SeekBar() {
    super(nativeCreate());
  }

  public SeekBar(int max) {
    super(nativeCreateWithMax(max));
  }

  private static native int nativeCreate();

  private static native int nativeCreateWithMax(int max);

  public native void setMax(int max);

  public native void setProgress(int progress);

  public native int getProgress();

  public void setOnSeekBarChangeListener(Runnable listener) {
    this.onSeekBarChangeListener = listener;
    nativeRegisterChangeListener();
  }

  private native void nativeRegisterChangeListener();

  void fireProgressChanged() {
    if (onSeekBarChangeListener != null) {
      onSeekBarChangeListener.run();
    }
  }
}
