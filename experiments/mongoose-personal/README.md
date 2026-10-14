# OpenSAAB personal Mongoose Developer Preview 0.1.1-preview.3 ARM64

This separate Android test package uses the existing Mongoose beta namespace and official signing certificate so saved reports survive an in-place upgrade. It launches the required Developer Preview welcome, then only the Mongoose connection tests. It contains no firmware/card, general diagnostic backend, security processing, processed-response import, DTC clearing or programming.

The I-Bus comparison follows the October 14 native Tech2Win captures: paired ISO1801/raw1701 at 33333 bit/s/pin1, exact captured filters, optional one flag0x400 high-voltage-wake request, functional tester-present, one CIM AA0101 status request, one CIM1A90 VIN request and a20 stop. The successful captured firmware1.2.8.0/bootloader1.1.8.0 profile remains mandatory. No arbitrary commands or retries are admitted by the Android physical USB gate.

A pass requires matching adapter acceptance and route/sequence-correlated completion, fresh CIM status and ordered complete VIN, then confirmed cleanup and actual Android USB release. The control omits HV wake but still sends normal communication traffic; it cannot prove a sleeping bus stayed asleep. Zero/intermediate ignition tuples are unknown. Prior sleep, engine running and electrical wake waveform are never inferred. Reports may contain a completely validated VIN; review the unique ZIP before choosing a share recipient. No automatic upload occurs.

Physical contributor testing is pending. Native USB captures show the recipe, but this developer has no Mongoose hardware. Simulated responses and emulator tests qualify software logic only; paired raw-channel lifetime and physical sleeping-vehicle wake remain unqualified. Other platforms have no inherited qualification. Existing beta.1/beta.2 and their evidence remain preserved.

## Rebuild

Use JDK supplied with Android Studio, Android SDK build-tools36.0.0/platform36, Rust1.98.0, aarch64-linux-android target and Android NDKr29. The minimum runtime is Android8/API26, ARM64. Dependencies are locked. Set ANDROID_HOME/JAVA_HOME/ANDROID_NDK_HOME when their locations differ from the script defaults. For signing, set OPENSAAB_SIGNING_HOME to your own keystore directory containing opensaab-release.p12/password.txt (alias opensaab-release); official release credentials are never distributed. Your own certificate cannot upgrade an official installation.

```sh
python3 experiments/mongoose-personal/build.py --output /absolute/outside-source/bundle
cargo test --locked --manifest-path experiments/mongoose-personal/worker/Cargo.toml --lib
python3 experiments/mongoose-personal/tests/build_tests.py --app-classes /absolute/outside-source/bundle/build/classes --output /absolute/test-output
adb -s YOUR_OWN_TEST_DEVICE install -r /absolute/outside-source/bundle/OpenSAAB_64_14OCT26_v0.1.1-preview.3.apk
adb -s YOUR_OWN_TEST_DEVICE install -r /absolute/test-output/installed-tests.apk
adb -s YOUR_OWN_TEST_DEVICE shell am instrument -w com.opensaab.mongoosepersonaltests/com.opensaab.mongoosepersonaltests.MongoosePersonalInstrumentation
```

Gitless matching-source rebuilds must supply --source-revision with the full commit in BUILD-RECEIPT.json. build.py embeds that identity, full version, release date, architecture, worker hash and all input hashes; it builds a nondebuggable APK. It uses local IPC for native worker USB effects. There is no proprietary Windows-driver dependency.

The ignored PCAP replay tests additionally require original private captures through OPENSAAB_MONGOOSE_CAPTURE and OPENSAAB_MONGOOSE_BODY_ON/OFF; these inputs and the contributor's VIN are intentionally absent from the distributed source. The source tests use a synthetic VIN. The installed instrumentation runs the actual packaged native worker and Android gate against explicitly simulated USB, as well as welcome, installed metadata, no-adapter reports, read-only sharing and unavailable security routes. This companion is a test artifact, not the user APK.

The canonical interface contract is frozen in interface-contract.json. Main OpenSAAB preview channels, updater recommendations and other adapters are separate and are not replaced by this personal probe.
