// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Fixed diagnostic labels only. Never return a substring of a log or exception. */
public final class DiagnosticFailure {
    public static String classify(String text) {
        if(text==null)return "unclassified";
        if(text.contains("Unvalidated Chipsoft firmware version"))return "adapter_firmware_not_validated";
        if(text.contains("App not in Chipsoft native mode"))return "native_mode_mismatch";
        if(text.contains("Unverified Chipsoft CDC layout"))return "usb_layout_not_validated";
        if(text.contains("USB open denied"))return "usb_open_denied";
        if(text.contains("USB interface claim failed"))return "usb_interface_claim_failed";
        if(text.contains("Adapter did not settle"))return "usb_startup_drain_failed";
        if(text.contains("Session authentication failed"))return "local_bridge_auth_failed";
        if(text.contains("Incomplete USB write"))return "usb_write_incomplete";
        if(text.contains("USB request failed/detached"))return "usb_read_detached";
        if(text.contains("Controller disconnected"))return "local_bridge_disconnected";
        if(text.contains("Chipsoft command gate rejected request")||text.contains("Chipsoft command guard blocked"))return "command_policy_rejected";
        if(text.contains("Missing original firmware:")||text.contains("Firmware setup needed:"))return "firmware_missing";
        if(text.contains("Chipsoft opcode="))return "adapter_status_error";
        if(text.contains("Chipsoft deadline")||text.contains("Chipsoft identification deadline expired"))return "adapter_deadline";
        if(text.contains("Chipsoft stream requires reopening"))return "adapter_reopen_required";
        if(text.contains("Chipsoft native session expired"))return "session_time_limit";
        if(text.contains("Chipsoft cleanup incomplete"))return "adapter_cleanup_incomplete";
        if(text.contains("Chipsoft startup failed"))return "adapter_startup_failed";
        return "unclassified";
    }
    private DiagnosticFailure(){}
}
