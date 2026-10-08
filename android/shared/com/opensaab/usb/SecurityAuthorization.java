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

/** Operator-entered service password, encrypted locally and checked by the server. */
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
    static void validate(String text){
        if(text==null||!text.matches("[A-Za-z0-9_-]{1,64}"))throw new IllegalArgumentException("Enter the case-sensitive service password. Do not add spaces.");
    }
    public static void save(Context c,String text)throws Exception{
        validate(text);Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding");cipher.init(Cipher.ENCRYPT_MODE,key());
        byte[] encrypted=cipher.doFinal(text.getBytes(StandardCharsets.US_ASCII));
        byte[] out=new byte[12+encrypted.length];System.arraycopy(cipher.getIV(),0,out,0,12);System.arraycopy(encrypted,0,out,12,encrypted.length);
        Files.write(file(c).toPath(),out);
    }
    private static String load(Context c)throws Exception{
        File f=file(c);if(!f.isFile()||f.length()>256)throw new IOException("Security access password required");
        byte[] data=Files.readAllBytes(f.toPath());if(data.length<29)throw new IOException("Saved password invalid");
        Cipher cipher=Cipher.getInstance("AES/GCM/NoPadding");cipher.init(Cipher.DECRYPT_MODE,key(),new GCMParameterSpec(128,Arrays.copyOf(data,12)));
        String text=new String(cipher.doFinal(Arrays.copyOfRange(data,12,data.length)),StandardCharsets.US_ASCII);validate(text);return text;
    }
    public static String bearer(Context c)throws IOException{
        try{return load(c);}catch(Exception e){throw new IOException("Enter your security access password before processing.");}
    }
    public static void forget(Context c){file(c).delete();}
    public static boolean available(Context c){try{load(c);return true;}catch(Exception e){return false;}}
    public static void show(Activity a){show(a,()->{});}
    public static void show(Activity a,Runnable saved){
        LinearLayout layout=new LinearLayout(a);layout.setOrientation(LinearLayout.VERTICAL);int pad=(int)(20*a.getResources().getDisplayMetrics().density);layout.setPadding(pad,pad,pad,pad);
        TextView info=new TextView(a);
        String status=available(a)?"A password is saved on this device.":"Enter the OpenSAAB service password.";
        info.setText(status+"\n\nThe password is case-sensitive and will be checked by the server when you process security data. This is not your vehicle's security code. It is saved securely on this device.");layout.addView(info);
        EditText input=new EditText(a);input.setHint("Case-sensitive password");input.setSingleLine(true);input.setInputType(InputType.TYPE_CLASS_TEXT|InputType.TYPE_TEXT_VARIATION_PASSWORD|InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        if(Build.VERSION.SDK_INT>=26)input.setImportantForAutofill(android.view.View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        layout.addView(input);
        AlertDialog d=new AlertDialog.Builder(a).setTitle("Security access password").setView(layout).setNegativeButton("Close",null).setNeutralButton("Forget password",(x,w)->{file(a).delete();Toast.makeText(a,"Password removed",Toast.LENGTH_SHORT).show();}).setPositiveButton("Save",null).create();
        d.setOnShowListener(x->{d.getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);d.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(v->{
            try{save(a,input.getText().toString());input.setText("");d.dismiss();Toast.makeText(a,"Password saved",Toast.LENGTH_SHORT).show();saved.run();}
            catch(IllegalArgumentException e){input.setError(e.getMessage());}catch(Exception e){input.setError("Could not save password securely. Please retry.");}
        });});d.show();
    }
    private SecurityAuthorization(){}
}
