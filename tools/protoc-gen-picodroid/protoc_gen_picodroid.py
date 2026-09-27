#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""protoc plugin: proto3 messages as Java classes over picodroid.protobuf.

Invoked by protoc (``--plugin=protoc-gen-picodroid=<this file>
--picodroid_out=<dir>``); reads a CodeGeneratorRequest on stdin and writes a
CodeGeneratorResponse on stdout. The generated classes use only
``picodroid.protobuf.*``, ``java.lang``, ``java.util.Arrays`` and
``System.arraycopy``, so they pass the runtime's API contract and cost the
device nothing beyond their own class metadata.

Shape (a deliberate subset of protobuf-javalite, see
website/src/content/docs/api/protobuf.md):

- one mutable final class per message, ``implements picodroid.protobuf.MessageLite``;
  ``getX()``, ``setX(v)`` (returns ``this``), ``clearX()``, ``hasX()`` for
  message and ``optional`` fields
- repeated fields as arrays with a count: ``getXCount()``, ``getX(int)``,
  ``addX(v)``, ``clearX()``
- enums as ``int`` constants in a constants-only class named after the enum
  (javac inlines them, so the class never loads on the device), with
  ``getXValue()`` / ``setXValue(int)`` on the message
- ``static parseFrom(byte[])`` / ``parseFrom(byte[], int, int)`` /
  ``parseFrom(CodedInputStream)``, ``mergeFrom(CodedInputStream)``,
  ``writeTo``, ``getSerializedSize``, ``toByteArray``

