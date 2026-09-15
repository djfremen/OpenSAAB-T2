// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.app.ActivityManager;
import android.app.AlertDialog;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.StatFs;
import android.util.DisplayMetrics;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;
import java.util.Arrays;
import java.util.Locale;

/** Local-only assessment. No vehicle access, network request, or device identifier collection. */
public final class DeviceCompatibility {
    public static String report(Context context) {
        StatFs storage = new StatFs(context.getFilesDir().getAbsolutePath());
        long free = storage.getAvailableBytes();
        String requiredAbi = AppBuildProfile.abi(context);
        boolean supportedAbi = CompatibilityCheck.supportsAbi(Build.SUPPORTED_ABIS, requiredAbi);
        boolean api = Build.VERSION.SDK_INT >= 26;
        ActivityManager.MemoryInfo memory = new ActivityManager.MemoryInfo();
        ((ActivityManager)context.getSystemService(Context.ACTIVITY_SERVICE)).getMemoryInfo(memory);
        DisplayMetrics display = context.getResources().getDisplayMetrics();
        boolean usb = context.getPackageManager().hasSystemFeature(PackageManager.FEATURE_USB_HOST);
        StringBuilder text = new StringBuilder("OpenSAAB system check\n\n");
        text.append(CompatibilityCheck.verdict(Build.VERSION.SDK_INT, Build.SUPPORTED_ABIS, free, requiredAbi));
        text.append("\nBuild profile: ").append(AppBuildProfile.name(context));
        text.append("\n\nDevice: ").append(Build.MANUFACTURER).append(' ').append(Build.MODEL);
        text.append("\nAndroid: ").append(Build.VERSION.RELEASE).append(" · API ").append(Build.VERSION.SDK_INT);
        text.append("\nAndroid application architectures: ").append(Arrays.toString(Build.SUPPORTED_ABIS));
        text.append("\n\n").append(api ? "PASS" : "FAIL").append(" — Android 8.0 / API 26 or newer");
        text.append("\n").append(supportedAbi ? "PASS" : "FAIL").append(" — Android supports ").append(requiredAbi).append(" applications");
        if (!supportedAbi) text.append("\nThis APK requires ").append(requiredAbi).append(" Android application support. Retrying this APK will not fix the mismatch. A compatible build or device is needed.");
        if (!api) text.append("\nThe Android API reported by this system is below the app minimum, regardless of the version advertised by the seller.");
        text.append(String.format(Locale.ROOT,"\n%s — %.0f MiB free in app storage; allow at least 140 MiB for initial setup", free >= CompatibilityCheck.SETUP_BYTES ? "PASS" : "LOW SPACE", free / 1048576.0));
        text.append(String.format(Locale.ROOT,"\nRAM reported: %.1f GiB", memory.totalMem / 1073741824.0));
        if (memory.totalMem < 2L * 1024 * 1024 * 1024) text.append("\nLow-memory device: emulator performance and stability need testing. This is not an installation block.");
        text.append("\nUSB host advertised by Android: ").append(usb ? "Yes — adapter/cable still need testing" : "No — direct USB diagnostics may be unavailable; emulation mode does not need USB");
        text.append("\nApp display: ").append(display.widthPixels).append(" × ").append(display.heightPixels).append(" px; density ").append(display.densityDpi).append(" dpi");
        text.append("\n\nThis checks requirements for the selected APK architecture, not complete vehicle compatibility. It does not test the USB adapter, firmware, server connection or available memory under load.");
        text.append("\n\nInternet is needed for software downloads and security access. Installed firmware and local USB diagnostics can work offline.");
        text.append("\n\nThis report stays on this device unless you copy and share it. No VIN, serial number or account information is collected.");
        return text.toString();
    }
    public static void copy(Context context, String report) {
        ((ClipboardManager)context.getSystemService(Context.CLIPBOARD_SERVICE)).setPrimaryClip(ClipData.newPlainText("OpenSAAB system check", report));
        Toast.makeText(context,"System report copied",Toast.LENGTH_SHORT).show();
    }
    public static void show(Activity activity) {
        String report = report(activity);
        TextView text = new TextView(activity);text.setText(report);text.setTextIsSelectable(true);text.setTextSize(16);
        int pad = Math.round(20 * activity.getResources().getDisplayMetrics().density);text.setPadding(pad,pad,pad,pad);
        ScrollView scroll = new ScrollView(activity);scroll.addView(text);
        new AlertDialog.Builder(activity).setTitle("Device compatibility").setView(scroll)
            .setPositiveButton("Close",null).setNeutralButton("Copy report",(d,w)->copy(activity,report)).show();
    }
    private DeviceCompatibility() {}
}
