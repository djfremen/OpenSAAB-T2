// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import org.json.JSONObject;

/** OS bytes/effects only; vehicle protocol decisions belong to the pinned Rust core. */
interface VlinkerTransport {
    void open() throws Exception;
    void send(JSONObject request) throws Exception;
    JSONObject receive(int timeoutMs) throws Exception;
    void cancelPendingIO();
    void shutdown();
}
