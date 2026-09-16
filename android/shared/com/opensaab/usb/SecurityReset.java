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
    public static void show(Activity a,BooleanSupplier running){
        if(running.getAsBoolean()||FirmwareGate.sessionActive()||SecurityAccessView.workflowBusy()||BUSY.get()){
            new AlertDialog.Builder(a).setTitle("Stop firmware first").setMessage("Stop emulation and release USB before clearing saved security data.").setPositiveButton("OK",null).show();return;
        }
        new AlertDialog.Builder(a).setTitle("Clear offset · fresh security data")
            .setMessage("Remove the saved VIN, security code and seed/key data from this emulator's working firmware card, using the same reset as NoMoreGlobal. A verified backup is saved on this device.\n\nThis does not clear fault codes or reset access in the vehicle. You must collect fresh security data afterward and follow the firmware's key-position instructions.")
            .setNegativeButton("Cancel",null).setPositiveButton("Clear security data",(d,w)->clear(a,running)).show();
    }
    private static void clear(Activity a,BooleanSupplier running){
        if(!BUSY.compareAndSet(false,true))return;
        ProgressDialog progress=new ProgressDialog(a);progress.setMessage("Backing up and clearing saved security data…");progress.setCancelable(false);progress.show();
        new Thread(()->{
            String message;boolean cleared=false;
            try(FirmwareGate.Lease lease=FirmwareGate.change()){
                BooleanSupplier allowed=()->!a.isDestroyed()&&!a.isFinishing()&&!running.getAsBoolean()&&!SecurityAccessView.workflowBusy();
                if(!allowed.getAsBoolean())throw new IOException("Session changed. Stop firmware before clearing.");
                File evidence=new File(a.getFilesDir(),"security-resets/"+UUID.randomUUID());Files.createDirectories(evidence.toPath());
                String hash=SsaCardReset.clear(new File(a.getFilesDir(),"firmware/card.bin"),evidence,allowed);cleared=true;
                String utc=java.time.Instant.now().toString();
                JSONObject receipt=new JSONObject().put("status","security_data_cleared").put("cleared_utc",utc)
                    .put("offset",SsaData.OFFSET).put("bytes",SsaData.SIZE).put("fill",255).put("working_card_sha256",hash)
                    .put("vehicle_command_sent",false);
                Files.write(new File(evidence,"result.json").toPath(),receipt.toString(2).getBytes(StandardCharsets.UTF_8));
                // Old processing history must not be presented as the current card's status.
                new File(a.getNoBackupFilesDir(),"security-processing-status.properties").delete();
                Files.write(new File(a.getNoBackupFilesDir(),"security-reset.json").toPath(),receipt.toString().getBytes(StandardCharsets.UTF_8));
                message="Saved security data cleared and verified at "+java.time.ZonedDateTime.now().format(java.time.format.DateTimeFormatter.ofPattern("MMM d, HH:mm:ss z"))+".\n\nTurn the key to ON for VIN discovery, then collect fresh security data and follow the firmware prompts. No reset command was sent to the vehicle.";
            }catch(Exception error){SupportReports.recordError(a,error,false);message=(cleared?"Security data was cleared, but its timestamp could not be saved: ":"Could not complete the reset: ")+error.getMessage();}
            finally{BUSY.set(false);}
            final String result=message;a.runOnUiThread(()->{if(!a.isDestroyed()){progress.dismiss();if(!a.isFinishing())new AlertDialog.Builder(a).setTitle("Security data reset").setMessage(result).setPositiveButton("OK",null).show();}});
        },"security-card-reset").start();
    }
    private SecurityReset(){}
}
