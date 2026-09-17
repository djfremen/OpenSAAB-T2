// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.nio.charset.StandardCharsets;
import java.util.*;

/** Same binary contract as scripts/process_ssa.py; no vehicle or network operations. */
public final class SsaData {
    public static final int SIZE=714, OFFSET=0xfe0000;
    private static int word(byte[] b,int p){return (b[p]&255)*256+(b[p+1]&255);}
    public static String vin(byte[] b){
        if(b.length!=SIZE)throw new IllegalArgumentException("Expected 714-byte SSA data");
        String vin=new String(b,0x14,17,StandardCharsets.US_ASCII);
        if(!vin.matches("[A-HJ-NPR-Z0-9]{17}"))throw new IllegalArgumentException("Invalid SSA VIN");
        return vin;
    }
    private static List<Integer> records(byte[] b){
        List<Integer> rows=new ArrayList<>();
        for(int p=0x132;p<=SIZE-8;p+=8){
            boolean empty=true;for(int i=p;i<p+8;i++)empty&=b[i]==(byte)255;
            if(empty)continue;
            if(word(b,p)==65535 || (word(b,p+2)>>8)!=3 || word(b,p+4)==65535)
                throw new IllegalArgumentException("Invalid SSA seed record");
            rows.add(p);
        }
        return rows;
    }
    public static int validateInput(byte[] b){
        vin(b);
        if((b[0]&255)!=0xb1)throw new IllegalArgumentException("Invalid SSA header");
        for(int p=0x26;p<0x2e;p++)if(b[p]!=(byte)255)throw new IllegalArgumentException("Collect fresh security data first");
        List<Integer> rows=records(b);
        if(rows.isEmpty())throw new IllegalArgumentException("No collected seeds in SSA");
        for(int p:rows)if(word(b,p+6)!=65535)throw new IllegalArgumentException("SSA already contains keys");
        return rows.size();
    }
    public static void validateReply(byte[] before,byte[] after){
        validateInput(before);
        if(after.length!=SIZE || (after[0]&255)!=0xb1 || after[1]!=0)throw new IllegalArgumentException("Invalid processed SSA header/size");
        if(!vin(before).equals(vin(after)))throw new IllegalArgumentException("API changed VIN");
        for(int p=0x26;p<0x2e;p++)if((after[p]&255)<32 || (after[p]&255)>126)throw new IllegalArgumentException("API security code incomplete");
        List<Integer> old=records(before),now=records(after);
        if(!old.equals(now))throw new IllegalArgumentException("API changed seed record positions");
        boolean[] allowed=new boolean[SIZE];Arrays.fill(allowed,0,0x14,true);allowed[0x25]=true;Arrays.fill(allowed,0x26,0x2e,true);
        for(int p:old){
            for(int i=p;i<p+6;i++)if(before[i]!=after[i])throw new IllegalArgumentException("API changed a seed or algorithm");
            if(word(after,p+6)==65535)throw new IllegalArgumentException("API returned an unfilled key");
            allowed[p+6]=allowed[p+7]=true;
        }
        for(int i=0;i<SIZE;i++)if(before[i]!=after[i]&&!allowed[i])throw new IllegalArgumentException("API changed unrelated SSA data");
    }
    /** Actual collection progress, not a menu item or a generic access-required help page. */
    public static boolean collectingAccess(String text){
        String s=text.replaceAll("\\s+"," ").toLowerCase(Locale.ROOT);
        return s.contains("checking security access")&&s.contains("reading all vehicle vins")
            &&s.contains("reading all vehicle seed");
    }
    public static boolean needsAccess(String text){
        String s=text.replaceAll("\\s+"," ").toLowerCase(Locale.ROOT);
        return s.contains("you need security access from tis2000") && s.contains("disconnect tech 2 from vehicle");
    }
}
