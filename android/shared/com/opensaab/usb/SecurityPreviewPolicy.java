// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Preview behavior is selected by matching installed and packaged identity. */
public final class SecurityPreviewPolicy {
    public static boolean noPrompt(String installed,String packaged,String channel,String mode) {
        return installed!=null && installed.equals(packaged)
            && installed.matches("\\d+\\.\\d+\\.\\d+-[A-Za-z0-9.-]+\\.[1-9][0-9]*")
            && "developer_preview".equals(channel) && "preview_no_prompt".equals(mode);
    }
    private SecurityPreviewPolicy(){}
}
