// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Common diagnostic guard copy from the reporting/session contract draft. */
public final class DiagnosticPrerequisites {
    public static final String OFFLINE="This action needs a supported adapter and a freshly identified vehicle. Offline menus do not provide a vehicle connection.";
    private DiagnosticPrerequisites(){}
}
