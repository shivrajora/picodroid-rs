# Shrink maps

Committed, append-only mappings from original Java class/method/field names
to their shortened forms. Each file `v<semver>.toml` is tied to a **released**
picodroid version and is immutable once merged.

## How the active map is resolved

Shrinking is **off by default**. Pass `--shrink` to the top-level scripts
(`build.sh`, `flash.sh`, `sim.sh`, `build-apk.sh`) or set
`PICODROID_SHRINK=1` to turn it on for a build. Both firmware (`build.rs`)
and PAPK builds honor the same env var, so the two always agree.

When shrinking is on, tooling reads the `version` field of the root
`Cargo.toml` and picks the **highest** committed map file whose semver is
≤ that version. If none exists, the active map version falls back to the
`0.0.0` sentinel and nothing is rewritten.

`class-shrink print-version` performs this resolution. It's invoked by
`build.rs` and `scripts/build-apk.sh` only when `PICODROID_SHRINK=1`.

## Append-only rule

Cutting a new release (M3's `class-shrink cut-release --version <x.y.z>`
command) must:

1. Copy every entry from the previous release map verbatim. **Never rename
   an existing entry.** This is what lets old PAPKs keep running on newer
   firmware.
2. Allocate new short names for symbols introduced since the previous
   release, continuing the deterministic allocator from where the previous
   release left off.
3. Write the result to `v<new-version>.toml` and commit it together with
   the `Cargo.toml` version bump.

Anything added to the framework between releases stays un-shrunk (full
names in `.class` files) until the next release folds it in. This keeps
the release→map relationship one-to-one and avoids churn on every commit.

## App maps are build outputs, not release maps

A release map never carries `c/` rows. `--shrink-app` (`build-apk.sh`)
cuts a **per-PAPK** map at build time — `class-shrink cut-app` copies the
active release map and appends the app's own classes under `c/` and its
private member names, resuming the release allocator so no target
collides. Plain `--shrink` cuts one too (`--declash-only`), renaming only
the app names that would collide with a shrunk framework name. That merged
file lands next to the PAPK
(`build/apks/<app>.shrink-map.toml`), is the PAPK's retrace key, and is
regenerated on every build; nothing under `sdk/shrink-maps/` changes.

## Versioning & PAPK compatibility

Each PAPK stores `framework-map-version` in its manifest. At load time the
firmware rejects a PAPK whose map version is greater than the firmware's
active version (a PAPK built against a newer release cannot run on older
firmware). Equal-or-lower is accepted, because the append-only rule
guarantees every name the PAPK uses is still present — with one floor:
a map that renames names an older map spelled verbatim moves
`compat::MEMBER_SHRINK_FLOOR` (0.16.0 for the first member map, 0.17.0
when the `java/**` contract members joined it), and firmware at or past
the floor rejects a shrunk PAPK cut before it. Un-keeping a member later
would need another floor.

## Cutting a release

Use the `class-shrink` tool. From the repo root:

```bash
# Fresh-compile the framework to a scratch dir.
TMP=$(mktemp -d)
find sdk/java -name '*.java' -print0 \
  | xargs -0 javac --release 8 -Xlint:-options -d "$TMP"

# The kotlin-shim's member names must never become member targets.
./gradlew :kotlin-shim:compileJava -q

# Generate the map. Pass --base <previous-release-map> to enforce
# append-only: existing entries are copied verbatim and only net-new
# classes get fresh short names. --extra-names feeds the java/** names
# the framework never references itself (RuntimeException, Iterator, …)
# from the committed list of everything pico-jvm serves. --members maps
# method/field names too: everything the SDK declares plus the
# --contract member column (every java/** member the runtime serves —
# toString, equals, hasNext, …; since v0.17.0 they are mapped, not
# kept), every name the --reserve tree spells is never a target, and
# --version becomes member-floor on the first member cut. Add --floor
# only for a cut that renames names the previous map left verbatim —
# it re-bases the floor and every older shrunk PAPK stops loading.
cargo run -p class-shrink -- cut-release --members \
  --classes-dir "$TMP" \
  --keep sdk/keep.toml \
  --extra-names sdk/api-contract.tsv \
  --contract sdk/api-contract.tsv \
  --reserve sdk/kotlin-shim/build/classes/java/main \
  --base sdk/shrink-maps/v<previous>.toml \
  --version <new> \
  --out  sdk/shrink-maps/v<new>.toml
```

