// SPDX-License-Identifier: GPL-3.0-only
package picodroid.protobuf;

import java.io.IOException;

/**
 * What a generated message can do on the wire, the subset of {@code
 * com.google.protobuf.MessageLite} that {@code protoc-gen-picodroid} emits: serialize into a {@link
 * CodedOutputStream}, report the size it will take, and produce its bytes. Parsing is a static
 * {@code parseFrom} on the generated class.
 */
public interface MessageLite {
  /** Writes every set field to {@code output}, in field-number order. */
  void writeTo(CodedOutputStream output) throws IOException;

  /** The number of bytes {@link #writeTo} will produce. */
  int getSerializedSize();

  /** The serialized message in a fresh array of exactly {@link #getSerializedSize()} bytes. */
  byte[] toByteArray();
}
