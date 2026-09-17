package com.opensaab.loadtest;

import android.app.Activity;
import android.os.Bundle;
import android.os.Handler;
import android.os.SystemClock;
import android.graphics.Color;
import android.graphics.Typeface;
import android.view.WindowManager;
import android.widget.ScrollView;
import android.widget.TextView;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.zip.CRC32;
import java.util.Locale;
import org.json.JSONObject;

/** Isolated offline cold-boot benchmark. No adapter, network, or input controls. */
public final class MainActivity extends Activity {
    private final Handler ui = new Handler();
    private final StringBuilder visible = new StringBuilder();
    private boolean dirty;
    private volatile boolean finished;
    private TextView console;
    private ScrollView scroll;
    private volatile Process child;
    private volatile boolean cancelled;
    private long started;
    private File session;
    private final Runnable refresh = new Runnable() {
        public void run() {
            synchronized (visible) {
                if (dirty) {
                    console.setText(visible.toString());
                    dirty = false;
                    scroll.post(() -> scroll.fullScroll(ScrollView.FOCUS_DOWN));
                }
            }
            if (!cancelled && !finished) ui.postDelayed(this, 250);
        }
    };
    @Override public void onCreate(Bundle saved) {
        super.onCreate(saved);
        long entered = SystemClock.elapsedRealtime();
        long origin = getIntent().getLongExtra("launch_origin_elapsed_ms", entered);
        started = origin > 0 && origin <= entered && entered - origin < 10000 ? origin : entered;
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        console = new TextView(this);
        console.setTextColor(Color.rgb(170, 255, 180));
        console.setBackgroundColor(Color.BLACK);
        console.setTypeface(Typeface.MONOSPACE);
        console.setTextSize(13);
        console.setPadding(12, 12, 12, 12);
        scroll = new ScrollView(this);
        scroll.setBackgroundColor(Color.BLACK);
        scroll.addView(console);
        setContentView(scroll);
        append("32-bit load test — cold firmware boot\nTarget: complete Saab 9.250 welcome screen <= 10.000 s\nOffline · no CANdi/adapter · no cached emulator state\n");
        ui.post(refresh);
        new Thread(this::runTest, "load-test").start();
    }
    private void append(String text) {
        synchronized (visible) {
            dirty = true;
            visible.append(text).append('\n');
            if (visible.length() > 8000) visible.delete(0, visible.length() - 7000);
        }
    }
    private void runTest() {
        try {
            Files.deleteIfExists(new File(getFilesDir(), "latest-result.json").toPath());
            File firmware = new File(getFilesDir(), "firmware");
            firmware.mkdirs();
            JSONObject manifest = new JSONObject(new String(readAsset("firmware.json"), StandardCharsets.UTF_8));
            for (String name: new String[]{"eprom.bin", "opsys.dwn", "card.bin"}) {
                JSONObject spec = manifest.getJSONObject(name);
                File target = new File(firmware, name);
                if (!target.isFile() || target.length() != spec.getLong("bytes")) {
                    File tmp = new File(firmware, name + ".tmp");
                    // The APK signature authenticates bundled assets; CRC checks extraction.
                    CRC32 digest = new CRC32();
                    try (InputStream in = getAssets().open("firmware/" + name); OutputStream out = new FileOutputStream(tmp)) {
                        byte[] buffer = new byte[65536]; int n;
                        while ((n = in.read(buffer)) != -1) { out.write(buffer, 0, n); digest.update(buffer, 0, n); }
                    }
                    if (tmp.length() != spec.getLong("bytes") || digest.getValue() != spec.getLong("crc32")) throw new IOException("Firmware checksum failed: " + name);
                    Files.move(tmp.toPath(), target.toPath(), java.nio.file.StandardCopyOption.REPLACE_EXISTING);
                }
            }
            if (cancelled) return;
            long prepared = SystemClock.elapsedRealtime() - started;
            session = new File(getFilesDir(), "runs/" + System.currentTimeMillis());
            if (!session.mkdirs()) throw new IOException("Cannot create run directory");
            append("Firmware ready at " + prepared + " ms. Starting ARMv7 emulator.");
            ProcessBuilder builder = new ProcessBuilder(getApplicationInfo().nativeLibraryDir + "/libloadtest.so",
                "--headless", "--research-harness",
                "--boot", new File(firmware,"eprom.bin").toString(), "--opsys", new File(firmware,"opsys.dwn").toString(),
                "--max-insns", "100000000", "--output-dir", session.toString(), new File(firmware,"card.bin").toString());
            builder.directory(session).redirectErrorStream(true);
            builder.environment().put("OPENSAAB_PERFORMANCE_DIR", session.toString());
            builder.environment().put("LOAD_TEST_ACCELERATE", "1");
            long nativeStart = SystemClock.elapsedRealtime();
            child = builder.start();
            if (cancelled) { child.destroy(); return; }
            long welcome = -1;
            try (BufferedReader reader = new BufferedReader(new InputStreamReader(child.getInputStream()));
                 PrintWriter log = new PrintWriter(new File(session,"console.log"))) {
                String line;
                while ((line = reader.readLine()) != null) {
                    line = line.replaceAll("\u001B\\[[;\\d]*m", "");
                    log.println(line);
                    if (line.contains("HEARTBEAT") || line.contains("SCREEN") || line.contains("LOAD_TEST_WELCOME") || line.contains("ERROR") || line.contains("VERDICT") || line.contains("REPORT")) append(line);
                    if (welcome < 0 && (line.contains("SCREEN") || line.contains("LOAD_TEST_WELCOME")) && line.contains("9.250") && line.contains("North American Operations")) welcome = SystemClock.elapsedRealtime() - started;
                }
            }
            int exit = child.waitFor();
            long finished = SystemClock.elapsedRealtime() - started;
            String screen = new File(session,"screen.txt").isFile() ? new String(Files.readAllBytes(new File(session,"screen.txt").toPath()), StandardCharsets.UTF_8) : "";
            JSONObject report = new JSONObject(new String(Files.readAllBytes(new File(session,"report.json").toPath()), StandardCharsets.UTF_8));
            boolean verified = welcome >= 0 && screen.contains("9.250") && screen.contains("North American Operations") && exit == 0 && "success".equals(report.optString("status")) && "splash".equals(report.optString("last_verified"));
            boolean pass = verified && welcome <= 10000;
            JSONObject result = new JSONObject();
            result.put("run_id", session.getName()).put("timing_origin", getIntent().hasExtra("launch_origin_elapsed_ms") ? "before_activity_launch" : "activity_onCreate").put("cold_boot",true).put("verified_welcome",verified).put("goal_met",pass).put("launch_to_welcome_ms",welcome)
                .put("firmware_preparation_ms",prepared).put("native_start_ms",nativeStart-started).put("total_ms",finished).put("exit_code",exit);
            Files.write(new File(session,"load-result.json").toPath(), result.toString(2).getBytes(StandardCharsets.UTF_8));
            Files.write(new File(getFilesDir(),"latest-result.json").toPath(), result.toString(2).getBytes(StandardCharsets.UTF_8));
            append("\n" + screen.trim() + "\n\n" + (pass ? "PASS" : "NOT YET") + " — " + (verified ? String.format(Locale.ROOT,"%.3f seconds to verified welcome",welcome/1000.0) : "welcome not verified") + "\nTarget <= 10.000 seconds\n" + result.toString(2));
        } catch (Exception error) { append("ERROR: " + error); }
        finally { finished = true; ui.post(refresh); }
    }
    private byte[] readAsset(String name) throws IOException {
        try (InputStream in = getAssets().open(name); ByteArrayOutputStream out = new ByteArrayOutputStream()) {
            byte[] b = new byte[4096]; int n; while ((n=in.read(b))!=-1) out.write(b,0,n); return out.toByteArray();
        }
    }
    @Override protected void onDestroy() {
        cancelled = true;
        ui.removeCallbacks(refresh);
        if (child != null) child.destroy();
        super.onDestroy();
    }
}
