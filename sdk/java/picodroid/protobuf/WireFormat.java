// SPDX-License-Identifier: GPL-3.0-only
package picodroid.protobuf;

/**
 * The protobuf wire-format constants and tag arithmetic, mirroring {@code
 * com.google.protobuf.WireFormat}: a tag is {@code (fieldNumber << 3) | wireType}.
 */
public final class WireFormat {
  public static final int WIRETYPE_VARINT = 0;
  public static final int WIRETYPE_FIXED64 = 1;
  public static final int WIRETYPE_LENGTH_DELIMITED = 2;
  public static final int WIRETYPE_START_GROUP = 3;
  public static final int WIRETYPE_END_GROUP = 4;
  public static final int WIRETYPE_FIXED32 = 5;

  static final int TAG_TYPE_BITS = 3;
  static final int TAG_TYPE_MASK = (1 << TAG_TYPE_BITS) - 1;

  private WireFormat() {}

  /** The wire type of {@code tag}: one of the {@code WIRETYPE_*} constants. */
  public static int getTagWireType(int tag) {
    return tag & TAG_TYPE_MASK;
  }

  /** The field number of {@code tag}. */
  public static int getTagFieldNumber(int tag) {
    return tag >>> TAG_TYPE_BITS;
  }

  /** The tag for {@code fieldNumber} with {@code wireType}. */
  public static int makeTag(int fieldNumber, int wireType) {
    return (fieldNumber << TAG_TYPE_BITS) | wireType;
  }
}
