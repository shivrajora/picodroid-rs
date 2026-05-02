# Licensing

picodroid-rs is **dual-licensed**. You can use it under either of two
licenses, at your choice:

1. **GPL-3.0-only** — the default open-source license. Free to use,
   modify, and redistribute, subject to the terms of the GNU General
   Public License, version 3, with no Classpath Exception. See
   [LICENSE](LICENSE) for the full text.

2. **Proprietary commercial license** — available from the project
   maintainer for customers who need to distribute closed-source
   derivatives or apps. Contact `rajora.shiv@gmail.com` for terms.

You do **not** need to choose between them up front. If you only ever
distribute your work as open source under GPL-3.0, you never need the
commercial license. If at some point you need to ship closed-source,
contact us before doing so.

## Which license applies to you?

### You are using picodroid as-is, for personal/hobbyist use

You can use picodroid under GPL-3.0 with no further obligation. If you do
not redistribute (convey) the resulting work to anyone else, GPL-3.0
imposes no requirements on you.

### You are forking or modifying picodroid itself

GPL-3.0 applies. Your fork must be released under GPL-3.0 (or a
compatible license), with full source available to anyone you distribute
it to. This is the standard "viral" property of GPL.

### You are writing a Java app that imports the picodroid SDK (`picodroid.*`)

GPL-3.0 applies — and **there is no Classpath Exception**. The Classpath
Exception, used by OpenJDK, allows applications to link the GPL-licensed
standard library without becoming GPL themselves. picodroid does **not**
grant that exception. This is intentional.

In practice this means: if you publish or distribute a `.papk` app that
links any class from `picodroid.app`, `picodroid.widget`, `picodroid.io`,
etc., your app must also be released under GPL-3.0 with full source.

If you need to ship a closed-source app that runs on picodroid, **buy the
commercial license** — that is exactly what it is for.

### You are a commercial customer building a closed-source product on picodroid

The commercial license removes the GPL-3.0 obligations. You can ship
firmware images and `.papk` apps without releasing source, integrate
picodroid into proprietary products, and modify the runtime without
publishing your modifications.

Contact `rajora.shiv@gmail.com` to discuss terms.

## Third-party components

picodroid bundles several third-party libraries (FreeRTOS-Kernel, LVGL,
LZ4, littlefs-rust, cyw43-driver, the rp-hal stack, and various Rust
crates). All of them are under permissive or compatible licenses (MIT,
BSD-2-Clause, BSD-3-Clause, MPL-2.0, Apache-2.0, or the Raspberry Pi
clause of cyw43-driver). See [NOTICE](NOTICE) for the full list and the
specific terms that apply to each. The dual-license arrangement above
covers only picodroid's own code; you must still comply with the
upstream terms of any third-party component you distribute.

## Contributors

Contributions are accepted under an inbound license grant that lets the
maintainer keep the dual-license model intact. See
[CONTRIBUTING.md](CONTRIBUTING.md) and [CLA.md](CLA.md). The grant is
non-exclusive: you keep your copyright and remain free to do whatever
you want with your own contribution outside of picodroid.
