// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import java.io.*;
import java.nio.ByteBuffer;
import java.nio.charset.*;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.zip.*;
import org.json.JSONObject;

/** Transitional Java storage binding. Identity is the uploaded JSON, never the ZIP. */
public final class ReportArtifacts {
    public static final int MAX_JSON=65536;
    public static final class Artifact {
        public final String id, text, hash;
        public final File file;
        private final byte[] body;
        Artifact(File f,byte[] b)throws Exception {
            file=f;id=f.getName();body=b.clone();
            text=StandardCharsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(b)).toString();
            new JSONObject(text);hash=digest(b);
        }
        public byte[] bytes(){return body.clone();}
    }
    private static File root(Context c){return new File(c.getFilesDir(),"support-reports");}
    static File resolve(Context c,String id)throws IOException {
        if(id==null||!id.matches("android_support_[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}\\.zip"))throw new IOException("Invalid report identity");
        File folder=root(c).getCanonicalFile(),file=new File(folder,id);
        if(!file.getCanonicalFile().equals(file)||!Files.isRegularFile(file.toPath(),LinkOption.NOFOLLOW_LINKS))throw new IOException("Saved report unavailable");
        return file;
    }
    public static Artifact read(Context c,String id,String expectedHash)throws Exception {
        File file=resolve(c,id);if(file.length()>1024*1024)throw new IOException("Saved report too large");
        byte[] bytes;
        try(ZipFile zip=new ZipFile(file)){
            ZipEntry entry=zip.getEntry("diagnostics.json");
            if(zip.size()!=1||entry==null||entry.isDirectory()||entry.getSize()<1||entry.getSize()>MAX_JSON)throw new IOException("Invalid saved report");
            try(InputStream in=zip.getInputStream(entry);ByteArrayOutputStream out=new ByteArrayOutputStream()){
                byte[] buffer=new byte[4096];int n;
                while((n=in.read(buffer))!=-1){if(out.size()+n>MAX_JSON)throw new IOException("Saved report too large");out.write(buffer,0,n);}bytes=out.toByteArray();
            }
        }
        Artifact a=new Artifact(file,bytes);
        if(expectedHash!=null&&!expectedHash.equals(a.hash))throw new IOException("Saved report changed");
        return a;
    }
    public static String digest(byte[] bytes)throws Exception {
        byte[] d=MessageDigest.getInstance("SHA-256").digest(bytes);StringBuilder s=new StringBuilder();
        for(byte b:d)s.append(String.format(java.util.Locale.ROOT,"%02x",b&255));return s.toString();
    }
    private static File marker(Context c,String id){return new File(new File(root(c),"uploaded"),id+".json");}
    public static String receipt(Context c,Artifact a){
        try{
            JSONObject m=FirmwareStore.json(marker(c,a.id));String id=m.getString("report_id");
            if(m.getString("saved_id").equals(a.id)&&m.getString("report_sha256").equals(a.hash)&&id.matches("OS-[a-f0-9]{24}"))return id;
        }catch(Exception unavailable){}return "";
    }
    public static void recordReceipt(Context c,Artifact a,String receipt)throws Exception {
        if(!receipt.matches("OS-[a-f0-9]{24}"))throw new IOException("Invalid receipt");
        Artifact current=read(c,a.id,a.hash);
        if(!Arrays.equals(a.bytes(),current.bytes()))throw new IOException("Saved report changed");
        File mark=marker(c,a.id);Files.createDirectories(mark.getParentFile().toPath());
        FirmwareStore.writeJson(mark,new JSONObject().put("saved_id",a.id).put("report_sha256",a.hash)
            .put("report_id",receipt).put("submitted_utc",java.time.Instant.now().toString()));
    }
    static void pruneReceipt(Context c,String id){marker(c,id).delete();}
    public static void remember(Context c,Artifact a,boolean open)throws Exception {
        FirmwareStore.writeJson(new File(root(c),"review-state.json"),new JSONObject()
            .put("saved_id",a.id).put("report_sha256",a.hash).put("review_open",open));
    }
    public static JSONObject remembered(Context c)throws Exception {return FirmwareStore.json(new File(root(c),"review-state.json"));}
    private ReportArtifacts(){}
}
