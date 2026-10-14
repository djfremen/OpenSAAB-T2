// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.Activity;
import android.app.AlertDialog;
import android.content.Intent;
import android.net.Uri;
import android.widget.Toast;
import java.io.File;
import java.util.Arrays;
import org.json.JSONObject;

/** Thin UI binding for advice produced by the shared passive Rust formatter. */
public final class AdapterCompatibilityNotice {
    public static final String INFO="https://www.chipsoft.com.ua/2021/01/13/chipsoft-j2534-152.html";
    public static final String PRO="https://chipsoft.com.ua/download/chipsoft_j2534_pro_kline_canhacker_tech2win_drivers_152.zip";
    public static final class Notice {
        public final String code,title,message,url;
        private Notice(String c,String t,String m,String u){code=c;title=t;message=m;url=u;}
    }
    public static Notice read(File run) {
        if(run==null)return null;
        File file=new File(run,"crash-diagnostics.json");
        if(!file.isFile()||file.length()>16384)return null;
        try {
            JSONObject j=FirmwareStore.json(file).getJSONObject("compatibility_guidance");
            String code=j.getString("code"),title=j.getString("title"),message=j.getString("message"),url=j.getString("url");
            if(!Arrays.asList("older_pro_firmware","model_not_validated","identity_not_validated","firmware_not_validated","identity_unknown").contains(code))return null;
            if(title.isEmpty()||title.length()>80||message.isEmpty()||message.length()>1800||!Arrays.asList(INFO,PRO).contains(url))return null;
            return new Notice(code,title,message,url);
        } catch(Exception ignored){return null;}
    }
    public static void openOfficial(Activity activity,Notice notice) {
        // Only fixed vendor URLs, and only following the user's explicit button tap.
        if(!Arrays.asList(INFO,PRO).contains(notice.url))return;
        try{activity.startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse(notice.url)));}
        catch(android.content.ActivityNotFoundException e){Toast.makeText(activity,"No browser available. The official link is saved in Console.",Toast.LENGTH_LONG).show();}
    }
    public static AlertDialog show(Activity activity,Notice notice) {
        return new AlertDialog.Builder(activity).setTitle(notice.title).setMessage(notice.message)
            .setNeutralButton(buttonLabel(notice),(d,w)->openOfficial(activity,notice))
            .setPositiveButton("Review issue report",(d,w)->activity.startActivity(new Intent(activity,SupportReportActivity.class)))
            .setNegativeButton("Close",(d,w)->{}).show();
    }
    public static String buttonLabel(Notice notice){return notice.url.equals(PRO)?"Pro driver/firmware":"Official Chipsoft info";}
    private AdapterCompatibilityNotice(){}
}
