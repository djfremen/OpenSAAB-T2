// SPDX-License-Identifier: MPL-2.0
package com.opensaab.checker;

import android.content.Context;
import android.content.pm.*;
import com.opensaab.usb.CompatibilityCheck;
import java.io.*;
import java.net.*;
import java.security.MessageDigest;
import java.util.*;
import java.util.zip.*;
import org.json.*;

/** Small HTTPS catalog, streamed downloads, pinned signer and strict per-channel identity. */
public final class SetupRelease {
    public static final String CATALOG="https://www.opensaab.com/static/t2/releases.json";
    static final String SIGNER="23dbf0c1c5ba4179e95bda88d6160f97067d601131e260de751e3b8c9c8088b7";
    static final String ROOT="https://github.com/djfremen/OpenSAAB-T2/releases/download/";
    public final String abi,packageName,version,url,sha;
    public final int code;public final long bytes;
    SetupRelease(JSONObject o)throws Exception{
        abi=o.getString("abi");packageName=o.getString("package");version=o.getString("version");code=o.getInt("version_code");url=o.getString("url");sha=o.getString("sha256");bytes=o.getLong("bytes");
        boolean arm32="armeabi-v7a".equals(abi);
        if(!arm32 && !"arm64-v8a".equals(abi))throw new IOException("Unknown architecture");
        String pkg=arm32?"com.opensaab.tech2.headunit32":"com.opensaab.tech2";
        String tag=arm32?"headunit-v"+version:"v"+version;
        String file=arm32?"OpenSAAB-T2-headunit-armeabi-v7a.apk":"OpenSAAB-T2-arm64-v8a.apk";
        if(!pkg.equals(packageName)||!url.equals(ROOT+tag+"/"+file)||!sha.matches("[a-f0-9]{64}")||bytes<100000||bytes>32*1024*1024||code<1
                ||!version.matches(arm32?"[0-9]+\\.[0-9]+\\.[0-9]+-headunit\\.[0-9]+":"[0-9]+\\.[0-9]+\\.[0-9]+(?:-preview\\.[0-9]+)?"))throw new IOException("Invalid release information");
    }
    static boolean releaseSigned(PackageInfo installed) {
        if (installed == null || installed.signatures == null || installed.signatures.length != 1) return false;
        try { return SIGNER.equals(hex(MessageDigest.getInstance("SHA-256").digest(installed.signatures[0].toByteArray()))); }
        catch (java.security.NoSuchAlgorithmException e) { return false; }
    }
    static String installedVersion(PackageInfo installed) {
        String name = installed.versionName;
        return name == null || name.trim().isEmpty() ? "Version not reported (build " + installed.versionCode + ")" : name;
    }
    private void checkInstalled(Context context) throws IOException {
        try {
            PackageInfo installed = context.getPackageManager().getPackageInfo(packageName, PackageManager.GET_SIGNATURES);
            if (!releaseSigned(installed)) throw new IOException("Installed app uses a different signing key. Keep it installed to preserve its data; contact OpenSAAB support.");
            if (installed.versionCode > code) throw new IOException("A newer version is already installed; it will not be downgraded");
        } catch (PackageManager.NameNotFoundException absent) { }
    }
    public String title(){return (abi.equals("armeabi-v7a")?"32-bit ARM · experimental":"64-bit ARM · preview")+" · "+version;}
    public static String preferred(int api,String[] abis,boolean installed32,boolean installed64){
        if(api<26)return "";
        if(installed32&&!installed64&&CompatibilityCheck.supportsAbi(abis,"armeabi-v7a"))return "armeabi-v7a";
        if(CompatibilityCheck.supportsAbi(abis,"arm64-v8a"))return "arm64-v8a";
        return CompatibilityCheck.supportsAbi(abis,"armeabi-v7a")?"armeabi-v7a":"";
    }
    public static List<SetupRelease> catalog()throws Exception{
        HttpURLConnection c=open(new URL(CATALOG),false);byte[] raw;
        try(InputStream in=c.getInputStream();ByteArrayOutputStream out=new ByteArrayOutputStream()){
            byte[] buf=new byte[4096];int n;while((n=in.read(buf))!=-1){if(out.size()+n>16384)throw new IOException("Catalog too large");out.write(buf,0,n);}raw=out.toByteArray();
        }finally{c.disconnect();}
        return parse(new String(raw,"UTF-8"));
    }
    static List<SetupRelease> parse(String text)throws Exception{
        JSONObject root=new JSONObject(text);if(root.getInt("schema")!=1)throw new IOException("Unsupported release catalog");
        JSONArray rows=root.getJSONArray("releases");if(rows.length()!=2)throw new IOException("Incomplete release catalog");
        List<SetupRelease> result=new ArrayList<>();Set<String> seen=new HashSet<>();
        for(int i=0;i<rows.length();i++){SetupRelease r=new SetupRelease(rows.getJSONObject(i));if(!seen.add(r.abi))throw new IOException("Duplicate channel");result.add(r);}return result;
    }
    static HttpURLConnection open(URL start,boolean asset)throws Exception{
        URL u=start;
        for(int i=0;i<5;i++){
            String host=u.getHost();boolean allowed=asset?(host.equals("github.com")||host.equals("release-assets.githubusercontent.com")||host.equals("objects.githubusercontent.com")):host.equals("www.opensaab.com");
            if(!"https".equals(u.getProtocol())||!allowed||u.getUserInfo()!=null||(u.getPort()!=-1&&u.getPort()!=443))throw new IOException("Untrusted download destination");
            HttpURLConnection c=(HttpURLConnection)u.openConnection();c.setConnectTimeout(15000);c.setReadTimeout(30000);c.setInstanceFollowRedirects(false);c.setRequestProperty("User-Agent","OpenSAAB-Setup/0.3.2");
            int code;try{code=c.getResponseCode();}catch(Exception e){c.disconnect();throw e;}
            if(code==200)return c;
            String location=c.getHeaderField("Location");c.disconnect();
            if(asset&&(code==301||code==302||code==303||code==307||code==308)&&location!=null){u=new URL(u,location);continue;}
            throw new IOException("Download unavailable (HTTP "+code+")");
        }throw new IOException("Too many download redirects");
    }
    public interface Progress {void update(int percent);}
    public File download(Context context,Progress progress)throws Exception{
        checkInstalled(context); // Fail before any network request or installer file changes.
        File dir=new File(context.getFilesDir(),"installers");if(!dir.isDirectory()&&!dir.mkdirs())throw new IOException("Cannot prepare storage");
        File target=new File(dir,sha+".apk"),part=new File(dir,sha+".part");
        if(target.isFile()){try{verify(context,target);return target;}catch(Exception e){target.delete();}}
        if(dir.getUsableSpace()<bytes*3+140L*1024*1024)throw new IOException("Free up storage before downloading (allow at least 160 MB).");
        HttpURLConnection c=open(new URL(url),true);
        try {
            try(InputStream in=c.getInputStream();FileOutputStream out=new FileOutputStream(part)){
                byte[] buf=new byte[16384];long total=0;int n,last=-1;
                while((n=in.read(buf))!=-1){total+=n;if(total>bytes)throw new IOException("Download size differs from release");out.write(buf,0,n);int percent=(int)(total*100/bytes);if(percent!=last){progress.update(percent);last=percent;}}
                if(total!=bytes)throw new IOException("Download interrupted; please retry");out.getFD().sync();
            }
            verify(context,part);if(!part.renameTo(target))throw new IOException("Cannot save verified installer");
            for(File old:dir.listFiles())if(!old.equals(target))old.delete();
            return target;
        }finally{c.disconnect();part.delete();}
    }
    public void verify(Context c,File apk)throws Exception{
        if(apk.length()!=bytes)throw new IOException("Installer size mismatch");
        MessageDigest digest=MessageDigest.getInstance("SHA-256");
        try(InputStream in=new FileInputStream(apk)){byte[] buf=new byte[16384];int n;while((n=in.read(buf))!=-1)digest.update(buf,0,n);}
        if(!sha.equals(hex(digest.digest())))throw new IOException("Installer checksum mismatch");
        PackageInfo p=c.getPackageManager().getPackageArchiveInfo(apk.getPath(),PackageManager.GET_SIGNATURES);
        if(p==null||!packageName.equals(p.packageName)||p.versionCode!=code||!version.equals(p.versionName)||p.signatures==null||p.signatures.length!=1
                ||!SIGNER.equals(hex(MessageDigest.getInstance("SHA-256").digest(p.signatures[0].toByteArray()))))throw new IOException("Installer identity or signature does not match OpenSAAB");
        try(ZipFile zip=new ZipFile(apk)){
            if(zip.getEntry("lib/"+abi+"/libtech2_emu.so")==null)throw new IOException("Installer architecture mismatch");
            Enumeration<? extends ZipEntry> all=zip.entries();while(all.hasMoreElements()){String n=all.nextElement().getName();if(n.startsWith("lib/")&&!n.startsWith("lib/"+abi+"/"))throw new IOException("Unexpected native architecture");}
        }
        checkInstalled(c); // Recheck at install time in case the installed app changed.
    }
    static String hex(byte[] data){StringBuilder out=new StringBuilder();for(byte b:data)out.append(String.format(java.util.Locale.ROOT,"%02x",b&255));return out.toString();}
}
