// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** One foreground selection may hand off only its verified, released discovery result. */
public final class BluetoothFirmwareHandoff {
    private long generation;
    private boolean pending;
    public long begin(){pending=true;return ++generation;}
    public void cancel(){pending=false;++generation;}
    public boolean current(long attempt){return pending&&attempt==generation;}
    public boolean finish(long attempt,boolean available,boolean ownerReleased,String state,String model,boolean verifiedDiscovery){
        if(!current(attempt))return false;
        pending=false;
        return available&&ownerReleased&&verifiedDiscovery&&"identified".equals(state)&&"obdlink_mxp".equals(model);
    }
}
