// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Tag/versionCode contract shared by the direct-distribution release process. */
public final class ReleaseVersion {
    public static int code(String tag) {
        if(tag==null)return -1;
        java.util.regex.Matcher m=java.util.regex.Pattern.compile("v?(\\d{1,2})\\.(\\d{1,2})\\.(\\d{1,2})(?:-preview\\.([1-9][0-9]?))?").matcher(tag);
        if(!m.matches())return -1;
        long value=Long.parseLong(m.group(1))*10000000L+Long.parseLong(m.group(2))*100000L
            +Long.parseLong(m.group(3))*1000L+(m.group(4)==null?999:Integer.parseInt(m.group(4)));
        return value>0 && value<=2100000000L?(int)value:-1;
    }
    private ReleaseVersion(){}
}
