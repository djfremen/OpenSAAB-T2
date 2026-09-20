// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.Activity;
import android.content.*;
import android.content.pm.*;
import android.net.Uri;
import android.widget.Toast;
import java.security.MessageDigest;
import java.util.Locale;

/** Optional maintenance after firmware preparation; Android confirms removal. */
public final class SetupCleanup {
    private static final String PACKAGE="com.opensaab.checker";
    private static final String SIGNER="23dbf0c1c5ba4179e95bda88d6160f97067d601131e260de751e3b8c9c8088b7";
    public static boolean available(Context context){
        if(!new FirmwareStore(context.getFilesDir()).missing().isEmpty())return false;
        try{
            PackageInfo p=context.getPackageManager().getPackageInfo(PACKAGE,PackageManager.GET_SIGNATURES);
            if(p.signatures==null||p.signatures.length!=1)return false;
            StringBuilder hex=new StringBuilder();for(byte b:MessageDigest.getInstance("SHA-256").digest(p.signatures[0].toByteArray()))hex.append(String.format(Locale.ROOT,"%02x",b&255));
            return SIGNER.equals(hex.toString());
        }catch(Exception absent){return false;}
    }
    public static void remove(Activity activity){
        if(!available(activity))return;
        try{activity.startActivity(new Intent(Intent.ACTION_UNINSTALL_PACKAGE,Uri.parse("package:"+PACKAGE)));}
        catch(ActivityNotFoundException|SecurityException e){Toast.makeText(activity,"Remove OpenSAAB Setup in Android Settings → Apps. Keep OpenSAAB T2 installed.",Toast.LENGTH_LONG).show();}
    }
    private SetupCleanup(){}
}
