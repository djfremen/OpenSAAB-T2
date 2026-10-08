// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Nano result-to-session boundary; no device, provider, card import or raw security data. */
public final class NanoSecurityModeTest {
    static int checks;
    static void check(boolean v){checks++;if(!v)throw new AssertionError("check "+checks);}
    static boolean ready(String status,String origin,String requestOrigin,boolean[] p,long dh,long tx){
        return NativeCommandGate.securityVinReady(status,origin,requestOrigin,p[0],p[1],p[2],dh,tx,p[3],p[4],p[5]);
    }
    public static void main(String[] args){
        boolean[] valid={true,true,true,true,true,true};
        check(ready("hs_vin_received","android-direct-usb","host-transport-probe",valid,2,2));
        for(int i=0;i<valid.length;i++){
            boolean[] b=valid.clone();b[i]=false;
            check(!ready("hs_vin_received","android-direct-usb","host-transport-probe",b,2,2));
        }
        for(String s:new String[]{"failed","hs_vin_no_reply","vin_received","channels_receive_test_completed","",null})
            check(!ready(s,"android-direct-usb","host-transport-probe",valid,2,2));
        for(String s:new String[]{"replay","original-guest-memory","host-startup-discovery","",null}){
            check(!ready("hs_vin_received",s,"host-transport-probe",valid,2,2));
            check(!ready("hs_vin_received","android-direct-usb",s,valid,2,2));
        }
        for(long n:new long[]{-1,0,1,3,Long.MAX_VALUE}){
            check(!ready("hs_vin_received","android-direct-usb","host-transport-probe",valid,n,2));
            check(!ready("hs_vin_received","android-direct-usb","host-transport-probe",valid,2,n));
        }
        // A VIN-first proof does not expand the seed profile into security-key submission.
        byte[] seed=NanoKeyStatusGateTest.wire(NanoKeyStatusGateTest.body(1,0x241,2,0x27,1,0,0,0,0,0));
        byte[] key=NanoKeyStatusGateTest.wire(NanoKeyStatusGateTest.body(1,0x241,4,0x27,2,0x12,0x34,0,0,0));
        check(new NativeCommandGate.Stream().allowed(seed,true,false,1));
        check(!new NativeCommandGate.Stream().allowed(seed,false,false,1));
        check(!new NativeCommandGate.Stream().allowed(key,true,false,1));
        check(!new NativeCommandGate.Stream().allowed(key,false,false,1));
        check(NativeCommandGate.fullNative(key));
        check(NativeCommandGate.fullNativeModeAllowed(true,false,false));
        check(!NativeCommandGate.fullNativeModeAllowed(false,false,false));
        check(!NativeCommandGate.fullNativeModeAllowed(true,true,false));
        check(!NativeCommandGate.fullNativeModeAllowed(true,false,true));
        check(NativeCommandGate.securityCollectionModeAllowed(false,false));
        check(!NativeCommandGate.securityCollectionModeAllowed(true,false));
        check(!NativeCommandGate.securityCollectionModeAllowed(false,true));
        String observed="Native adapter stopped: Native USB command rejected by collection profile";
        check(NativeCommandGate.collectionRejectedExplanation(observed,true).contains("Get security access"));
        check(NativeCommandGate.collectionRejectedExplanation(observed,false)==null);
        check(NativeCommandGate.collectionRejectedExplanation("ECU response7F2733",true)==null);
        check(NativeCommandGate.collectionRejectedExplanation("USB timeout",true)==null);
        check(NativeCommandGate.collectionRejectedExplanation(null,true)==null);
        System.out.println("NANO_SECURITY_MODE PASS checks="+checks+"; no device/provider/card effects");
    }
}
