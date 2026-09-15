// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.res.AssetManager;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import java.util.function.BooleanSupplier;
import org.json.JSONObject;

/** Validate all selected support inputs before replacing any installed component. */
public final class SupportFirmware {
    public static void install(AssetManager assets,FirmwareStore store,Map<String,File> inputs,BooleanSupplier cancel)throws Exception{
        if(inputs.isEmpty() || inputs.size()>3)throw new IOException("Choose up to three support files");
        JSONObject manifest;
        try(InputStream in=assets.open("system/manifest.json");ByteArrayOutputStream bytes=new ByteArrayOutputStream()){
            byte[] buffer=new byte[1024];int n;
            while((n=in.read(buffer))!=-1){FirmwareFiles.check(cancel);bytes.write(buffer,0,n);if(bytes.size()>16384)throw new IOException("Support metadata is too large");}
            manifest=new JSONObject(new String(bytes.toByteArray(),StandardCharsets.UTF_8));
        }
        for(Map.Entry<String,File> item:inputs.entrySet()){
            FirmwareFiles.check(cancel);
            if(!Arrays.asList(BundledSupport.NAMES).contains(item.getKey()))throw new IOException("Choose eprom.bin, opsys.dwn or candi.bin");
            FirmwareFiles.validate(item.getKey(),item.getValue());
            FirmwareFiles.requireSha(item.getValue(),manifest.getJSONObject(item.getKey()).getString("sha256"));
        }
        for(Map.Entry<String,File> item:inputs.entrySet())store.support(item.getKey(),item.getValue(),cancel);
    }
    private SupportFirmware(){}
}
