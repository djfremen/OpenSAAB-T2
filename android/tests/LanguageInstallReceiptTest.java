// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Instrumentation;
import android.os.Bundle;
import java.io.File;
import org.json.JSONObject;

/** Read-only receipt check after the recorded German-to-English installation walkthrough. */
public final class LanguageInstallReceiptTest extends Instrumentation {
    public void onCreate(Bundle b){super.onCreate(b);start();}
    void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
    public void onStart(){Bundle result=new Bundle();int status=-1;
        try{
            FirmwareStore store=new FirmwareStore(getTargetContext().getFilesDir());
            FirmwareCatalog.Entry de=FirmwareCatalog.byId("tech2_card_saab_v148.000_de");
            FirmwareCatalog.Entry en=FirmwareCatalog.byId("tech2win_card_saab_v148.000_en");
            JSONObject active=store.active();
            check(active.getString("id").equals(en.id)&&active.getString("language").equals("en"),"English activation metadata missing");
            FirmwareFiles.requireSha(store.original(de.id),de.imageSha);
            FirmwareFiles.requireSha(store.original(en.id),en.imageSha);
            File[] backups=new File(store.library,"backups").listFiles();boolean preserved=false;
            if(backups!=null)for(File backup:backups){
                JSONObject meta=FirmwareStore.json(new File(backup,"active.json"));
                if(meta.optString("id").equals(de.id)){
                    FirmwareFiles.requireSha(new File(backup,"card.bin"),meta.getString("backup_sha256"));preserved=true;
                }
            }
            check(preserved,"German working-card backup missing");check(store.missing().isEmpty(),"Incomplete installation");
            result.putString("stream","PASS: recorded installation has pinned German and English originals, English active language, a checksum-verified German working-card backup, and complete support files; no files changed\n");
        }catch(Throwable e){status=0;result.putString("stream","FAIL: "+e+"\n");}
        finish(status,result);
    }
}
