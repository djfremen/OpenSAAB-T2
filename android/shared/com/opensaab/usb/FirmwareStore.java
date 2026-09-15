// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.function.BooleanSupplier;
import org.json.JSONObject;

/** Originals remain immutable. Activation backs up the working card and journals its replacement. */
public final class FirmwareStore {
    public final File files,firmware,library;
    public FirmwareStore(File files){this.files=files;firmware=new File(files,"firmware");library=new File(files,"firmware-library");}
    public File original(String id){if(!id.matches("[a-zA-Z0-9_.-]+"))throw new IllegalArgumentException("Invalid image identifier");return new File(new File(library,id),"original.bin");}
    public String missingSupport(){
        List<String> missing=new ArrayList<>();
        for(String name:BundledSupport.NAMES)try{FirmwareFiles.validate(name,new File(firmware,name));}catch(IOException e){missing.add(name);}
        return String.join(", ",missing);
    }
    public String missing(){
        List<String> missing=new ArrayList<>();
        if(new File(firmware,"card-change.json").exists())return "Open Firmware to finish an interrupted image change";
        for(String n:new String[]{"card.bin","eprom.bin","opsys.dwn","candi.bin"})try{FirmwareFiles.validate(n,new File(firmware,n));}catch(IOException e){missing.add(n);}
        return String.join(", ",missing);
    }
    public JSONObject active()throws Exception{
        File f=new File(firmware,"active.json");
        return f.isFile()?json(f):new JSONObject().put("id","existing").put("label",new File(firmware,"card.bin").exists()?"Existing card (preserved)":"No card installed");
    }
    public void saveOriginal(File image,String id,String label,String sha,BooleanSupplier cancel)throws Exception{
        FirmwareFiles.validate("card.bin",image);FirmwareFiles.requireSha(image,sha);
        File dest=original(id);
        if(dest.exists())FirmwareFiles.requireSha(dest,sha);else FirmwareFiles.atomicCopy(image,dest,cancel);
        writeJson(new File(dest.getParentFile(),"image.json"),new JSONObject().put("id",id).put("label",label).put("sha256",sha));
    }
    public void activate(String id,String label,String expectedSha,BooleanSupplier cancel)throws Exception{
        FirmwareCatalog.requireRunnable(expectedSha);recover();File image=original(id);FirmwareFiles.validate("card.bin",image);FirmwareFiles.requireSha(image,expectedSha);
        if(active().optString("id").equals(id) && new File(firmware,"card.bin").isFile()){FirmwareFiles.validate("card.bin",new File(firmware,"card.bin"));return;}
        if(files.getUsableSpace()<100L*1024*1024)throw new IOException("Need at least 100 MiB free to preserve and switch the working card");
        Files.createDirectories(firmware.toPath());
        File card=new File(firmware,"card.bin");String before=card.isFile()?FirmwareFiles.sha(card):"";
        if(card.isFile()){
            File backup=new File(new File(library,"backups"),stamp());Files.createDirectories(backup.toPath());
            FirmwareFiles.atomicCopy(card,new File(backup,"card.bin"),cancel);
            writeJson(new File(backup,"active.json"),active().put("backup_sha256",before));
            for(String n:new String[]{"card-authorized.bin"})if(new File(firmware,n).isFile())FirmwareFiles.atomicCopy(new File(firmware,n),new File(backup,n),cancel);
            for(String n:new String[]{"chipsoft-audible-authority.json","chipsoft-symbol-authority.json"})if(new File(files,n).isFile())FirmwareFiles.atomicCopy(new File(files,n),new File(backup,n),cancel);
        }
        File next=new File(firmware,"card.next");
        try{
            FirmwareFiles.atomicCopy(image,next,cancel);FirmwareFiles.requireSha(next,expectedSha);FirmwareFiles.check(cancel);
            JSONObject nextInfo=new JSONObject().put("id",id).put("label",label).put("sha256_at_activation",expectedSha).put("activated_utc",java.time.Instant.now().toString());
            writeJson(new File(firmware,"card-change.json"),new JSONObject().put("before_sha256",before).put("after_sha256",expectedSha).put("active",nextInfo));
            // Commit is deliberately non-cancellable. Recovery finishes metadata after a process loss.
            Files.move(next.toPath(),card.toPath(),StandardCopyOption.REPLACE_EXISTING,StandardCopyOption.ATOMIC_MOVE);
            finishCommit(nextInfo);
        }finally{next.delete();}
    }
    private void finishCommit(JSONObject active)throws Exception{
        writeJson(new File(firmware,"active.json"),active);
        // Explicitly scoped, one-shot security artifacts cannot follow a different program image.
        Files.deleteIfExists(new File(firmware,"card-authorized.bin").toPath());
        for(String n:new String[]{"chipsoft-audible-authority.json","chipsoft-symbol-authority.json"})Files.deleteIfExists(new File(files,n).toPath());
        Files.deleteIfExists(new File(firmware,"card-change.json").toPath());
    }
    public void recover()throws Exception{
        File journal=new File(firmware,"card-change.json");if(!journal.exists())return;
        JSONObject j=json(journal);File card=new File(firmware,"card.bin");String hash=card.isFile()?FirmwareFiles.sha(card):"";
        if(hash.equals(j.getString("after_sha256")))finishCommit(j.getJSONObject("active"));
        else if(hash.equals(j.getString("before_sha256")))Files.delete(journal.toPath());
        else throw new IOException("Interrupted card change needs recovery; preserved backups are in Firmware library");
        Files.deleteIfExists(new File(firmware,"card.next").toPath());
    }
    public void support(String name,File source,BooleanSupplier cancel)throws Exception{
        FirmwareFiles.validate(name,source);if(name.equals("card.bin"))throw new IOException("Use Import card for program images");
        File dest=new File(firmware,name);
        if(dest.isFile())FirmwareFiles.atomicCopy(dest,new File(new File(library,"support-backups"),stamp()+"-"+name),cancel);
        FirmwareFiles.atomicCopy(source,dest,cancel);
    }
    public static JSONObject json(File f)throws Exception{if(f.length()>16384)throw new IOException("Metadata too large");return new JSONObject(new String(Files.readAllBytes(f.toPath()),StandardCharsets.UTF_8));}
    static void writeJson(File file,JSONObject json)throws Exception{
        Files.createDirectories(file.getParentFile().toPath());File temp=File.createTempFile("metadata-",".tmp",file.getParentFile());
        try{try(FileOutputStream out=new FileOutputStream(temp)){out.write(json.toString(2).getBytes(StandardCharsets.UTF_8));out.getFD().sync();}Files.move(temp.toPath(),file.toPath(),StandardCopyOption.REPLACE_EXISTING,StandardCopyOption.ATOMIC_MOVE);}finally{temp.delete();}
    }
    private static String stamp(){return java.time.Instant.now().toString().replaceAll("[^0-9TZ]","")+"-"+UUID.randomUUID();}
}
