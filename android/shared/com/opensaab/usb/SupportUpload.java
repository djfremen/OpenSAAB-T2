// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.net.*;
import java.nio.charset.StandardCharsets;
import org.json.JSONObject;

/** Explicit, bounded HTTPS upload. No account keys, redirects or automatic retries. */
public final class SupportUpload {
    public static final String ENDPOINT="https://www.opensaab.com/api/support/reports";
    public static String send(String reviewedJson)throws IOException {
        return send(reviewedJson,(HttpURLConnection)new URL(ENDPOINT).openConnection());
    }
    static String send(String reviewedJson,HttpURLConnection connection)throws IOException {
        byte[] body=reviewedJson.getBytes(StandardCharsets.UTF_8);
        try {
            if(body.length>65536)throw new IOException("This report is too large. Use Copy / save instead.");
            connection.setConnectTimeout(15000);connection.setReadTimeout(30000);
            connection.setInstanceFollowRedirects(false);
            connection.setRequestMethod("POST");connection.setDoOutput(true);
            connection.setRequestProperty("Content-Type","application/json; charset=utf-8");
            connection.setRequestProperty("X-OpenSAAB-Consent","support-report-v1");
            connection.setFixedLengthStreamingMode(body.length);
            try(OutputStream out=connection.getOutputStream()){out.write(body);}
            int status=connection.getResponseCode();
            if(status!=201){
                if(status==429)throw new IOException("Too many reports have been sent recently. Please try again later.");
                if(status==400||status==413||status==415)throw new IOException("The server could not accept this report. Use Copy / save instead.");
                throw new IOException("OpenSAAB could not confirm storage. Your report is kept here; try again later.");
            }
            ByteArrayOutputStream reply=new ByteArrayOutputStream();
            try(InputStream in=connection.getInputStream()){
                byte[] buffer=new byte[1024];int n;
                while((n=in.read(buffer))!=-1){if(reply.size()+n>4096)throw new IOException("Unexpected server reply; keep your local report.");reply.write(buffer,0,n);}
            }
            try {
                JSONObject receipt=new JSONObject(new String(reply.toByteArray(),StandardCharsets.UTF_8));
                String id=receipt.getString("report_id");
                if(!receipt.getBoolean("stored")||!id.matches("OS-[a-f0-9]{24}"))throw new Exception();
                return id;
            }catch(Exception e){throw new IOException("Storage was not confirmed. Keep your report and try again.");}
        }finally{connection.disconnect();}
    }
    private SupportUpload(){}
}
