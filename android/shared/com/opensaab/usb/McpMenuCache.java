// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.os.FileObserver;
import java.io.*;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;

/** Observational snapshots only. Command admission always requests a fresh read. */
public final class McpMenuCache implements AutoCloseable {
    private File directory;
    private FileObserver observer;
    private String encoded;
    private boolean loaded;
    private final int maxBytes;
    public McpMenuCache(){this(131071);}
    public McpMenuCache(int maxBytes){if(maxBytes<1||maxBytes>131071)throw new IllegalArgumentException("Menu limit");this.maxBytes=maxBytes;}

    public synchronized JSONObject get(File root, boolean fresh) throws Exception {
        if (root == null) { close(); return null; }
        if (!root.equals(directory)) {
            close(); directory = root;
            final File watched = root;
            observer = new FileObserver(root.getAbsolutePath(), FileObserver.MOVED_TO |
                    FileObserver.CLOSE_WRITE | FileObserver.DELETE | FileObserver.DELETE_SELF |
                    FileObserver.MOVE_SELF) {
                public void onEvent(int event, String path) {
                    if (path != null && !path.equals("native-menu.json")) return;
                    synchronized (McpMenuCache.this) {
                        if (!watched.equals(directory)) return;
                        try { load(watched); } catch (Exception ignored) { encoded = null; loaded = true; }
                    }
                    AndroidMcp.changed();
                }
            };
            observer.startWatching();
        }
        if (fresh || !loaded) load(root);
        // Never share a mutable JSONObject across the UI and observer threads.
        return encoded == null ? null : new JSONObject(encoded);
    }

    private void load(File root) throws Exception {
        encoded = null; loaded = true;
        File file = new File(root, "native-menu.json");
        if (!file.isFile()) return;
        try (InputStream in = new FileInputStream(file);
             ByteArrayOutputStream out = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[4096]; int count;
            while ((count = in.read(buffer)) != -1) {
                if (out.size() + count > maxBytes) throw new IOException("Menu snapshot exceeds limit");
                out.write(buffer, 0, count);
            }
            encoded = new JSONObject(out.toString(StandardCharsets.UTF_8.name())).toString();
        }
    }

    public synchronized void close() {
        if (observer != null) observer.stopWatching();
        observer = null; directory = null; encoded = null; loaded = false;
    }
}
