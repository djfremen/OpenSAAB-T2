// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import org.json.JSONObject;
import android.content.Context;

/** Local connection credentials; deliberately excluded from APKs and backups. */
public final class MdiProfile {
    final byte[] key;
    private MdiProfile(byte[] key){this.key=key;}
    static MdiProfile parse(byte[] bytes)throws IOException {
        try {
            if(bytes.length>4096)throw new IOException();
            JSONObject p=new JSONObject(new String(bytes,StandardCharsets.UTF_8));
            byte[] key=Base64.getDecoder().decode(p.getString("base_key"));
            int schema=p.getInt("schema");
            if((schema!=1&&schema!=2) || !"classic_mdi".equals(p.getString("adapter_family"))
                || key.length!=56)throw new IOException();
            // Legacy schema-1 serials are ignored. Every connection obtains a
            // fresh management serial from the selected adapter, before login.
            return new MdiProfile(key);
        }catch(Exception invalid){throw new IOException("Invalid MDI connection profile");}
    }
    static File file(Context c){return new File(c.getNoBackupFilesDir(),"mdi/connection-profile.json");}
    static MdiProfile read(Context c)throws IOException {
        File f=file(c);
        if(!f.isFile()||f.length()>4096)throw new IOException("MDI setup needed — import the adapter connection profile in App menu");
        return parse(Files.readAllBytes(f.toPath()));
    }
    static void importProfile(Context c,InputStream input)throws IOException {
        ByteArrayOutputStream bytes=new ByteArrayOutputStream();byte[] buffer=new byte[1024];int n;
        while((n=input.read(buffer))!=-1){if(bytes.size()+n>4096)throw new IOException("Invalid MDI connection profile");bytes.write(buffer,0,n);}
        byte[] raw=bytes.toByteArray();parse(raw);
        File dest=file(c);Files.createDirectories(dest.getParentFile().toPath());
        File temp=File.createTempFile("profile-",".tmp",dest.getParentFile());
        try {Files.write(temp.toPath(),raw);Files.move(temp.toPath(),dest.toPath(),StandardCopyOption.ATOMIC_MOVE,StandardCopyOption.REPLACE_EXISTING);}
        finally {temp.delete();java.util.Arrays.fill(raw,(byte)0);}
    }
    public static boolean candidate(AdapterCatalog.Match match){return match.family.equals("generic_rndis")||match.family.equals("bosch_etas_vci_candidate");}
    public static boolean packaged(Context c){return new File(c.getApplicationInfo().nativeLibraryDir,"libopensaab_mdi_android.so").isFile()
        &&new File(c.getApplicationInfo().nativeLibraryDir,"libtech2_mdi.so").isFile();}
}
