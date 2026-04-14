package bootcount;

import picodroid.app.Application;
import picodroid.io.File;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;
import picodroid.util.Log;

public class BootCount extends Application {
  private static final String TAG = "BootCount";
  private static final String PATH = "/boot.count";

  @Override
  public void onCreate() {
    int count = readCount() + 1;
    writeCount(count);
    Log.i(TAG, "Boot #" + count);
  }

  // Stored as a single little-endian 32-bit value (4 bytes).
  private int readCount() {
    File f = new File(PATH);
    if (!f.exists()) {
      return 0;
    }
    byte[] buf = new byte[4];
    FileInputStream in = new FileInputStream(f);
    int n = in.read(buf);
    in.close();
    if (n < 4) {
      return 0;
    }
    return (buf[0] & 0xff)
        | ((buf[1] & 0xff) << 8)
        | ((buf[2] & 0xff) << 16)
        | ((buf[3] & 0xff) << 24);
  }

  private void writeCount(int count) {
    byte[] buf = new byte[4];
    buf[0] = (byte) (count & 0xff);
    buf[1] = (byte) ((count >> 8) & 0xff);
    buf[2] = (byte) ((count >> 16) & 0xff);
    buf[3] = (byte) ((count >> 24) & 0xff);
    FileOutputStream out = new FileOutputStream(PATH);
    out.write(buf);
    out.close();
  }
}
