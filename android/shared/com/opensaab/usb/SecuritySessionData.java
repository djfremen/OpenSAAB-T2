// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.*;
import org.json.JSONObject;

/** Read-only working SSA and completed original-guest collection evidence. */
public final class SecuritySessionData {
    final byte[] bytes;
    final SsaState state;
    final boolean completed;
    private SecuritySessionData(byte[] bytes,boolean completed){
        this.bytes=bytes;this.state=SsaState.analyze(bytes);this.completed=completed;
    }
    static byte[] read(File file,int size)throws IOException{
        if(!file.isFile()||file.length()!=size)throw new IOException("Collected security data is unavailable or changed.");
        return Files.readAllBytes(file.toPath());
    }
    static byte[] card(File card)throws IOException{
        byte[] b=new byte[SsaData.SIZE];try(RandomAccessFile f=new RandomAccessFile(card,"r")){f.seek(SsaData.OFFSET);f.readFully(b);}return b;
    }
    static SecuritySessionData load(File run,File card,VehicleIdentity identity){
        byte[] b=null;boolean complete=false;
        try{
            if(run!=null&&identity!=null){
                File working=new File(run,"working-ssa.bin"),finalSsa=new File(run,"ssa-card-after.bin");
                if(working.exists())b=read(working,SsaData.SIZE);
                else if(finalSsa.exists()){snapshot(run,false);b=read(finalSsa,SsaData.SIZE);}
                if(b!=null&&SsaState.analyze(b)!=SsaState.INIT_AUTH&&!identity.vin.equals(SsaData.vin(b)))
                    return new SecuritySessionData(null,false);
                if(b!=null)try{SsaData.validateInput(b);complete=new File(run,"security-card-baseline.sha256").isFile()&&transferEvidence(run,b);if(complete&&finalSsa.isFile())snapshot(run,true);}catch(Exception invalid){complete=false;}
            }
            if(b==null)b=card(card);
            return new SecuritySessionData(b,complete);
        }catch(Exception unavailable){return new SecuritySessionData(null,false);}
    }
    static void snapshot(File run,boolean fresh)throws Exception{
        File metadata=new File(run,"native-security-snapshot.json");
        if(!metadata.isFile()||metadata.length()>8192)throw new IOException("Missing original-firmware collection evidence.");
        JSONObject j=new JSONObject(new String(Files.readAllBytes(metadata.toPath()),StandardCharsets.UTF_8));
        if(!"original-guest-memory".equals(j.optString("origin"))||j.optInt("card_offset")!=SsaData.OFFSET||j.optInt("bytes")!=SsaData.SIZE
            ||(fresh&&(!j.optBoolean("ssa_memory_flash_enabled")||j.optLong("ssa_erases")<1||j.optLong("ssa_programmed_bytes")<SsaData.SIZE)))
            throw new IOException("Firmware has not written complete fresh security data. Follow all collection prompts.");
    }
    static boolean transferEvidence(File run,byte[] selected)throws IOException{
        File witness=new File(run,"collection-ssa.bin");
        if(witness.exists())return Arrays.equals(selected,read(witness,SsaData.SIZE));
        // Legacy MDI sessions recorded every original LCD transition in stdout.
        // Bounded recovery, only complete original LCD transfer prompts count.
        File log=new File(run.getParentFile(),"native-process-private.log");
        if(!log.isFile())log=new File(run,"native-process.log");
        if(!log.isFile())log=new File(run,"rust.log");
        if(!log.isFile())return false;
        try(RandomAccessFile f=new RandomAccessFile(log,"r")){
            f.seek(Math.max(0,f.length()-2*1024*1024));
            if(f.getFilePointer()>0)f.readLine();
            String line;while((line=f.readLine())!=null)if(line.startsWith("LCD: ")&&SsaData.needsAccess(line.substring(5)))return true;
        }
        return false;
    }
    /** Main-screen history uses the latest matching working session without changing it. */
    public static byte[] savedBytes(File files,VehicleIdentity identity){
        File card=new File(files,"firmware/card.bin");
        try{
            List<File> runs=new ArrayList<>();File[] direct=files.listFiles(File::isDirectory);
            if(direct!=null)for(File dir:direct)if(dir.getName().startsWith("native-")||dir.getName().startsWith("chipsoft-"))runs.add(dir);
            File[] mdi=new File(files,"mdi-sessions").listFiles(File::isDirectory);if(mdi!=null)for(File dir:mdi)runs.add(new File(dir,"native"));
            runs.removeIf(dir->!new File(dir,"security-card-baseline.sha256").isFile());
            runs.sort(Comparator.comparingLong(File::lastModified).reversed());
            if(identity!=null&&!runs.isEmpty()){
                File run=runs.get(0);VehicleIdentity saved=VehicleSession.read(new File(run,VehicleSession.FILE));
                if(saved!=null&&saved.vin.equals(identity.vin)){
                    String baseline=new String(read(new File(run,"security-card-baseline.sha256"),64),StandardCharsets.US_ASCII);
                    if(baseline.equals(SsaCardImport.hash(card)))return load(run,card,saved).bytes;
                }
            }
            return card(card);
        }catch(Exception unavailable){return null;}
    }
    static void recordBaseline(File run,File card)throws Exception{
        File temporary=new File(run,"security-card-baseline.sha256.tmp");
        Files.write(temporary.toPath(),SsaCardImport.hash(card).getBytes(StandardCharsets.US_ASCII));
        Files.move(temporary.toPath(),new File(run,"security-card-baseline.sha256").toPath(),java.nio.file.StandardCopyOption.ATOMIC_MOVE,java.nio.file.StandardCopyOption.REPLACE_EXISTING);
    }
    static byte[] processingInput(File run,File card,VehicleIdentity identity,byte[] selected)throws Exception{
        if(identity==null)throw new IOException("No verified vehicle identity for these seeds.");
        snapshot(run,true);
        byte[] before=read(new File(run,"ssa-card-before.bin"),SsaData.SIZE),after=read(new File(run,"ssa-card-after.bin"),SsaData.SIZE);
        if(Arrays.equals(before,after))throw new IOException("SSA unchanged; fresh collection required.");
        SsaData.validateInput(after);
        if(!identity.vin.equals(SsaData.vin(after))||!Arrays.equals(after,selected))throw new IOException("Selected security data changed; review it again.");
        if(!transferEvidence(run,after))throw new IOException("Complete the original firmware collection and transfer prompt first.");
        SsaCardImport.verifyBaseline(card,before);
        File baseline=new File(run,"security-card-baseline.sha256");
        if(!baseline.isFile()||baseline.length()!=64||!SsaCardImport.hash(card).equals(new String(Files.readAllBytes(baseline.toPath()),StandardCharsets.US_ASCII)))
            throw new IOException("Diagnostic software changed since collection. Select the original software or collect fresh data.");
        return after;
    }
}
