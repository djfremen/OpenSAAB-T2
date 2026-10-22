// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.function.BooleanSupplier;
import org.json.JSONObject;

/** Explicit stopped-session card reset; never sends a command to a vehicle. */
public final class SecurityReset {
    private static final AtomicBoolean BUSY=new AtomicBoolean();
    public static boolean busy(){return BUSY.get();}
    public static void show(Activity a,BooleanSupplier running){
        if(running.getAsBoolean()||FirmwareGate.sessionActive()||SecurityAccessView.workflowBusy()||BUSY.get()){
            new AlertDialog.Builder(a).setTitle("Stop firmware first").setMessage("Stop emulation and release USB before clearing saved security data.").setPositiveButton("OK",null).show();return;
        }
        new AlertDialog.Builder(a).setTitle("Reset security data to INIT_AUTH")
            .setMessage("Remove the saved VIN, security code and seed/key data from this emulator's working firmware card, using the same reset as NoMoreGlobal. Clear only BEFORE collecting fresh security data, never after processing or loading the response. A verified backup is saved on this device.\n\nThis does not clear fault codes or reset access in the vehicle. You must collect fresh security data afterward and follow the firmware's key-position instructions.")
            .setNegativeButton("Cancel",null).setPositiveButton("Reset to INIT_AUTH",(d,w)->clear(a,running)).show();
    }
    private static void clear(Activity a,BooleanSupplier running){
        ProgressDialog progress=new ProgressDialog(a);progress.setMessage("Backing up and clearing saved security data…");progress.setCancelable(false);progress.show();
        new Thread(()->{
            String message;AtomicBoolean cleared=new AtomicBoolean();
            try{
                perform(a,running,()->true,null,cleared);
                message="Saved security data cleared and verified at "+java.time.ZonedDateTime.now().format(java.time.format.DateTimeFormatter.ofPattern("MMM d, HH:mm:ss z"))+".\n\nNo processed security data is loaded. Turn the key to ON, choose Seed / Get Security Access, and follow all firmware prompts. Process and load the fresh response before retrying your task. Do not clear again after processing. No reset command was sent to the vehicle.";
            }catch(Exception error){SupportReports.recordError(a,error,false);message=(cleared.get()?"Security data was cleared, but its timestamp could not be saved: ":"Could not complete the reset: ")+error.getMessage();}
            final String result=message;a.runOnUiThread(()->{if(!a.isDestroyed()){progress.dismiss();if(!a.isFinishing())new AlertDialog.Builder(a).setTitle("Security data reset").setMessage(result).setPositiveButton("OK",null).show();}});
        },"security-card-reset").start();
    }
    /** Same transaction for the visible action and authenticated, explicitly confirmed MCP request. */
    static JSONObject perform(Activity a,BooleanSupplier running,BooleanSupplier permission,String expectedHash,AtomicBoolean cleared)throws Exception{
        if(!BUSY.compareAndSet(false,true))throw new IOException("Security reset already in progress");
        try(FirmwareGate.Lease lease=FirmwareGate.change()){
                BooleanSupplier allowed=()->permission.getAsBoolean()&&!a.isDestroyed()&&!a.isFinishing()&&!running.getAsBoolean()&&!SecurityAccessView.workflowBusy();
                if(!allowed.getAsBoolean())throw new IOException("Session changed. Stop firmware before clearing.");
                File card=new File(a.getFilesDir(),"firmware/card.bin");String beforeHash=SsaCardImport.hash(card);
                if(expectedHash!=null&&!expectedHash.equals(beforeHash))throw new IOException("Working card changed; inspect it again before resetting");
                File evidence=new File(a.getFilesDir(),"security-resets/"+UUID.randomUUID());Files.createDirectories(evidence.toPath());
                File previous=new File(a.getNoBackupFilesDir(),"security-processing-status.properties");
                if(previous.isFile())Files.copy(previous.toPath(),new File(evidence,"processing-before-reset.properties").toPath());
                String hash=SsaCardReset.clear(card,evidence,allowed,beforeHash);cleared.set(true);
                if(SsaState.read(card)!=SsaState.INIT_AUTH||!SsaCardImport.hash(card).equals(hash))throw new IOException("Reset readback failed; inspect retained evidence");
                String utc=java.time.Instant.now().toString();
                JSONObject receipt=new JSONObject().put("status","security_data_cleared").put("cleared_utc",utc)
                    .put("offset",SsaData.OFFSET).put("bytes",SsaData.SIZE).put("fill",255).put("working_card_sha256",hash)
                    .put("vehicle_command_sent",false).put("card_state","INIT_AUTH").put("before_card_sha256",beforeHash)
                    .put("backup_sha256",SsaCardImport.hash(new File(evidence,"card-before-security.bin")))
                    .put("receipt_relative_path","security-resets/"+evidence.getName()+"/result.json");
                Files.write(new File(evidence,"result.json").toPath(),receipt.toString(2).getBytes(StandardCharsets.UTF_8));
                // Old processing history must not be presented as the current card's status.
                new File(a.getNoBackupFilesDir(),"security-processing-status.properties").delete();
                Files.write(new File(a.getNoBackupFilesDir(),"security-reset.json").toPath(),receipt.toString().getBytes(StandardCharsets.UTF_8));
                return receipt;
        }finally{BUSY.set(false);}
    }
    private SecurityReset(){}
}
