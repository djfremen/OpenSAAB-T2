// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
public final class DiagnosticFailureTest {
    static void check(boolean ok){if(!ok)throw new AssertionError();}
    public static void main(String[] args){
        check(DiagnosticFailure.classify("ERROR: Unvalidated Chipsoft firmware version").equals("adapter_firmware_not_validated"));
        check(DiagnosticFailure.classify("ERROR: Chipsoft opcode=000F status=001D private-data").equals("adapter_status_error"));
        check(DiagnosticFailure.classify("ERROR: Missing original firmware: /private/secret").equals("firmware_missing"));
        check(DiagnosticFailure.classify("USB_OPEN private-data").equals("unclassified"));
        check(DiagnosticFailure.classify(null).equals("unclassified"));
        check(DiagnosticFailure.classify("ERROR secret password arbitrary").equals("unclassified"));
        for(String suffix:new String[]{"\nsecret"," /private/path"," VIN=TEST_VALUE"," token=TEST_VALUE"})
            check(DiagnosticFailure.classify("USB interface claim failed"+suffix).equals("usb_interface_claim_failed"));
        System.out.println("PASS: fixed failure taxonomy distinguishes firmware validation, adapter status and missing files; no raw values exported");
    }
}
