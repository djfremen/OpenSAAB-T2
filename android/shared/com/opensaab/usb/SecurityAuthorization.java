// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.Context;
import android.os.Build;
import android.security.keystore.*;
import android.text.InputType;
import android.view.WindowManager;
import android.widget.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.security.KeyStore;
import java.util.*;
import javax.crypto.*;
import javax.crypto.spec.GCMParameterSpec;

/** Owner pilot credentials are entered by the operator, never shipped in an APK.
 * Stored with Android Keystore encryption under noBackupFilesDir. Server expiry,
 * revocation, vehicle entitlement and quotas are authoritative.
 */
public final class SecurityAuthorization {
    private static final String ALIAS="opensaab-owner-security-v1";
    private static File file(Context c){return new File(c.getNoBackupFilesDir(),"security-authorization.enc");}
    private static javax.crypto.SecretKey key()throws Exception{
        KeyStore ks=KeyStore.getInstance("AndroidKeyStore");ks.load(null);
        if(ks.containsAlias(ALIAS))return (javax.crypto.SecretKey)ks.getKey(ALIAS,null);
        KeyGenerator gen=KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES,"AndroidKeyStore");
        gen.init(new KeyGenParameterSpec.Builder(ALIAS,KeyProperties.PURPOSE_ENCRYPT|KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build());
        return gen.generateKey();
    }
    static long validate(String text){
        if(text==null||!text.matches("[0-9]{10}\\.[A-Za-z0-9_-]{43}"))throw new IllegalArgumentException("Use the complete owner authorization provided by OpenSAAB.");
        long expires=Long.parseLong(text.substring(0,10));
        if(expires<=System.currentTimeMillis()/1000)throw new IllegalArgumentException("Authorization expired. Request a new owner test authorization.");
        return expires;
    }
    public static void save(Context c,String text)throws Exception{
        validate(text);Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding");cipher.init(Cipher.ENCRYPT_MODE,key());
        byte[] encrypted=cipher.doFinal(text.getBytes(StandardCharsets.US_ASCII));
        byte[] out=new byte[12+encrypted.length];System.arraycopy(cipher.getIV(),0,out,0,12);System.arraycopy(encrypted,0,out,12,encrypted.length);
        Files.write(file(c).toPath(),out);
    }
    private static String load(Context c)throws Exception{
        File f=file(c);if(!f.isFile()||f.length()>256)throw new IOException("Owner authorization required");
        byte[] data=Files.readAllBytes(f.toPath());if(data.length<29)throw new IOException("Owner authorization invalid");
        Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding");cipher.init(Cipher.DECRYPT_MODE,key(),new GCMParameterSpec(128,Arrays.copyOf(data,12)));
        String text=new String(cipher.doFinal(Arrays.copyOfRange(data,12,data.length)),StandardCharsets.US_ASCII);validate(text);return text;
    }
    public static String bearer(Context c)throws IOException{
        try{return load(c).substring(11);}catch(Exception e){throw new IOException("Add or renew your owner-test authorization before processing security access.");}
    }
    public static boolean available(Context c){try{load(c);return true;}catch(Exception e){return false;}}
    public static void show(Activity a){
        LinearLayout layout=new LinearLayout(a);layout.setOrientation(LinearLayout.VERTICAL);int pad=(int)(20*a.getResources().getDisplayMetrics().density);layout.setPadding(pad,pad,pad,pad);
        TextView info=new TextView(a);
        String status="No current owner authorization.";
        try{status="Authorization saved until "+java.text.DateFormat.getDateTimeInstance().format(new Date(Long.parseLong(load(a).substring(0,10))*1000))+". Server approval still applies.";}catch(Exception ignored){}
        info.setText(status+"\n\nThis limited pilot is for approved vehicles only. Paste the private authorization provided by OpenSAAB. It is not your vehicle security code. Do not share it or include it in reports.");layout.addView(info);
        EditText input=new EditText(a);input.setHint("Private owner authorization");input.setSingleLine(true);input.setInputType(InputType.TYPE_CLASS_TEXT|InputType.TYPE_TEXT_VARIATION_PASSWORD|InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        if(Build.VERSION.SDK_INT>=26)input.setImportantForAutofill(android.view.View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        layout.addView(input);
        AlertDialog d=new AlertDialog.Builder(a).setTitle("Security access authorization").setView(layout).setNegativeButton("Close",null).setNeutralButton("Remove authorization",(x,w)->{file(a).delete();Toast.makeText(a,"Owner authorization removed",Toast.LENGTH_SHORT).show();}).setPositiveButton("Save",null).create();
        d.setOnShowListener(x->{d.getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);d.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(v->{
            try{save(a,input.getText().toString().trim());input.setText("");d.dismiss();Toast.makeText(a,"Owner authorization saved",Toast.LENGTH_SHORT).show();}
            catch(IllegalArgumentException e){input.setError(e.getMessage());}catch(Exception e){input.setError("Could not save authorization securely. Please retry.");}
        });});d.show();
    }
    private SecurityAuthorization(){}
}
