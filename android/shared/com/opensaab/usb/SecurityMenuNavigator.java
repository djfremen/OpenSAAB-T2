// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Security collection uses the same bounded menu navigation as diagnostic shortcuts. */
public final class SecurityMenuNavigator {
    private final FirmwareMenuNavigator delegate;
    public SecurityMenuNavigator(VehicleIdentity vehicle){delegate=new FirmwareMenuNavigator(vehicle,FirmwareMenuNavigator.Target.SECURITY);}
    public boolean active(){return delegate.active();}
    public void cancel(){delegate.cancel();}
    public String hint(){return delegate.hint();}
    public Integer next(String screen,long now){return delegate.next(screen,now);}
    public void sent(long now){delegate.sent(now);}
}
