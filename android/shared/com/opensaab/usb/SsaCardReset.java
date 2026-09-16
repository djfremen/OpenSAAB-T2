// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;
import java.util.Arrays;
import java.util.function.BooleanSupplier;

/** NoMoreGlobal Clear Offset: exactly 714 bytes of FF at 0xFE0000, with atomic backup. */
public final class SsaCardReset {
    public static String clear(File card,File evidence,BooleanSupplier allowed)throws Exception{
        if(card.length()!=33554432)throw new IOException("A valid installed 32 MB working card is required.");
        if(!allowed.getAsBoolean())throw new IOException("Stop firmware and USB before clearing security data.");
        byte[] before=new byte[SsaData.SIZE],erased=new byte[SsaData.SIZE];Arrays.fill(erased,(byte)255);
        try(RandomAccessFile f=new RandomAccessFile(card,"r")){f.seek(SsaData.OFFSET);f.readFully(before);}
        return SsaCardImport.apply(card,before,erased,SsaCardImport.hash(card),evidence,allowed);
    }
    private SsaCardReset(){}
}
