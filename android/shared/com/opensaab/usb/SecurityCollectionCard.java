// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.nio.file.*;
import java.util.Arrays;

/** Fresh collection resets only a disposable guest card, never the selected working card. */
public final class SecurityCollectionCard {
    public static File prepare(File source,File run)throws Exception{
        if(source.length()!=33554432)throw new IOException("Fresh collection requires the selected 32 MB diagnostic card.");
        String hash=SsaCardImport.hash(source);
        byte[] before=SecuritySessionData.card(source),empty=new byte[SsaData.SIZE];Arrays.fill(empty,(byte)255);
        File copy=new File(run,"collection-card.bin");
        if(copy.exists())throw new IOException("This collection already has a working card; start a new session.");
        try {
            Files.copy(source.toPath(),copy.toPath());
            try(RandomAccessFile file=new RandomAccessFile(copy,"rw")){file.seek(SsaData.OFFSET);file.write(empty);file.getFD().sync();}
            // Verify every byte outside the security region, not just the new SSA value.
            try(InputStream a=new FileInputStream(source);InputStream b=new FileInputStream(copy)){
                byte[] left=new byte[65536],right=new byte[65536];int offset=0,n;
                while((n=a.read(left))>=0){if(n==0)continue;int got=0;while(got<n){int count=b.read(right,got,n-got);if(count<0)throw new IOException("Truncated collection card.");got+=count;}
                    for(int i=0;i<n;i++){int p=offset+i;if((p<SsaData.OFFSET||p>=SsaData.OFFSET+SsaData.SIZE)&&left[i]!=right[i])throw new IOException("Collection changed bytes outside security data.");}offset+=n;}
                if(b.read()!=-1)throw new IOException("Unexpected collection card size.");
            }
            if(!hash.equals(SsaCardImport.hash(source))||!Arrays.equals(empty,SecuritySessionData.card(copy)))throw new IOException("Diagnostic card changed while preparing collection.");
            Files.write(new File(run,"ssa-source-before.bin").toPath(),before,StandardOpenOption.CREATE_NEW);
            return copy;
        }catch(Exception failure){copy.delete();throw failure;}
    }
    public static byte[] beforeImport(File run)throws IOException{
        File original=new File(run,"ssa-source-before.bin");
        return SecuritySessionData.read(original.isFile()?original:new File(run,"ssa-card-before.bin"),SsaData.SIZE);
    }
    private SecurityCollectionCard(){}
}