Supported: proto3 scalars, string, bytes, bool, enum, nested messages and
enums, repeated (packed and unpacked read; packed write for scalars),
``optional``. Refused with an error: proto2 and editions, map, oneof, groups,
extensions.
"""

import sys

from google.protobuf import descriptor_pb2 as d
from google.protobuf.compiler import plugin_pb2 as plugin

F = d.FieldDescriptorProto

# type -> (java type, reader, NoTag writer suffix, size suffix, wire type, default)
SCALARS = {
    F.TYPE_DOUBLE: ("double", "readDouble", "Double", "Double", 1, "0.0"),
    F.TYPE_FLOAT: ("float", "readFloat", "Float", "Float", 5, "0.0f"),
    F.TYPE_INT64: ("long", "readInt64", "Int64", "Int64", 0, "0L"),
    F.TYPE_UINT64: ("long", "readUInt64", "UInt64", "UInt64", 0, "0L"),
    F.TYPE_INT32: ("int", "readInt32", "Int32", "Int32", 0, "0"),
    F.TYPE_FIXED64: ("long", "readFixed64", "Fixed64", "Fixed64", 1, "0L"),
    F.TYPE_FIXED32: ("int", "readFixed32", "Fixed32", "Fixed32", 5, "0"),
    F.TYPE_BOOL: ("boolean", "readBool", "Bool", "Bool", 0, "false"),
    F.TYPE_STRING: ("String", "readString", "String", "String", 2, '""'),
    F.TYPE_BYTES: ("byte[]", "readBytes", "Bytes", "Bytes", 2, "EMPTY_BYTES"),
    F.TYPE_UINT32: ("int", "readUInt32", "UInt32", "UInt32", 0, "0"),
    F.TYPE_ENUM: ("int", "readEnum", "Enum", "Enum", 0, "0"),
    F.TYPE_SFIXED32: ("int", "readSFixed32", "SFixed32", "SFixed32", 5, "0"),
    F.TYPE_SFIXED64: ("long", "readSFixed64", "SFixed64", "SFixed64", 1, "0L"),
    F.TYPE_SINT32: ("int", "readSInt32", "SInt32", "SInt32", 0, "0"),
    F.TYPE_SINT64: ("long", "readSInt64", "SInt64", "SInt64", 0, "0L"),
}
# Types whose repeated form is packed on the wire (everything but the
# length-delimited ones).
PACKABLE = {t for t, s in SCALARS.items() if s[4] != 2}
# Types Arrays.copyOf serves on this runtime (byte/char/short/int/long/float/double).
COPYOF = {"int", "long", "float", "double"}

JAVA_KEYWORDS = {
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const",
    "continue", "default", "do", "double", "else", "enum", "extends", "final", "finally", "float",
    "for", "goto", "if", "implements", "import", "instanceof", "int", "interface", "long", "native",
    "new", "package", "private", "protected", "public", "return", "short", "static", "strictfp",
    "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try", "void",
    "volatile", "while", "true", "false", "null",
}

WT_VARINT, WT_FIXED64, WT_LEN, WT_FIXED32 = 0, 1, 2, 5

# Accessor stems that would collide with java.lang.Object or the message's
# own methods; javalite appends an underscore to these too.
RESERVED_ACCESSORS = {"Class", "SerializedSize"}


class Fail(Exception):
    pass


def camel(name, upper):
    out = []
    up = upper
    for ch in name:
        if ch == "_":
            up = True
        elif up:
            out.append(ch.upper())
            up = False
        else:
            out.append(ch)
    return "".join(out)


def make_tag(num, wt):
    return (num << 3) | wt


class Field:
    def __init__(self, proto, ctx, where):
        self.p = proto
        self.num = proto.number
        self.name = proto.name
        if proto.type == F.TYPE_GROUP:
            raise Fail("%s: groups are not supported" % where)
        if proto.HasField("extendee"):
            raise Fail("%s: extensions are not supported" % where)
        self.repeated = proto.label == F.LABEL_REPEATED
        self.optional = proto.proto3_optional
        if proto.HasField("oneof_index") and not self.optional:
            raise Fail("%s: oneof is not supported" % where)
        self.message = proto.type == F.TYPE_MESSAGE
        if self.message:
            target = ctx.lookup.get(proto.type_name)
            if target is None:
                raise Fail("%s: unknown message type %s" % (where, proto.type_name))
            if target.options.map_entry:
                raise Fail("%s: map fields are not supported" % where)
            self.java_type = ctx.java_names[proto.type_name]
            self.wt = WT_LEN
        else:
            (self.java_type, self.reader, self.suffix, self.size_suffix, self.wt, self.default) = SCALARS[
                proto.type
            ]
        base = camel(self.name, False)
        if base in JAVA_KEYWORDS:
            base += "_"
        self.field = base + "_"
        cap = camel(self.name, True)
        if cap in RESERVED_ACCESSORS:
            cap += "_"  # javalite's escape: `class` -> getClass_()
        # javalite: an enum field's value accessors carry `Value` (getMoodValue,
        # addMoodsValue), its count and clear do not (getMoodsCount, clearMoods).
        self.stem = cap
        self.acc = cap + "Value" if proto.type == F.TYPE_ENUM else cap
        self.count = base + "Count_"
        self.packed = self.repeated and proto.type in PACKABLE
        self.tag = make_tag(self.num, self.wt)


class Ctx:
    def __init__(self, fd):
        self.fd = fd
        self.package = fd.options.java_package or fd.package
        self.multiple = fd.options.java_multiple_files
        self.outer = fd.options.java_outer_classname or (camel(basename(fd.name), True) + ("OuterClass" if self.clashes(fd) else ""))
        # full proto name -> descriptor, and -> the Java spelling of the type
        # from the same package (`Outer.Inner`, with the outer class when the
        # file generates one).
        self.lookup = {}
        self.java_names = {}
        prefix = "." + fd.package if fd.package else ""
        for m in fd.message_type:
            self.index(m, prefix, "" if self.multiple else self.outer + ".")
        for e in fd.enum_type:
            self.lookup[prefix + "." + e.name] = e

    def clashes(self, fd):
        n = camel(basename(fd.name), True)
        return any(m.name == n for m in fd.message_type) or any(e.name == n for e in fd.enum_type)

    def index(self, m, prefix, java_prefix):
        full = prefix + "." + m.name
        java_name = java_prefix + m.name
        self.lookup[full] = m
        self.java_names[full] = java_name
        for n in m.nested_type:
            self.index(n, full, java_name + ".")
        for e in m.enum_type:
            self.lookup[full + "." + e.name] = e


def basename(path):
    name = path.rsplit("/", 1)[-1]
    return name[:-6] if name.endswith(".proto") else name


class Out:
    def __init__(self):
        self.lines = []
        self.depth = 0

    def __call__(self, line=""):
        self.lines.append(("  " * self.depth + line) if line else "")

    def block(self, head):
        self(head + " {")
        self.depth += 1

    def end(self):
        self.depth -= 1
        self("}")

    def text(self):
        return "\n".join(self.lines) + "\n"


def gen_enum(o, e, static):
    o("/** The values of proto enum {@code %s}; {@code int} constants, inlined by javac. */" % e.name)
    o.block("public %sfinal class %s" % ("static " if static else "", e.name))
    for v in e.value:
        o("public static final int %s = %d;" % (v.name, v.number))
    o()
    o("private %s() {}" % e.name)
    o.end()


def gen_message(o, m, ctx, where, static):
    fields = [Field(f, ctx, where + "." + f.name) for f in m.field]
    fields.sort(key=lambda f: f.num)
    o("/** Proto message {@code %s}. */" % where)
    o.block("public %sfinal class %s implements MessageLite" % ("static " if static else "", m.name))
    for e in m.enum_type:
        gen_enum(o, e, True)
        o()
    for n in m.nested_type:
        if n.options.map_entry:
            continue
        gen_message(o, n, ctx, where + "." + n.name, True)
        o()
    if any(not f.repeated and not f.message and f.p.type == F.TYPE_BYTES for f in fields):
        o("private static final byte[] EMPTY_BYTES = new byte[0];")
        o()
    # ── storage ─────────────────────────────────────────────────────────
    for f in fields:
        if f.repeated:
            o("private %s[] %s;" % (f.java_type, f.field))
            o("private int %s;" % f.count)
        elif f.message:
            o("private %s %s;" % (f.java_type, f.field))
        else:
            o("private %s %s = %s;" % (f.java_type, f.field, f.default))
            if f.optional:
                o("private boolean has%s_;" % f.stem)
    o()
    o("public %s() {}" % m.name)
    # ── accessors ───────────────────────────────────────────────────────
    for f in fields:
        o()
        if f.repeated:
            gen_repeated_accessors(o, f, m.name)
        elif f.message:
            o.block("public boolean has%s()" % f.acc)
            o("return %s != null;" % f.field)
            o.end()
            o()
            o.block("public %s get%s()" % (f.java_type, f.acc))
            o("return %s;" % f.field)
            o.end()
            o()
            o.block("public %s set%s(%s value)" % (m.name, f.acc, f.java_type))
            o("%s = value;" % f.field)
            o("return this;")
            o.end()
            o()
            o.block("public %s clear%s()" % (m.name, f.acc))
            o("%s = null;" % f.field)
            o("return this;")
            o.end()
        else:
            if f.optional:
                o.block("public boolean has%s()" % f.stem)
                o("return has%s_;" % f.stem)
                o.end()
                o()
            o.block("public %s get%s()" % (f.java_type, f.acc))
            o("return %s;" % f.field)
            o.end()
            o()
            o.block("public %s set%s(%s value)" % (m.name, f.acc, f.java_type))
            if f.java_type in ("String", "byte[]"):
                o.block("if (value == null)")
                o('throw new NullPointerException("%s");' % f.name)
                o.end()
            o("%s = value;" % f.field)
            if f.optional:
                o("has%s_ = true;" % f.stem)
            o("return this;")
            o.end()
            o()
            o.block("public %s clear%s()" % (m.name, f.stem))
            o("%s = %s;" % (f.field, f.default))
            if f.optional:
                o("has%s_ = false;" % f.stem)
            o("return this;")
            o.end()
    # ── parse ───────────────────────────────────────────────────────────
    o()
    o.block("public static %s parseFrom(byte[] data) throws InvalidProtocolBufferException" % m.name)
    o("return parseFrom(data, 0, data.length);")
    o.end()
    o()
    o.block(
        "public static %s parseFrom(byte[] data, int off, int len) throws InvalidProtocolBufferException"
        % m.name
    )
    o("return parseFrom(CodedInputStream.newInstance(data, off, len));")
    o.end()
    o()
    o.block("public static %s parseFrom(CodedInputStream input) throws InvalidProtocolBufferException" % m.name)
    o("%s message = new %s();" % (m.name, m.name))
    o.block("try")
    o("message.mergeFrom(input);")
    o.end()
    o.lines[-1] = o.lines[-1] + " catch (InvalidProtocolBufferException e) {"
    o.depth += 1
    o("throw e;")
    o.end()
    o.lines[-1] = o.lines[-1] + " catch (IOException e) {"
    o.depth += 1
    o("throw new InvalidProtocolBufferException(e.getMessage());")
    o.end()
    o("return message;")
    o.end()
    o()
    o("/** Reads fields from {@code input} until its end or limit, on top of what is set. */")
    o.block("public %s mergeFrom(CodedInputStream input) throws IOException" % m.name)
    o.block("while (true)")
    o("int tag = input.readTag();")
    o.block("switch (tag)")
    o("case 0:")
    o("  return this;")
    for f in fields:
        o("case %d:" % f.tag)
        o.depth += 1
        gen_read_case(o, f)
        o("break;")
        o.depth -= 1
        if f.packed:
            o("case %d:" % make_tag(f.num, WT_LEN))
            o.depth += 1
            o.block("")
            o.lines[-1] = o.lines[-1].rstrip()
            o("int length = input.readRawVarint32();")
            o("int limit = input.pushLimit(length);")
            o.block("while (input.getBytesUntilLimit() > 0)")
            o("add%s(input.%s());" % (f.acc, f.reader))
            o.end()
            o("input.popLimit(limit);")
            o.end()
            o("break;")
            o.depth -= 1
    o("default:")
    o("  input.skipField(tag);")
    o.end()
    o.end()
    o.end()
    # ── write ───────────────────────────────────────────────────────────
    o()
    o("@Override")
    o.block("public void writeTo(CodedOutputStream output) throws IOException")
    for f in fields:
        gen_write(o, f)
    o.end()
    o()
    o("@Override")
    o.block("public int getSerializedSize()")
    o("int size = 0;")
    for f in fields:
        gen_size(o, f)
    o("return size;")
    o.end()
    for f in fields:
        if f.packed:
            o()
            o.block("private int packed%sSize()" % f.acc)
            o("int size = 0;")
            o.block("for (int i = 0; i < %s; i++)" % f.count)
            o("size += CodedOutputStream.compute%sSizeNoTag(%s[i]);" % (f.size_suffix, f.field))
            o.end()
            o("return size;")
            o.end()
    o()
    o("@Override")
    o.block("public byte[] toByteArray()")
    o("byte[] result = new byte[getSerializedSize()];")
    o.block("try")
    o("CodedOutputStream output = CodedOutputStream.newInstance(result);")
    o("writeTo(output);")
    o("output.checkNoSpaceLeft();")
    o.end()
    o.lines[-1] = o.lines[-1] + " catch (IOException e) {"
    o.depth += 1
    o('throw new RuntimeException("Serializing to a byte array threw an IOException (should never happen).");')
    o.end()
    o("return result;")
    o.end()
    o.end()


def gen_repeated_accessors(o, f, cls):
    t = f.java_type
    o.block("public int get%sCount()" % f.stem)
    o("return %s;" % f.count)
    o.end()
    o()
    o.block("public %s get%s(int index)" % (t, f.acc))
    o.block("if (index < 0 || index >= %s)" % f.count)
    o("throw new IndexOutOfBoundsException(String.valueOf(index));")
    o.end()
    o("return %s[index];" % f.field)
    o.end()
    o()
    o.block("public %s add%s(%s value)" % (cls, f.acc, t))
    if t in ("String", "byte[]") or f.message:
        o.block("if (value == null)")
        o('throw new NullPointerException("%s");' % f.name)
        o.end()
    o.block("if (%s == null || %s == %s.length)" % (f.field, f.count, f.field))
    o("int capacity = %s == 0 ? 4 : %s * 2;" % (f.count, f.count))
    if t in COPYOF:
        o("%s = %s == null ? new %s[capacity] : Arrays.copyOf(%s, capacity);" % (f.field, f.field, t, f.field))
    else:
        elem = t[:-2] if t.endswith("[]") else t
        dims = "[capacity][]" if t.endswith("[]") else "[capacity]"
        o("%s[] grown = new %s%s;" % (t, elem, dims))
        o.block("if (%s != null)" % f.field)
        o("System.arraycopy(%s, 0, grown, 0, %s);" % (f.field, f.count))
        o.end()
        o("%s = grown;" % f.field)
    o.end()
    o("%s[%s++] = value;" % (f.field, f.count))
    o("return this;")
    o.end()
    o()
    o.block("public %s clear%s()" % (cls, f.stem))
    o("%s = null;" % f.field)
    o("%s = 0;" % f.count)
    o("return this;")
    o.end()


def gen_read_case(o, f):
    if f.message:
        o.block("")
        o.lines[-1] = o.lines[-1].rstrip()
        o("int length = input.readRawVarint32();")
        o("int limit = input.pushLimit(length);")
        o("%s value = new %s();" % (f.java_type, f.java_type))
        o("value.mergeFrom(input);")
        o("input.popLimit(limit);")
        if f.repeated:
            o("add%s(value);" % f.acc)
        else:
            o("%s = value;" % f.field)
        o.end()
    elif f.repeated:
        o("add%s(input.%s());" % (f.acc, f.reader))
    else:
        o("%s = input.%s();" % (f.field, f.reader))
        if f.optional:
            o("has%s_ = true;" % f.stem)


def present(f):
    """The condition under which a singular field is on the wire."""
    if f.message:
        return "%s != null" % f.field
    if f.optional:
        return "has%s_" % f.stem
    if f.java_type == "String":
        return "!%s.isEmpty()" % f.field
    if f.java_type == "byte[]":
        return "%s.length != 0" % f.field
    if f.java_type == "boolean":
        return f.field
    if f.java_type in ("float", "double"):
        return "%s != %s" % (f.field, f.default)
    return "%s != %s" % (f.field, f.default)


def gen_write(o, f):
    if f.repeated:
        if f.packed:
            o.block("if (%s > 0)" % f.count)
            o("output.writeTag(%d, WireFormat.WIRETYPE_LENGTH_DELIMITED);" % f.num)
            o("output.writeUInt32NoTag(packed%sSize());" % f.acc)
            o.block("for (int i = 0; i < %s; i++)" % f.count)
            o("output.write%sNoTag(%s[i]);" % (f.suffix, f.field))
            o.end()
            o.end()
        else:
            kind = "Message" if f.message else f.suffix
            o.block("for (int i = 0; i < %s; i++)" % f.count)
            o("output.write%s(%d, %s[i]);" % (kind, f.num, f.field))
            o.end()
    else:
        kind = "Message" if f.message else f.suffix
        o.block("if (%s)" % present(f))
        o("output.write%s(%d, %s);" % (kind, f.num, f.field))
        o.end()


def gen_size(o, f):
    if f.repeated:
        if f.packed:
            o.block("if (%s > 0)" % f.count)
            o("int packed = packed%sSize();" % f.acc)
            o("size += CodedOutputStream.computeTagSize(%d) + CodedOutputStream.computeUInt32SizeNoTag(packed) + packed;" % f.num)
            o.end()
        else:
            kind = "Message" if f.message else f.size_suffix
            o.block("for (int i = 0; i < %s; i++)" % f.count)
            o("size += CodedOutputStream.compute%sSize(%d, %s[i]);" % (kind, f.num, f.field))
            o.end()
    else:
        kind = "Message" if f.message else f.size_suffix
        o.block("if (%s)" % present(f))
        o("size += CodedOutputStream.compute%sSize(%d, %s);" % (kind, f.num, f.field))
        o.end()


def header(o, fd, ctx):
    o("// SPDX-License-Identifier: GPL-3.0-only")
    o("// Generated by protoc-gen-picodroid from %s. DO NOT EDIT." % fd.name)
    if ctx.package:
        o("package %s;" % ctx.package)
    o()
    o("import java.io.IOException;")
    o("import java.util.Arrays;")
    o("import picodroid.protobuf.CodedInputStream;")
    o("import picodroid.protobuf.CodedOutputStream;")
    o("import picodroid.protobuf.InvalidProtocolBufferException;")
    o("import picodroid.protobuf.MessageLite;")
    o("import picodroid.protobuf.WireFormat;")
    o()


def generate_file(fd, response):
    if fd.syntax != "proto3":
        raise Fail("%s: only proto3 is supported (syntax = \"%s\")" % (fd.name, fd.syntax or "proto2"))
    if fd.extension:
        raise Fail("%s: extensions are not supported" % fd.name)
    ctx = Ctx(fd)
    where = fd.package or basename(fd.name)
    directory = ctx.package.replace(".", "/") + "/" if ctx.package else ""
    if ctx.multiple:
        for m in fd.message_type:
            o = Out()
            header(o, fd, ctx)
            gen_message(o, m, ctx, where + "." + m.name, False)
            emit(response, directory + m.name + ".java", o)
        for e in fd.enum_type:
            o = Out()
            o("// SPDX-License-Identifier: GPL-3.0-only")
            o("// Generated by protoc-gen-picodroid from %s. DO NOT EDIT." % fd.name)
            if ctx.package:
                o("package %s;" % ctx.package)
            o()
            gen_enum(o, e, False)
            emit(response, directory + e.name + ".java", o)
    else:
        o = Out()
        header(o, fd, ctx)
        o("/** The messages of {@code %s}. */" % fd.name)
        o.block("public final class %s" % ctx.outer)
        o("private %s() {}" % ctx.outer)
        for e in fd.enum_type:
            o()
            gen_enum(o, e, True)
        for m in fd.message_type:
            o()
            gen_message(o, m, ctx, where + "." + m.name, True)
        o.end()
        emit(response, directory + ctx.outer + ".java", o)


def emit(response, name, o):
    f = response.file.add()
    f.name = name
    f.content = o.text()


def main():
    request = plugin.CodeGeneratorRequest.FromString(sys.stdin.buffer.read())
    response = plugin.CodeGeneratorResponse()
    response.supported_features = plugin.CodeGeneratorResponse.FEATURE_PROTO3_OPTIONAL
    by_name = {f.name: f for f in request.proto_file}
    try:
        for name in request.file_to_generate:
            generate_file(by_name[name], response)
    except Fail as e:
        response.error = str(e)
    sys.stdout.buffer.write(response.SerializeToString())


if __name__ == "__main__":
    main()
