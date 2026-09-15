// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.res.AssetManager;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.function.BooleanSupplier;
import org.json.JSONObject;

/** Install shipped support files offline, under the caller's exclusive FirmwareGate lease. */
public final class BundledSupport {
    public static final String[] NAMES={"eprom.bin","opsys.dwn","candi.bin"};
    public static boolean available(AssetManager assets)throws IOException{
        List<String> entries=Arrays.asList(assets.list("system"));
        int count=0;for(String name:NAMES)if(entries.contains(name))count++;
        if(count!=0 && count!=NAMES.length)throw new IOException("Incomplete bundled development firmware");
        return count==NAMES.length;
    }
    public static void ensure(AssetManager assets,FirmwareStore store,BooleanSupplier cancel)throws Exception{
        if(!available(assets))return; // Default APK intentionally has metadata only; no automatic download.
        List<String> needed=new ArrayList<>();
        for(String name:NAMES){try{FirmwareFiles.validate(name,new File(store.firmware,name));}catch(IOException e){needed.add(name);}}
        if(needed.isEmpty())return; // Preserve valid existing support files, including deliberate custom profiles.
        byte[] metadata;
        try(InputStream in=assets.open("system/manifest.json");ByteArrayOutputStream out=new ByteArrayOutputStream()){
            byte[] b=new byte[1024];int n;while((n=in.read(b))!=-1){out.write(b,0,n);if(out.size()>16384)throw new IOException("App component manifest is too large");}metadata=out.toByteArray();
        }
        JSONObject manifest=new JSONObject(new String(metadata,StandardCharsets.UTF_8));
        Map<String,File> staged=new LinkedHashMap<>();
        try{
            // Validate every replacement before changing any installed component.
            for(String name:needed){
                FirmwareFiles.check(cancel);JSONObject spec=manifest.getJSONObject(name);
                File temp=File.createTempFile("bundled-support-",".tmp",store.files);staged.put(name,temp);
                try(InputStream in=assets.open("system/"+name)){FirmwareFiles.copy(in,temp,spec.getLong("bytes"),cancel);}
                FirmwareFiles.validate(name,temp);FirmwareFiles.requireSha(temp,spec.getString("sha256"));
            }
            for(Map.Entry<String,File> item:staged.entrySet())store.support(item.getKey(),item.getValue(),cancel);
        }finally{for(File file:staged.values())file.delete();}
    }
    private BundledSupport(){}
}