Then bump the `version` field in `platforms/rp/Cargo.toml` (the root
`Cargo.toml` is a virtual workspace) and commit both files together. From
that commit onwards, both `build.rs` and `scripts/build-apk.sh`
automatically pick up the new map.

## Namespaces

Shrunk names live in three synthetic packages, each allocated from its own
counter so the suffix sequences never collide:

| Prefix | Holds |
|---|---|
| `a/` | framework classes (`picodroid/**`, `javax/**`) |
| `b/` | `java/**` classes pico-jvm serves natively — the ones defined in `sdk/java`, every one the framework references, and every owner in `sdk/api-contract.tsv` |
| `c/` | an app's own classes — only in the per-app map `--shrink-app` cuts at build time (see [App maps are build outputs](#app-maps-are-build-outputs-not-release-maps)); a release map never has one |

Nothing translates either prefix at run time. The Rust side names every
class, member and descriptor through constants generated from the active
map (`build_support/names.rs`: `c::picodroid_view_View`, `m::toString`,
`d::String__V`), so a `--shrink` firmware's tables, `catch` matching,
`instanceof`, native dispatch and `Class.getName()` all use the mapped
spelling and the image carries no original name — ProGuard semantics.
Build without `--shrink` for readable names, or pipe a shrunk log through
`scripts/retrace.sh`. All three prefixes are reserved: `cut-app` moves an
app class in package `a`, `b` or `c` (or the default package) under a fresh
`c/` name, in both shrink modes.

## Current releases

