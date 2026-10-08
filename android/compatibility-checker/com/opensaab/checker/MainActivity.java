// SPDX-License-Identifier: MPL-2.0
package com.opensaab.checker;

import android.app.*;
import android.os.*;
import android.content.*;
import android.content.pm.*;
import android.net.Uri;
import android.provider.Settings;
import android.widget.*;
import android.view.View;
import android.view.ViewGroup;
import android.view.Gravity;
import com.opensaab.usb.*;
import java.io.File;
import java.util.*;

/** Universal Java-only setup; installs separately versioned, signed emulator APKs. */
public final class MainActivity extends Activity {
    private TextView state,choice,installationProblem;private Button action,open,refresh,migrationHelp,removeSetup;private Spinner channels;
    private final List<SetupRelease> releases=new ArrayList<>();private SetupRelease selected;
    private FrameLayout viewport;
    private LinearLayout introduction, installer;
    private int layoutWidth = -1, layoutHeight = -1;
    private boolean wideLayout;
    private boolean resumed;
    private String installStep="idle";
    private AlertDialog sourceDialog;
    private AlertDialog cleanupDialog;
    private String preferred;private File ready;private boolean busy;
    private static final java.util.concurrent.atomic.AtomicBoolean DOWNLOADING=new java.util.concurrent.atomic.AtomicBoolean();
    private int dp(int v){return Math.round(v*getResources().getDisplayMetrics().density);}
    private TextView text(LinearLayout root,String value,int size){TextView t=new TextView(this);t.setText(value);t.setTextSize(size);t.setTextColor(0xffeaf4f7);t.setPadding(0,dp(6),0,dp(6));root.addView(t);return t;}
    private Button button(LinearLayout root,String value,Runnable run){Button b=new Button(this);b.setText(value);b.setAllCaps(false);b.setMinHeight(dp(48));b.setOnClickListener(v->run.run());root.addView(b,new LinearLayout.LayoutParams(-1,-2));return b;}
    private PackageInfo installed(String pkg){try{return getPackageManager().getPackageInfo(pkg,PackageManager.GET_SIGNATURES);}catch(PackageManager.NameNotFoundException e){return null;}}
    private String preferredPackage(){return "armeabi-v7a".equals(preferred)?"com.opensaab.tech2.headunit32":"com.opensaab.tech2";}
    @Override public void onCreate(Bundle saved){
        super.onCreate(saved);
        preferred=SetupRelease.preferred(Build.VERSION.SDK_INT,Build.SUPPORTED_ABIS,installed("com.opensaab.tech2.headunit32")!=null,installed("com.opensaab.tech2")!=null);
        viewport = new FrameLayout(this);
        viewport.setBackgroundColor(0xff0d1620);
        introduction = new LinearLayout(this);
        introduction.setOrientation(LinearLayout.VERTICAL);
        installer = new LinearLayout(this);
        installer.setOrientation(LinearLayout.VERTICAL);
        LinearLayout root = introduction;
        root.addView(new BrandHeader(this,"OpenSAAB Setup"));
        text(root,"Install OpenSAAB",22);
        ActivityManager.MemoryInfo mem=new ActivityManager.MemoryInfo();((ActivityManager)getSystemService(ACTIVITY_SERVICE)).getMemoryInfo(mem);
        choice=text(installer,preferred.isEmpty()?"This device cannot run the current emulator.":("armeabi-v7a".equals(preferred)?"32-bit ARM Android":"64-bit ARM Android")+" · "+String.format(Locale.ROOT,"%.1f GB RAM",mem.totalMem/1073741824.0),18);
        if(mem.totalMem<2L*1024*1024*1024)text(root,"This device has limited RAM. Installation may work, but emulator speed and stability still need testing.",15);
        text(root,"Setup chooses the right app for this device. Approve Android’s installation prompt; OpenSAAB will open automatically.",16);
        text(root,"Use internet for setup and allow 160 MB free. Updates keep your software and settings. Local USB diagnostics work offline after setup.",15);
        text(root,"Release checks contact OpenSAAB; verified downloads come from GitHub. No vehicle information is sent by Setup.",13);
        root = installer;
        channels=new Spinner(this);root.addView(channels);channels.setVisibility(android.view.View.GONE);
        channels.setOnItemSelectedListener(new android.widget.AdapterView.OnItemSelectedListener(){public void onNothingSelected(android.widget.AdapterView<?> p){}public void onItemSelected(android.widget.AdapterView<?> p,android.view.View v,int pos,long id){if(pos<releases.size()){SetupRelease next=releases.get(pos);if(selected==null||!selected.sha.equals(next.sha)){selected=next;ready=null;setInstallStep("idle");}update();}}});
        installationProblem=text(root,"",16);
        installationProblem.setTextIsSelectable(true);
        installationProblem.setVisibility(View.GONE);
        state=text(root,"",15);state.setTextIsSelectable(true);
        action=button(root,"Install OpenSAAB",()->{if(ready!=null)install();else download();});action.setEnabled(false);
        open=button(root,"Open OpenSAAB · continue setup",()->{
            String pkg=selected==null?preferredPackage():selected.packageName;Intent launch=getPackageManager().getLaunchIntentForPackage(pkg);
            if(launch!=null){startActivity(launch);finishAndRemoveTask();}else state.setText("Finish installing OpenSAAB, then return here to open it.");
        });
        removeSetup=button(root,"Remove Setup · keep OpenSAAB",()->offerCleanup(true));
        removeSetup.setVisibility(View.GONE);
        migrationHelp=button(root,"Why can’t this app be updated?",()->new AlertDialog.Builder(this)
                .setTitle("Your existing app is protected")
                .setMessage("Android cannot update an app using a different signing key. This can happen with an earlier Android Studio or development build. Downloading again will not fix it.\n\nKeep using Open existing OpenSAAB. Before moving to the public release, preserve the reports, firmware and settings you need. Setup cannot copy another app’s private data. A development build may allow a computer-assisted backup; contact OpenSAAB support before removing it. Uninstalling erases its private data.")
                .setPositiveButton("Got it",null).show());
        migrationHelp.setVisibility(View.GONE);
        refresh=button(root,"Check available version",()->load());
        button(root,"Device requirements / copy report",()->DeviceCompatibility.show(this));
        viewport.setOnApplyWindowInsetsListener((v, insets) -> {
            viewport.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                    insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            viewport.post(this::arrangeForWindow);
            return insets;
        });
        viewport.addOnLayoutChangeListener((v,l,t,r,b,ol,ot,or,ob) -> viewport.post(this::arrangeForWindow));
        setContentView(viewport);restoreInstall();update();if(selected!=null){state.setText("Resume your verified installation.");}else if(!preferred.isEmpty())load();else{state.setText("OpenSAAB requires Android 8 / API 26 or newer, and ARMv7 or ARM64 Android application support. The advertised processor or Android version alone is not sufficient.");refresh.setEnabled(false);}
    }
    /** Use usable window dp, not device names or a fixed pixel resolution. Keep the
     * existing controls and download state alive when rotation or window size changes. */
    private void arrangeForWindow() {
        int width = viewport.getWidth() - viewport.getPaddingLeft() - viewport.getPaddingRight();
        int height = viewport.getHeight() - viewport.getPaddingTop() - viewport.getPaddingBottom();
        if (width <= 0 || height <= 0 || (width == layoutWidth && height == layoutHeight)) return;
        layoutWidth = width;
        layoutHeight = height;
        float density = getResources().getDisplayMetrics().density;
        // At larger font scales leave room for each column's readable text.
        float minimumWidthDp = 720 * Math.max(1f, getResources().getConfiguration().fontScale);
        wideLayout = width > height && width / density >= minimumWidthDp;
        detach(introduction);
        detach(installer);
        viewport.removeAllViews();
        int pad = dp(16);
        introduction.setPadding(pad, pad, pad, pad);
        installer.setPadding(pad, pad, pad, pad);
        if (wideLayout) {
            LinearLayout columns = new LinearLayout(this);
            columns.setOrientation(LinearLayout.HORIZONTAL);
            columns.addView(scroller(introduction), new LinearLayout.LayoutParams(0, -1, 1f));
            columns.addView(scroller(installer), new LinearLayout.LayoutParams(0, -1, 1f));
            viewport.addView(columns, new FrameLayout.LayoutParams(-1, -1));
        } else {
            LinearLayout stack = new LinearLayout(this);
            stack.setOrientation(LinearLayout.VERTICAL);
            stack.addView(introduction);
            stack.addView(installer);
            FrameLayout.LayoutParams bounds = new FrameLayout.LayoutParams(Math.min(width, dp(600)), -1, Gravity.TOP | Gravity.CENTER_HORIZONTAL);
            viewport.addView(scroller(stack), bounds);
        }
    }
    private ScrollView scroller(View child) {
        ScrollView scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        scroll.setClipToPadding(false);
        scroll.addView(child, new ScrollView.LayoutParams(-1, -2));
        return scroll;
    }
    private static void detach(View view) {
        if (view.getParent() instanceof ViewGroup) ((ViewGroup) view.getParent()).removeView(view);
    }
    @Override protected void onResume(){super.onResume();resumed=true;if(action!=null){update();resumeInstall();}}
    @Override protected void onPause(){resumed=false;super.onPause();}
    @Override protected void onDestroy(){if(cleanupDialog!=null){cleanupDialog.dismiss();cleanupDialog=null;}if(sourceDialog!=null)sourceDialog.dismiss();super.onDestroy();}
    /** Only a recognized, signed and launchable emulator can make Setup optional. */
    private String removableAfter(){
        String pkg=selected==null?preferredPackage():selected.packageName;
        if(preferred.isEmpty()||!removalEligible(installed(pkg),getPackageManager().getLaunchIntentForPackage(pkg)))return null;
        return pkg;
    }
    static boolean removalEligible(PackageInfo app,Intent launch){return SetupRelease.releaseSigned(app)&&launch!=null;}
    static Intent selfRemovalIntent(Context context){
        return new Intent(Intent.ACTION_UNINSTALL_PACKAGE,Uri.parse("package:"+context.getPackageName()))
                .putExtra(Intent.EXTRA_RETURN_RESULT,true);
    }
    private void offerCleanup(boolean requested){
        String pkg=removableAfter();
        if(pkg==null||busy||!resumed||cleanupDialog!=null||isFinishing()||isDestroyed())return;
        android.content.SharedPreferences prefs=getSharedPreferences("setup-completion",MODE_PRIVATE);
        if(!requested){
            PackageInfo p=installed(pkg);
            if(selected==null||p==null||p.versionCode<selected.code||prefs.getBoolean("offered-"+pkg,false))return;
        }
        prefs.edit().putBoolean("offered-"+pkg,true).apply();
        cleanupDialog=new AlertDialog.Builder(this).setTitle("OpenSAAB is installed")
                .setMessage("Setup has finished its job. Remove OpenSAAB Setup to keep just the main app. Your OpenSAAB firmware, reports and settings will stay in place. Use Check for updates inside OpenSAAB for future updates.\n\nAndroid will ask you to confirm removing Setup. You can keep it if you prefer.")
                .setPositiveButton("Remove Setup",(d,w)->removeOnlySetup())
                .setNegativeButton("Keep Setup",null).create();
        cleanupDialog.setOnDismissListener(d->cleanupDialog=null);cleanupDialog.show();
    }
    private void removeOnlySetup(){
        if(removableAfter()==null||busy){state.setText("OpenSAAB is not ready. Keep Setup to finish installation.");return;}
        try{startActivityForResult(selfRemovalIntent(this),43);}
        catch(ActivityNotFoundException|SecurityException e){
            state.setText("Open Android Settings → Apps → OpenSAAB Setup → Uninstall. Keep the main OpenSAAB app installed.");
            try{startActivity(new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,Uri.parse("package:"+getPackageName())));}catch(ActivityNotFoundException ignored){}
        }
    }
    private void update(){
        if(action==null)return;
        String pkg=selected==null?preferredPackage():selected.packageName;PackageInfo p=installed(pkg);
        boolean conflict=p!=null&&!SetupRelease.releaseSigned(p);
        open.setVisibility(!preferred.isEmpty()&&p!=null?View.VISIBLE:View.GONE);
        open.setText(conflict?"Open existing OpenSAAB":"Open OpenSAAB · continue setup");
        installationProblem.setVisibility(conflict?View.VISIBLE:View.GONE);
        migrationHelp.setVisibility(conflict?View.VISIBLE:View.GONE);
        state.setVisibility(conflict?View.GONE:View.VISIBLE);
        if(conflict) installationProblem.setText("Your existing OpenSAAB uses a different signing key. Android cannot update it with the public release. Keep using the existing app; your firmware and reports have not been changed. See the guidance below before migrating.");
        if(selected!=null){
            choice.setText(selected.title()+(p==null?"":"\nInstalled: "+SetupRelease.installedVersion(p)+(conflict?"":" · same update channel")));
            boolean current=p!=null&&p.versionCode>=selected.code;
            action.setText(conflict?"Public update unavailable for this installation":current?"This version or newer is installed":ready!=null?"Retry installation":"Install OpenSAAB");
            action.setEnabled(!busy&&!current&&!conflict);
        } else action.setEnabled(false);
        refresh.setEnabled(!busy&&!preferred.isEmpty());channels.setEnabled(!busy);
        removeSetup.setVisibility(removableAfter()!=null?View.VISIBLE:View.GONE);removeSetup.setEnabled(!busy);

    }
    private void load(){
        if(busy)return;setInstallStep("idle");ready=null;busy=true;state.setText("Checking available versions…");update();
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
        PackageInfo existing=installed(selected.packageName);
        if(existing!=null&&!SetupRelease.releaseSigned(existing)){update();return;}
        if(!DOWNLOADING.compareAndSet(false,true)){state.setText("A download is already finishing. Please check again shortly.");return;}
        busy=true;SetupRelease release=selected;state.setText("Downloading verified OpenSAAB installer…");update();
        new Thread(()->{try{
            File file=release.download(getApplicationContext(),percent->runOnUiThread(()->{if(!isDestroyed())state.setText("Downloading OpenSAAB… "+percent+"%\nKeep Setup open. Your firmware will be prepared inside OpenSAAB after installation.");}));
            runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;ready=file;setInstallStep("ready");state.setText("Download verified. Continuing to Android’s installation prompt…");update();resumeInstall();});
        }catch(Exception e){runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;ready=null;state.setText("Download was not completed. "+friendly(e)+"\nTap Download to retry. Your installed app has not been changed.");update();});}
        finally{DOWNLOADING.set(false);}},"setup-download").start();
    }
    private static String friendly(Exception e){
        if(e instanceof java.net.UnknownHostException||e instanceof java.net.SocketTimeoutException)return "Check your internet connection.";
        String m=e.getMessage();return m==null?"Please check your connection and free storage.":m;
    }
    private void install(){
        if(selected==null||ready==null||busy)return;
        if(!resumed){setInstallStep("ready");return;}
        if(Build.VERSION.SDK_INT>=26&&!getPackageManager().canRequestPackageInstalls()){
            if(sourceDialog!=null&&sourceDialog.isShowing())return;
            setInstallStep("idle");
            sourceDialog=new AlertDialog.Builder(this).setTitle("Allow OpenSAAB Setup to install")
                .setMessage("Enable Allow from this source in Android Settings, then return here. Installation will continue automatically. Keep Play Protect enabled.")
                .setPositiveButton("Open Android settings",(d,w)->{try{setInstallStep("permission");startActivityForResult(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,Uri.parse("package:"+getPackageName())),44);}catch(ActivityNotFoundException e){setInstallStep("idle");state.setText("Open Android Settings → Apps → Special access → Install unknown apps, and allow OpenSAAB Setup. Then tap Retry installation.");}})
                .setNegativeButton("Not now",(d,w)->state.setText("Installation paused. Tap Retry installation when ready.")).create();
            sourceDialog.setOnDismissListener(d->sourceDialog=null);sourceDialog.show();return;
        }
        busy=true;setInstallStep("ready");update();SetupRelease release=selected;File file=ready;
        new Thread(()->{try{release.verify(getApplicationContext(),file);
            runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;update();if(!resumed)return;Uri uri=Uri.parse("content://com.opensaab.checker.installers/"+file.getName());
                Intent i=new Intent(Intent.ACTION_INSTALL_PACKAGE).setDataAndType(uri,"application/vnd.android.package-archive").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION).putExtra(Intent.EXTRA_RETURN_RESULT,true);i.setClipData(ClipData.newRawUri("OpenSAAB installer",uri));
                try{setInstallStep("installer");startActivityForResult(i,42);state.setText("Confirm installation in Android. OpenSAAB will open when it finishes.");}catch(ActivityNotFoundException e){setInstallStep("idle");state.setText("This device has no compatible Android package installer. The verified download remains here; contact OpenSAAB support.");}
            });
        }catch(Exception e){runOnUiThread(()->{if(isFinishing()||isDestroyed())return;busy=false;ready=null;setInstallStep("idle");state.setText("Installer could not be verified. "+friendly(e));update();});}},"setup-verify").start();
    }
    private void setInstallStep(String step){
        installStep=step;
        android.content.SharedPreferences.Editor e=getSharedPreferences("pending-install",MODE_PRIVATE).edit().putString("step",step);
        if(selected!=null&&ready!=null)e.putString("release",selected.json().toString());else e.remove("release");
        e.apply();
    }
    private void restoreInstall(){
        android.content.SharedPreferences p=getSharedPreferences("pending-install",MODE_PRIVATE);
        try{
            String raw=p.getString("release","");if(raw.isEmpty())return;
            SetupRelease restored=new SetupRelease(new org.json.JSONObject(raw));
            if(preferred.isEmpty()||!CompatibilityCheck.supportsAbi(Build.SUPPORTED_ABIS,restored.abi))return;
            selected=restored;ready=new File(new File(getFilesDir(),"installers"),selected.sha+".apk");
            installStep=p.getString("step","idle");
            if(!ready.isFile()&&!installStep.equals("installer")){ready=null;setInstallStep("idle");}
        }catch(Exception invalid){selected=null;ready=null;setInstallStep("idle");}
    }
    private void resumeInstall(){
        if(!resumed||busy||selected==null||isFinishing()||isDestroyed())return;
        if(installStep.equals("installer")){
            setInstallStep("idle");
            PackageInfo p=installed(selected.packageName);
            Intent launch=getPackageManager().getLaunchIntentForPackage(selected.packageName);
            if(SetupRelease.releaseSigned(p)&&p.versionCode>=selected.code&&launch!=null){
                state.setText("Installed. Opening OpenSAAB…");
                try{startActivity(launch);if(ready!=null)ready.delete();ready=null;setInstallStep("idle");finishAndRemoveTask();}
                catch(ActivityNotFoundException|SecurityException e){state.setText("OpenSAAB is installed. Use Open OpenSAAB to continue.");}
            }else state.setText("Installation was cancelled or did not finish. Tap Retry installation to continue. Your existing app is kept.");
            update();return;
        }
        if(installStep.equals("permission")){
            setInstallStep("idle");
            if(Build.VERSION.SDK_INT>=26&&!getPackageManager().canRequestPackageInstalls()){
                state.setText("Installation permission was not granted. Tap Retry installation when ready.");update();return;
            }
            install();return;
        }
        if(installStep.equals("ready"))install();
    }
    @Override protected void onActivityResult(int request,int result,Intent data){
        super.onActivityResult(request,result,data);
        // onResume is the single continuation point, including Android installers that omit result data.
        if(request==43){state.setText("Setup was kept. OpenSAAB is ready to use.");update();}
    }
}
