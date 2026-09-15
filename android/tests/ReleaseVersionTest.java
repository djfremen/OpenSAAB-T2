// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
public final class ReleaseVersionTest {
    public static void main(String[] args){
        if(ReleaseVersion.code("v0.1.0-preview.1")!=100001)throw new AssertionError("candidate code");
        if(ReleaseVersion.code("v0.1.0")<=ReleaseVersion.code("v0.1.0-preview.99"))throw new AssertionError("stable ordering");
        if(ReleaseVersion.code("v0.1.1-preview.1")<=ReleaseVersion.code("v0.1.0"))throw new AssertionError("patch ordering");
        for(String s:new String[]{null,"","v0.1.0/evil","v1.0.0-preview.0","v1.0.0-preview.100","v1.2","v1.2.3-beta.1"})
            if(ReleaseVersion.code(s)!=-1)throw new AssertionError("invalid tag accepted");
        System.out.println("PASS: update version ordering and invalid release tags");
    }
}