| Map | Notes |
|---|---|
| `v0.1.0.toml` | First release cut — 42 framework classes outside `java/**`. |
| `v0.2.0.toml` | + `Executors` family, `SensorManager` family, HTTP client, `KeyEvent` / `OnKeyListener`. |
| `v0.3.0.toml` | + `Theme`, drawables, gesture / animation surface, dialog / keyboard widgets. |
| `v0.4.0.toml` | + Service / DI surface (`Service`, `IBinder`, `Notification`, `ServiceConnection`, manual DI components). |
| `v0.5.0.toml` | + Soft-keyboard polish (`OnEditorActionListener`, `EditorInfo`). |
| `v0.6.0.toml` | Stable — byte-identical to v0.5.0 (`picoenvmon` + LTR559 added no framework classes). |
| `v0.7.0.toml` | + Tier C widgets (`Snackbar`, `DatePicker`, `TimePicker`, `SwipeRefreshLayout`, `OnSwipeListener`). |
| `v0.8.0.toml` | Stable — byte-identical to v0.7.0 (PAPK 1.1 bundled assets land outside the framework). |
| `v0.9.0.toml` | Stable — byte-identical to v0.8.0 (relicense, multi-family refactor, ESP32-S3 M1, Display singleton bootstrap). |
| `v0.10.0.toml` | + 23 classes (87 → 110): Android-parity Tier 1/2 typed-listener interfaces, the `Adapter` pattern (`Adapter`, `AdapterView`, `ArrayAdapter`, `BaseAdapter`), `ViewGroup` / `LayoutParams`, `CompoundButton`, and `DialogInterface` / `DisplayDebug`. v0.9.0 entries copied verbatim. |
| `v0.11.0.toml` | + 25 classes (110 → 135): `AlertDialog` moved to `picodroid.app`, `SharedPreferences`, `IBinder` moved to `picodroid.os`, `URL` / `HttpURLConnection` renamed to Java casing, `TextWatcher`, `Gravity`, `InputType`, `GestureDetector.SimpleOnGestureListener`, the animation interpolator family, `NumberPicker`, and `RadioButton` / `RadioGroup`. v0.10.0 entries copied verbatim. |
| `v0.12.0.toml` | Stable — byte-identical to v0.11.0 (the Pico 2 W networking bring-up, FreeRTOS host sim, and crate extractions added no framework classes). |
| `v0.13.0.toml` | Stable — byte-identical to v0.12.0 (the networking-maturity, JVM-correctness, and memory work extended existing classes rather than adding new ones). |
| `v0.14.0.toml` | + 14 classes (135 → 149): the `java.util.concurrent` core set (`Callable`, `Future`, `FutureTask`, `ExecutorService`, `ThreadPoolExecutor`, `TimeUnit`, `CountDownLatch`, the four `Atomic*` types), `Thread.UncaughtExceptionHandler`, and the DI injection points `javax.inject.Provider` / `picodroid.di.Lazy`. v0.13.0 entries copied verbatim. |
| `v0.15.0.toml` | + 88 `java/**` classes under the new `b/` namespace (149 → 237): everything the framework references or pico-jvm serves — `Object`, `String`, `StringBuilder`, the boxed types, the collection classes and interfaces, every builtin exception, the `java.lang.invoke` bootstrap names. The 149 `a/` entries copied verbatim; `a/` allocation is untouched. |
| `v0.16.0.toml` | Schema 2: + 868 `[[member]]` rows — every method and field name the framework declares, keyed by bare name; `member-floor = 0.16.0`. Classes unchanged (238). |
| `v0.17.0.toml` | + 125 members (868 → 993): the `java/**` contract members the runtime serves (`toString`, `hashCode`, `equals`, `hasNext`, …) and javac's `$` synthetics, previously kept; `member-floor` re-based to 0.17.0. Only `main` and `injectMembers` stay verbatim. Classes unchanged. |
| `v0.18.0.toml` | + 1 class (238 → 239): `java/util/Objects`; + 14 members (993 → 1007): the Tier 1 fills — `getFloat` / `putFloat`, `DIRECTION_IN`, `createNewFile` / `mkdirs` / `getParent` / `getParentFile` / `getAbsolutePath`, `hash` / `isNull` / `nonNull` / `requireNonNull`, `intBitsToFloat`, `T_FLOAT`. `member-floor` stays 0.17.0. |
| `v0.19.0.toml` | + 1 class (239 → 240): `picodroid/net/ConnectivityManager`; + 4 members (1007 → 1011): `TYPE_NONE` / `TYPE_WIFI` / `TYPE_ETHERNET` / `FEATURE_ETHERNET` (`getType` was already a target). Member floor unchanged (0.17.0). |
| `v0.20.0.toml` | + 4 classes (240 → 244): `picodroid/json/JSONObject`, `JSONArray`, `JSONException` and `JSONObject$1`; + 83 members (1011 → 1094): the `picodroid.json` surface (`opt*` / `get*` accessors, `accumulate`, `putOpt`, `names`, `quote`, `numberToString`, `wrap`, `NULL`, the `K_*` kind tags and the `native*` bindings). Member floor unchanged (0.17.0). |
| `v0.21.0.toml` | + 5 classes (244 → 249): `picodroid/content/ActivityNotFoundException`, `picodroid/content/pm/PackageInfo`, `ApplicationInfo`, `PackageManager$NameNotFoundException` and `picodroid/graphics/drawable/BitmapDrawable`; + 31 members (1094 → 1125): the multi-app surface — `getInstalledPackages`, `getPackageInfo`, `getLaunchIntentForPackage`, `getApplicationLabel` / `getApplicationIcon`, `loadLabel` / `loadIcon`, `packageName` / `versionName` / `versionCode` / `applicationInfo` / `flags` / `FLAG_SYSTEM`, `getLongVersionCode`, `setPackage` / `getPackage`, `getPackageName`, `setImageDrawable`, `imageHandle` and the `native*` bindings. Member floor unchanged (0.17.0). |
| `v0.22.0.toml` | + 6 classes (249 → 255): `picodroid/os/Build`, `Build$VERSION`, `StatFs`, `picodroid/app/usage/StorageStatsManager`, `StorageStats` and `picodroid/content/pm/PackageInstaller`; + 40 members (1125 → 1165): the storage surface — `getDataDir` / `getFilesDir` / `openFileOutput` / `openFileInput` / `fileList` / `deleteFile` / `filePath` / `FILES_DIR` / `MODE_APPEND` / `STORAGE_STATS_SERVICE`, `list` / `listFiles`, the `StatFs` getters and `restat`, `queryStatsForPackage` / `getAppBytes` / `getDataBytes` / `getCacheBytes` / `appBytes` / `dataBytes`, `BOARD` / `HARDWARE` / `RELEASE`, `getPackageInstaller` / `uninstall` and the `native*` bindings. Member floor unchanged (0.17.0). |
| `v0.23.0.toml` | + 2 classes (255 → 257): `picodroid/text/TextUtils` and `TextUtils$TruncateAt`; + 15 members (1165 → 1180): the `TextView` line-mode surface — `setSingleLine` / `setEllipsize` / `getEllipsize` / `setMaxLines` / `getMaxLines`, `applyLineMode` and `mLineMode` with its `ELLIPSIZE_MASK` / `SINGLE_LINE` / `MAX_LINES_SHIFT` / `MAX_LINES_LIMIT`, the `MIDDLE` and `MARQUEE` constants, and the `nativeSetLineMode` / `nativeSetIncludeFontPadding` bindings. Member floor unchanged (0.17.0). |
| `v0.24.0.toml` | + 2 classes (257 → 259): `picodroid/app/AlarmManager` and `picodroid/app/PendingIntent`; + 29 members (1180 → 1209): the alarm surface — `set` is already mapped, so `setExact` / `cancel`'s siblings `nativeSet`, `fireAlarm`, `getActivity`, `requestCode`, `isIntExtra`, `extraCount` / `extraKey` / `extraInt`, `key0` / `key1` / `value0` / `value1`, `MAX_EXTRAS` / `MAX_KEY_LENGTH`, the four clock constants (`RTC` / `RTC_WAKEUP` / `ELAPSED_REALTIME` / `ELAPSED_REALTIME_WAKEUP`), the six `PendingIntent` flags, `ALARM_SERVICE`, `elapsedRealtime`, `setClassName` and `KEYCODE_HOME`. Member floor unchanged (0.17.0). |
| `v0.25.0.toml` | + 2 classes (259 → 261): `picodroid/media/ToneGenerator` and `picodroid/media/AudioManager`; + 45 members (1209 → 1254): the tone surface — the sixteen `TONE_DTMF_*` constants, the ten `TONE_SUP_*` cadences, the five `TONE_PROP_*` tones, the eight `STREAM_*` types, `MAX_SEQUENCE_LENGTH`, and `startTone` / `startToneSequence` / `stopTone` / `release` / `nativeInit`. Member floor unchanged (0.17.0). |
| `v0.26.0.toml` | + 4 classes (261 → 265): `java/util/IllegalFormatConversionException`, `IllegalFormatPrecisionException`, `MissingFormatArgumentException` and `UnknownFormatConversionException`; + 12 members (1254 → 1266): the released-view API (`isReleased`, `checkNotReleased`, `mChildren`, `mChildCount`, `nativeAddView`, `nativeRemoveView`, `nativeRemoveAllViews`), the radio-group sync (`mGroup`, `setCheckedSilently`, `onButtonChecked`, `nativeSetChecked`) and `writeInPlace`. Member floor unchanged (0.17.0). |
| `v0.27.0.toml` | + 5 classes (265 → 270): `picodroid/os/Bundle`, `picodroid/content/res/Resources`, `Resources$NotFoundException`, `picodroid/view/LayoutInflater` and `picodroid/view/InflateException`; + 44 members (1266 → 1310): the saved-instance-state surface (`getBundle` / `getByteArray` / `getIntArray` / `getStringArray` / `getExtras` / `putAll` / `putBundle` / `putByteArray` / `putDouble` / `putExtras` / `putIntArray` / `putStringArray` / `extras` / `vals`, `onSaveInstanceState` / `onRestoreInstanceState`, `performCreate` / `performSaveInstanceState` / `performRestoreInstanceState`, `recreate`, `getBundleExtra` / `getLongExtra`), the resource lookups (`getResources` / `getColor` / `getInteger` / `getDimension` / `getDimensionPixelOffset` / `getDimensionPixelSize`, `sInstance` / `string` / `from`) and the inflater (`inflate` / `getContext` / `getLayoutInflater` / `mContentView` / `mContext` / `mLayout` / `mPos` / `node` / `nativeWord` / `truncateAt`, `findViewById` / `findViewTraversal`, `setImageResource`). Member floor unchanged (0.17.0). |
| `v0.29.0.toml` | + 27 classes (274 → 301): `java/time/DateTimeException`, `DayOfWeek`, `Duration`, `Instant`, `LocalDate`, `LocalDateTime`, `LocalTime`, `Month`, `Year`, `ZoneId` (+ `ZoneId$Fixed`), `ZoneOffset`, `chrono/ChronoLocalDate`, `chrono/ChronoLocalDateTime`, `format/DateTimeFormatter`, `temporal/ChronoUnit`, `temporal/Temporal`, `temporal/TemporalAccessor`, `temporal/TemporalUnit`, `zone/ZoneRules` and `java/util/TimeZone`, the four `$1` switch-map classes javac made for `ChronoUnit` switches, and the name-only `temporal/TemporalAmount` / `temporal/TemporalQuery` the contract lists; + 227 members (1402 → 1629): the `Activity` key path (`onKeyDown` / `onKeyUp` / `performKeyDown` / `performKeyUp`, `KeyEvent.startTracking` / `isTracking` / `tracking`), `TextView.setGravity` / `getGravity` / `mGravity` / `nativeSetGravity` and the `Gravity` masks, `Math.floorDiv` / `floorMod` / `addExact` / `subtractExact` / `multiplyExact` / `toIntExact`, and the whole `java.time` surface. Member floor unchanged (0.17.0). |
| `v0.30.0.toml` | + 1 class (301 → 302): `picodroid/view/ViewParent`; + 3 members (1629 → 1632): `mParent`, `detachChild` and `nativeClose` (the parent back-pointer and the detaching `View.close()`). Member floor unchanged (0.17.0). |
| `v0.31.0.toml` | + 6 classes (302 → 308): `picodroid/graphics/Canvas`, `Paint`, `Paint$Align`, `Paint$Cap`, `Paint$Style` and `picodroid/view/View$DrawTask`; + 54 members (1632 → 1686): the custom-drawing surface (`onDraw` / `invalidate` / `postInvalidate` / `performDraw` / `scheduleDraw`, the seven `Canvas.draw*` calls and their `native*` bindings, the `Paint` accessors and constants) and `DatagramSocket.setBroadcast` / `getBroadcast` / `broadcast`. Member floor unchanged (0.17.0). |
| `v0.32.0.toml` | + 11 classes (308 → 319): `java/util/zip/CRC32` and `Checksum`, `picodroid/concurrent/ScheduledExecutorService`, `ScheduledFuture`, `MainScheduledExecutor` and `ScheduledFutureTask`, `picodroid/net/ConnectivityManager$NetworkCallback`, `Network`, `NetworkCapabilities`, `NetworkRequest` and `NetworkRequest$Builder`; + 76 members (1686 → 1762): the CRC32 step (`updateBytes` / `crc`), the scheduler surface (`newSingleThreadScheduledExecutor`, `schedule*`, `getDelay` / `isPeriodic` / `runAndReset`, its `*0` bindings) and the connectivity surface (`register*NetworkCallback` / `requestNetwork` / `unregisterNetworkCallback`, the `NetworkCallback` hooks, the `NET_CAPABILITY_*` / `TRANSPORT_*` constants). Member floor unchanged (0.17.0). |
| `v0.33.0.toml` | + 9 classes (319 → 328): `picodroid/protobuf/CodedInputStream`, `CodedOutputStream`, `CodedOutputStream$OutOfSpaceException`, `InvalidProtocolBufferException`, `MessageLite` and `WireFormat`; `picodroid/view/KeyEvent$Callback`, `KeyEvent$DispatcherState` and `ViewConfiguration`; + 176 members (1762 → 1938): the protobuf streams (`readTag` / `readInt64` / `readString` / `readBytes` / … / `skipField` / `skipMessage`, `pushLimit` / `popLimit` / `isAtEnd` / `getBytesUntilLimit`, the `write*` / `write*NoTag` and `compute*Size` families, `writeRawVarint32` / `writeRawVarint64`, `encodeZigZag*` / `decodeZigZag*`, `getSerializedSize` / `writeTo` / `toByteArray`, the `WIRETYPE_*` constants, `makeTag` / `getTagWireType` / `getTagFieldNumber`, the exception factories and the `native*` codec bindings) and the key path (`dispatch`, `onKeyLongPress` / `onKeyMultiple` / `performKeyEvent` / `handleUpEvent`, `getRepeatCount` / `isLongPress` / `isCanceled` / `getDownTime`, the `FLAG_LONG_PRESS` / `FLAG_CANCELED` / `FLAG_CANCELED_LONG_PRESS` / `FLAG_TRACKING` / `FLAG_START_TRACKING` flags, `getLongPressTimeout` / `getKeyRepeatTimeout` / `getKeyRepeatDelay`, `mKeyDispatchState`). Member floor unchanged (0.17.0). |
| `v0.34.0.toml` | + 7 classes (328 → 335): `picodroid/net/wifi/ScanResult`, `SupplicantState`, `WifiConfiguration`, `WifiConfiguration$Status`, `WifiInfo`, `WifiManager` and `WifiManager$ScanResultsCallback`; + 86 members (1938 → 2024): the Wi-Fi surface (`WIFI_SERVICE`, `isWifiEnabled` / `setWifiEnabled` / `getWifiState`, `startScan` / `getScanResults` / `registerScanResultsCallback` / `unregisterScanResultsCallback` / `onScanResultsAvailable`, `getConnectionInfo` / `getConfiguredNetworks` / `addNetwork` / `updateNetwork` / `enableNetwork` / `removeNetwork` / `reconnect` / `reassociate`, `calculateSignalLevel` / `compareSignalLevel`, the `WifiInfo` getters (`getSSID` / `getBSSID` / `getRssi` / `getNetworkId` / `getSupplicantState`), the `ScanResult` and `WifiConfiguration` fields (`SSID` / `BSSID` / `level` / `frequency` / `preSharedKey` / `hiddenSSID` / `networkId`), the `SupplicantState` and `Status` constants and the `native*` link bindings) and the password input types (`TYPE_TEXT_VARIATION_WEB_PASSWORD` / `TYPE_TEXT_VARIATION_VISIBLE_PASSWORD` / `TYPE_NUMBER_VARIATION_PASSWORD`). Member floor unchanged (0.17.0). |
| `v0.35.0.toml` | + 13 classes (335 → 348): `picodroid/app/Fragment`, `FragmentFactory`, `FragmentManager`, `FragmentManager$OnBackStackChangedListener`, `FragmentTransaction`, `picodroid/widget/ViewPager2`, `ViewPager2$OnPageChangeCallback`, `FragmentStateAdapter`, `picodroid/net/ssl/HttpsURLConnection`, `picodroid/net/SntpClient` and `javax/net/ssl/SSLException` / `SSLHandshakeException` / `SSLPeerUnverifiedException`; + 233 members (2024 → 2257): the Fragment / `FragmentManager` / `FragmentTransaction` lifecycle and transaction surface, `ViewPager2` and its adapter and callback, `HttpsURLConnection` and `SntpClient`. The v0.35.0 release. Member floor unchanged (0.17.0). |
| `v0.28.0.toml` | + 4 classes (270 → 274): `picodroid/util/TypedValue`, `picodroid/util/DisplayMetrics`, `picodroid/content/res/ColorStateList` and `picodroid/widget/CircularProgressIndicator`; + 92 members (1310 → 1402): the text-size surface (`setTextSize` / `getTextSize` / `getLineHeight` / `mTextSize` / `nativeSetTextSize` / `nativeGetLineHeight` / `hasLineMode` / `FONT_PAD_OFF` / `DEFAULT_TEXT_SIZE`, the six `COMPLEX_UNIT_*` constants, `applyDimension`, `getDisplayMetrics` / `mMetrics` / `setToDefaults`, `density` / `densityDpi` / `scaledDensity` / `xdpi` / `ydpi` / `widthPixels` / `heightPixels` / `DENSITY_DEFAULT`), the `ProgressBar` range and tints (`getMax` / `getMin` / `setMin` / `incrementProgressBy` / `setProgressInternal` / `nativeSetRange` / `nativeSetTint` / `applyTint`, the three `set*TintList` / `get*TintList` pairs and their `progressTint` / `progressBackgroundTint` / `indeterminateTint` fields, `ColorStateList`'s `getDefaultColor` / `getColorForState` / `isStateful` / `withAlpha` / `color`), the ring gauge (`set`/`get` `IndicatorColor` / `TrackColor` / `TrackThickness` / `IndicatorSize` / `IndicatorDirection` / `TrackCornerRadius` / `StartAngle` / `SweepAngle`, their fields and `native*` bindings, `pushAngles` / `nativeSetAngles`, the `DEFAULT_*` sizes and `INDICATOR_DIRECTION_*`) and the Activity callback trampolines (`performStart` / `performResume` / `performRestart` / `performPause` / `performStop` / `performDestroy` / `performBackPressed` / `performActivityResult`). Member floor unchanged (0.17.0). |

See [`reference/shrinker`](https://shivrajora.github.io/picodroid-rs/reference/shrinker/) for the full design and per-release detail.
