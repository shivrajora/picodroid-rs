// SPDX-License-Identifier: GPL-3.0-only
package picodroid.protobuf;

import java.io.IOException;

/**
 * Thrown when a protobuf message on the wire is malformed, mirroring {@code
 * com.google.protobuf.InvalidProtocolBufferException}: truncated input, a varint longer than ten
 * bytes, a tag with field number zero, a group, or a negative length.
 *
 * <p>Declares no instance fields on purpose: the runtime's natives raise it by class name without
 * running a constructor (the message travels in the exception side table), which only works for a
 * field-less class.
 */
public class InvalidProtocolBufferException extends IOException {
  public InvalidProtocolBufferException(String description) {
    super(description);
  }

  public static InvalidProtocolBufferException truncatedMessage() {
    return new InvalidProtocolBufferException(
        "While parsing a protocol message, the input ended unexpectedly in the middle of a field."
            + " This could mean either that the input has been truncated or that an embedded"
            + " message misreported its own length.");
  }

  public static InvalidProtocolBufferException negativeSize() {
    return new InvalidProtocolBufferException(
        "CodedInputStream encountered an embedded string or message which claimed to have negative "
            + "size.");
  }

  public static InvalidProtocolBufferException malformedVarint() {
    return new InvalidProtocolBufferException("CodedInputStream encountered a malformed varint.");
  }

  public static InvalidProtocolBufferException invalidTag() {
    return new InvalidProtocolBufferException("Protocol message contained an invalid tag (zero).");
  }

  public static InvalidProtocolBufferException invalidEndTag() {
    return new InvalidProtocolBufferException(
        "Protocol message end-group tag did not match expected tag.");
  }

  public static InvalidProtocolBufferException invalidWireType() {
    return new InvalidProtocolBufferException("Protocol message tag had invalid wire type.");
  }

  public static InvalidProtocolBufferException sizeLimitExceeded() {
    return new InvalidProtocolBufferException(
        "Protocol message was too large.  May be malicious.  Use CodedInputStream.setSizeLimit() to"
            + " increase the size limit.");
  }
}
