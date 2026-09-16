// SPDX-License-Identifier: MPL-2.0
package com.opensaab.checker;

import android.app.*;
import android.os.*;
import android.content.*;
import android.content.pm.*;
import android.net.Uri;
import android.provider.Settings;
import android.widget.*;
import com.opensaab.usb.*;
import java.io.File;
import java.util.*;

/** Universal Java-only setup; installs separately versioned, signed emulator APKs. */
public final class MainActivity extends Activity {
    private TextView state,choice;private Button action,open,refresh;private Spinner channels;
    private final List<SetupRelease> releases=new ArrayList<>();private SetupRelease selected;
    private String preferred;private File ready;private boolean busy;
    private static final java.util.concurrent.atomic.AtomicBoolean DOWNLOADING=new java.util.concurrent.atomic.AtomicBoolean();
    private int dp(int v){return Math.round(v*getResources().getDisplayMetrics().density);}
    private TextView text(LinearLayout root,String value,int size){TextView t=new TextView(this);t.setText(value);t.setTextSize(size);t.setTextColor(0xffeaf4f7);t.setPadding(0,dp(6),0,dp(6));root.addView(t);return t;}
    private Button button(LinearLayout root,String value,Runnable run){Button b=new Button(this);b.setText(value);b.setAllCaps(false);b.setOnClickListener(v->run.run());root.addView(b,new LinearLayout.LayoutParams(-1,-2));return b;}
    private PackageInfo installed(String pkg){try{return getPackageManager().getPackageInfo(pkg,0);}catch(PackageManager.NameNotFoundException e){return null;}}
    private String preferredPackage(){return "armeabi-v7a".equals(preferred)?"com.opensaab.tech2.headunit32":"com.opensaab.tech2";}
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);
        preferred=SetupRelease.preferred(Build.VERSION.SDK_INT,Build.SUPPORTED_ABIS,installed("com.opensaab.tech2.headunit32")!=null,installed("com.opensaab.tech2")!=null);
        ScrollView scroll=new ScrollView(this);scroll.setFillViewport(true);scroll.setBackgroundColor(0xff0d1620);
        LinearLayout root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);int pad=dp(16);root.setPadding(pad,pad,pad,pad);
        root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(pad,pad+i.getSystemWindowInsetTop(),pad,pad+i.getSystemWindowInsetBottom());return i;});scroll.addView(root);
        root.addView(new BrandHeader(this,"OpenSAAB Setup"));
        text(root,"One setup. The right app for your Android.",22);
        ActivityManager.MemoryInfo mem=new ActivityManager.MemoryInfo();((ActivityManager)getSystemService(ACTIVITY_SERVICE)).getMemoryInfo(mem);
        choice=text(root,preferred.isEmpty()?"This device cannot run the current emulator.":("armeabi-v7a".equals(preferred)?"32-bit ARM Android":"64-bit ARM Android")+" · "+String.format(Locale.ROOT,"%.1f GB RAM",mem.totalMem/1073741824.0),18);
        if(mem.totalMem<2L*1024*1024*1024)text(root,"This device has limited RAM. Installation may work, but emulator speed and stability still need testing.",15);
        text(root,"1. Check this device and choose its app.\n2. Download the verified installer and approve Android’s installation prompt.\n3. Open OpenSAAB to choose your Saab software and language. It will download, check and extract the software for you.",16);
        text(root,"Use an internet connection for setup. Allow about 160 MB free. Existing installations keep their firmware and settings—do not uninstall them. After setup, local USB diagnostics can work offline; security access needs internet.",15);
        text(root,"Device checks stay here. Checking releases contacts OpenSAAB; downloads come from the official GitHub releases. No vehicle or account information is sent by Setup.",13);
        channels=new Spinner(this);root.addView(channels);channels.setVisibility(android.view.View.GONE);
        channels.setOnItemSelectedListener(new android.widget.AdapterView.OnItemSelectedListener(){public void onNothingSelected(android.widget.AdapterView<?> p){}public void onItemSelected(android.widget.AdapterView<?> p,android.view.View v,int pos,long id){if(pos<releases.size()){selected=releases.get(pos);ready=null;update();}}});
        state=text(root,"",15);state.setTextIsSelectable(true);
        action=button(root,"Download matching app",()->{if(ready!=null)install();else download();});action.setEnabled(false);
        open=button(root,"Open OpenSAAB · continue setup",()->{
            String pkg=selected==null?preferredPackage():selected.packageName;Intent launch=getPackageManager().getLaunchIntentForPackage(pkg);
            if(launch!=null)startActivity(launch);else state.setText("Finish installing OpenSAAB, then return here to open it.");
        });
        refresh=button(root,"Check available version",()->load());
        button(root,"Device requirements / copy report",()->DeviceCompatibility.show(this));
        setContentView(scroll);update();if(!preferred.isEmpty())load();else{state.setText("OpenSAAB requires Android 8 / API 26 or newer, and ARMv7 or ARM64 Android application support. The advertised processor or Android version alone is not sufficient.");refresh.setEnabled(false);}
    }
    @Override protected void onResume(){super.onResume();if(action!=null)update();}
    private void update(){
        if(action==null)return;
        String pkg=selected==null?preferredPackage():selected.packageName;PackageInfo p=installed(pkg);
        open.setVisibility(!preferred.isEmpty()&&p!=null?android.view.View.VISIBLE:android.view.View.GONE);
        if(selected!=null){
            choice.setText(selected.title()+(p==null?"":"\nInstalled: "+p.versionName+" · same update channel"));
            boolean current=p!=null&&p.versionCode>=selected.code;
            action.setText(current?"This version or newer is installed":ready!=null?"Install verified app":"Download "+selected.title());
            action.setEnabled(!busy&&!current);
        }
        refresh.setEnabled(!busy&&!preferred.isEmpty());channels.setEnabled(!busy);
    }
    private void load(){
        if(busy)return;busy=true;state.setText("Checking available versions…");update();
        new Thread(()->{try{
            List<SetupRelease> list=SetupRelease.catalog();
            runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;releases.clear();List<String> titles=new ArrayList<>();int pick=0;
                for(SetupRelease r:list)if(Build.VERSION.SDK_INT>=26&&CompatibilityCheck.supportsAbi(Build.SUPPORTED_ABIS,r.abi)){if(r.abi.equals(preferred))pick=releases.size();releases.add(r);titles.add(r.title());}
                if(releases.isEmpty()){state.setText("No matching published version is available.");selected=null;action.setEnabled(false);return;}
                channels.setAdapter(new ArrayAdapter<String>(this,android.R.layout.simple_spinner_dropdown_item,titles));channels.setSelection(pick);channels.setVisibility(releases.size()>1?android.view.View.VISIBLE:android.view.View.GONE);selected=releases.get(pick);ready=null;
                state.setText("Ready. "+(selected.abi.equals("armeabi-v7a")?"The 32-bit release is experimental. ":"")+"Each architecture has its own version and updates.");update();
            });
        }catch(Exception e){runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;state.setText("Could not check releases. Check your internet connection and tap Check available version. An installed OpenSAAB app can still be opened offline.");update();});}},"setup-catalog").start();
    }
    private void download(){
        if(selected==null||busy)return;
        if(!DOWNLOADING.compareAndSet(false,true)){state.setText("A download is already finishing. Please check again shortly.");return;}
        busy=true;SetupRelease release=selected;state.setText("Downloading verified OpenSAAB installer…");update();
        new Thread(()->{try{
            File file=release.download(getApplicationContext(),percent->runOnUiThread(()->{if(!isDestroyed())state.setText("Downloading OpenSAAB… "+percent+"%\nKeep Setup open. Your firmware will be prepared inside OpenSAAB after installation.");}));
            runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;ready=file;state.setText("Download verified. Tap Install verified app. Android will ask you to confirm.");update();});
        }catch(Exception e){runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;ready=null;state.setText("Download was not completed. "+friendly(e)+"\nTap Download to retry. Your installed app has not been changed.");update();});}
        finally{DOWNLOADING.set(false);}},"setup-download").start();
    }
    private static String friendly(Exception e){
        if(e instanceof java.net.UnknownHostException||e instanceof java.net.SocketTimeoutException)return "Check your internet connection.";
        String m=e.getMessage();return m==null?"Please check your connection and free storage.":m;
    }
    private void install(){
        if(selected==null||ready==null||busy)return;
        if(Build.VERSION.SDK_INT>=26&&!getPackageManager().canRequestPackageInstalls()){
            new AlertDialog.Builder(this).setTitle("Allow OpenSAAB Setup to install")
                .setMessage("Android requires permission to install the downloaded app. Enable Allow from this source, return here, then tap Install verified app. Keep Play Protect enabled.")
                .setPositiveButton("Open Android settings",(d,w)->{try{startActivity(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,Uri.parse("package:"+getPackageName())));}catch(ActivityNotFoundException e){state.setText("Open Android Settings → Apps → Special access → Install unknown apps, and allow OpenSAAB Setup.");}}).setNegativeButton("Not now",null).show();return;
        }
        busy=true;update();SetupRelease release=selected;File file=ready;
        new Thread(()->{try{release.verify(getApplicationContext(),file);
            runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;update();Uri uri=Uri.parse("content://com.opensaab.checker.installers/"+file.getName());
                Intent i=new Intent(Intent.ACTION_INSTALL_PACKAGE).setDataAndType(uri,"application/vnd.android.package-archive").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION).putExtra(Intent.EXTRA_RETURN_RESULT,true);i.setClipData(ClipData.newRawUri("OpenSAAB installer",uri));
                try{startActivityForResult(i,42);state.setText("Complete Android’s installation prompt. Then open OpenSAAB to choose software and language.");}catch(ActivityNotFoundException e){state.setText("This device has no compatible Android package installer. The verified download remains here; contact OpenSAAB support.");}
            });
        }catch(Exception e){runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;ready=null;state.setText("Installer could not be verified. "+friendly(e));update();});}},"setup-verify").start();
    }
    @Override protected void onActivityResult(int request,int result,Intent data){super.onActivityResult(request,result,data);if(request==42){PackageInfo p=selected==null?null:installed(selected.packageName);state.setText(p!=null&&p.versionCode>=selected.code?"OpenSAAB is installed. Open it to continue software setup or use your existing firmware.":"Installation was cancelled or did not finish. Tap Install verified app to retry. Do not uninstall an existing app to troubleshoot an update.");update();}}
}
