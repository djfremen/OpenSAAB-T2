// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.util.Arrays;
public final class SsaStateTest {
    static void expect(byte[] b,SsaState state){if(SsaState.analyze(b)!=state)throw new AssertionError(state);}
    public static void main(String[] args)throws Exception{
        byte[] b=new byte[714];Arrays.fill(b,(byte)255);expect(b,SsaState.INIT_AUTH);
        b[0x30]=b[0x31]=0;expect(b,SsaState.INIT_AUTH);
        b[0x30]=1;expect(b,SsaState.INVALID);
        System.arraycopy("TESTVEH1CLE0000001".getBytes("US-ASCII"),0,b,0x14,17);expect(b,SsaState.PRE_AUTH);
        Arrays.fill(b,0x26,0x2e,(byte)0);expect(b,SsaState.INVALID);
        System.arraycopy("12345678".getBytes("US-ASCII"),0,b,0x26,8);expect(b,SsaState.POST_AUTH);
        b[0x14]='I';expect(b,SsaState.INVALID);b[0x14]=0;expect(b,SsaState.POST_AUTH);
        expect(null,SsaState.UNAVAILABLE);expect(new byte[713],SsaState.UNAVAILABLE);
        if(!"[PRE-AUTH]".equals(SsaState.PRE_AUTH.label)||!"[POST-AUTH]".equals(SsaState.POST_AUTH.label)||!"[INIT_AUTH]".equals(SsaState.INIT_AUTH.label))throw new AssertionError("NoMoreGlobal labels");
        System.out.println("PASS: NoMoreGlobal state parity: cleared, collected, processed, invalid and missing SSA");
    }
}
