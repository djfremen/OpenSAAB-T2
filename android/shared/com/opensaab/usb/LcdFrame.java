// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;
import java.nio.charset.StandardCharsets;

/** Strict decoder for the emulator's fixed-size P6 framebuffer artifact. */
final class LcdFrame {
    static final int WIDTH = 320, HEIGHT = 240;
    static int[] decode(byte[] bytes) throws IOException {
        byte[] header = "P6\n320 240\n255\n".getBytes(StandardCharsets.US_ASCII);
        if (bytes.length != header.length + WIDTH * HEIGHT * 3)
            throw new IOException("Incomplete LCD frame");
        for (int i = 0; i < header.length; i++)
            if (bytes[i] != header[i]) throw new IOException("Unexpected LCD header");
        int[] pixels = new int[WIDTH * HEIGHT];
        int pos = header.length;
        for (int i = 0; i < pixels.length; i++)
            pixels[i] = 0xff000000 | ((bytes[pos++] & 255) << 16)
                | ((bytes[pos++] & 255) << 8) | (bytes[pos++] & 255);
        return pixels;
    }
}
