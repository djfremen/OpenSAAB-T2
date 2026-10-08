// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.util.Arrays;
public final class ChipsoftIdentityTest {
    static final byte[] REQUEST={1,0,0,0,0,0,0,0};
    static void check(boolean value){if(!value)throw new AssertionError();}
    static byte[] reply(String version){byte[] p=("CHIPSOFT J2534 Pro v. "+version).getBytes(java.nio.charset.StandardCharsets.US_ASCII);byte[] b=new byte[p.length+8];b[0]=1;b[2]=(byte)p.length;int sum=0;for(byte v:p)sum+=v&255;b[6]=(byte)sum;b[7]=(byte)(sum>>8);System.arraycopy(p,0,b,8,p.length);return b;}
    public static void main(String[] args){
        byte[] response=reply("1.5.2");
        for(int split=0;split<response.length;split++){
            ChipsoftIdentity p=new ChipsoftIdentity();p.request(REQUEST);
            check(p.receive(Arrays.copyOfRange(response,0,split))==null);
            check("1.5.2".equals(p.receive(Arrays.copyOfRange(response,split,response.length))));
            check(p.receive(response)==null);
        }
        ChipsoftIdentity p=new ChipsoftIdentity();check(p.receive(response)==null);
        p.request(REQUEST);check("1.6.0-beta".equals(p.receive(reply("1.6.0-beta"))));
        p.request(REQUEST);byte[] bad=response.clone();bad[6]++;check(p.receive(bad)==null);
        p.request(REQUEST);bad=response.clone();bad[4]=1;check(p.receive(bad)==null);
        p.request(REQUEST);bad=response.clone();bad[2]=(byte)255;check(p.receive(bad)==null);
        p.request(REQUEST);check(p.receive(reply("1.5.2 private serial"))==null);
        p.request(REQUEST);p.receive(Arrays.copyOf(response,10));p.request(new byte[]{15,0});check(p.receive(response)==null);
        p.request(REQUEST);check("2.0".equals(p.receive(reply("2.0"))));
        System.out.println("PASS: passive firmware identity, fragments, checksum, bounds, reset and unknown versions");
    }
}
