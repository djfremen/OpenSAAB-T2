// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.database.Cursor;
import android.net.Uri;
import android.os.*;
import android.provider.OpenableColumns;
import android.widget.*;
import java.io.*;
import java.net.*;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import org.json.JSONObject;

/** User-selected card downloads and support imports; OEM firmware is external by default. */
public final class FirmwareActivity extends Activity {
    private FirmwareStore store;
    private TextView status,current,details,heading,subtitle,stage,supportStatus;
    private LinearLayout welcomePanel,choosePanel,supportPanel,readyPanel,transferPanel,advancedPanel;
    private Button next,download,more,retry;
    private boolean guided,welcome,chooseRequested,failed,bundledSupport;
    private Callable<String> retryWork;
    private Spinner choices;
    private final List<Button> actions=new ArrayList<>();
    private Button cancel;
    private ProgressBar progress;
    private final ExecutorService worker=Executors.newSingleThreadExecutor();
    private volatile boolean busy,cancelled,closed;
    private volatile HttpURLConnection connection;
    private static final int CARD=51,SUPPORT=52;
    public void onCreate(Bundle state){
        super.onCreate(state);com.opensaab.usb.BackNavigation.install(this,this::leave);store=new FirmwareStore(getFilesDir());
        try{bundledSupport=BundledSupport.available(getAssets());}catch(IOException ignored){}
        guided=!store.missing().isEmpty();
        welcome=guided&&!getSharedPreferences("firmware_library",MODE_PRIVATE).getBoolean("setup_started",false);
        if(state!=null){welcome=state.getBoolean("welcome",welcome);chooseRequested=state.getBoolean("choose_requested",false);}
        ScrollView scroll=new ScrollView(this);scroll.setFillViewport(true);scroll.setBackgroundColor(0xff0d1620);
        LinearLayout root=new LinearLayout(this);root.setOrientation(LinearLayout.VERTICAL);scroll.addView(root);
        root.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(dp(20),i.getSystemWindowInsetTop()+dp(20),dp(20),i.getSystemWindowInsetBottom()+dp(16));return i;});
        root.addView(new BrandHeader(this,"OpenSAAB T2"));
        heading=label(root,"Welcome to OpenSAAB",28);subtitle=label(root,"Let’s get your diagnostic software ready.",16);
        current=label(root,"Checking your setup…",14);

        welcomePanel=panel(root);
        label(welcomePanel,"A little setup, then you’re ready",20);
        label(welcomePanel,CompatibilityCheck.verdict(Build.VERSION.SDK_INT,Build.SUPPORTED_ABIS,new StatFs(getFilesDir().getAbsolutePath()).getAvailableBytes(),AppBuildProfile.abi(this)),14);
        button(welcomePanel,"Check device compatibility",()->DeviceCompatibility.show(this));
        label(welcomePanel,"1. Choose your Saab software and language. English for North America is selected to get you started.",16);
        label(welcomePanel,"2. Connect to the internet to download it. We’ll check the download and unpack it for your first run.",16);
        label(welcomePanel,bundledSupport?"3. This development build includes the communication firmware and prepares it automatically.":"3. Import the three communication firmware files from your existing Tech2Win installation. They are not included in this app.",16);
        if(!bundledSupport)label(welcomePanel,"Tech2Wiki provides the Saab software ZIPs. A download source for the three support files has not been verified; setup will tell you what is still missing.",14);
        label(welcomePanel,"You can set this up without connecting to a vehicle. Keep about 140 MB free for installation and backups.",14);
        label(welcomePanel,"After setup, your installed software, USB diagnostics and saved reports can be used offline. Security access requires an internet connection to the OpenSAAB security-access API; it cannot be processed offline. Downloads, online vehicle details and sending reports also need internet. You can import a supported Saab image from a file in More options.",14);
        button(welcomePanel,"Get started",()->{welcome=false;getSharedPreferences("firmware_library",MODE_PRIVATE).edit().putBoolean("setup_started",true).apply();render();});

        choosePanel=panel(root);label(choosePanel,"Choose your software",20);
        label(choosePanel,"Version and language",14);
        choices=new Spinner(this);ArrayAdapter<FirmwareCatalog.Entry> adapter=new ArrayAdapter<>(this,android.R.layout.simple_spinner_dropdown_item,FirmwareCatalog.ENTRIES);choices.setAdapter(adapter);choices.setMinimumHeight(dp(56));choices.setContentDescription("Software version and language");choosePanel.addView(choices);
        String remembered=getSharedPreferences("firmware_library",MODE_PRIVATE).getString("selected_id",FirmwareCatalog.ENTRIES[0].id);for(int i=0;i<FirmwareCatalog.ENTRIES.length;i++)if(FirmwareCatalog.ENTRIES[i].id.equals(remembered))choices.setSelection(i);
        details=label(choosePanel,"",15);choices.setOnItemSelectedListener(new android.widget.AdapterView.OnItemSelectedListener(){public void onNothingSelected(android.widget.AdapterView<?> p){}public void onItemSelected(android.widget.AdapterView<?> p,android.view.View v,int i,long id){
            FirmwareCatalog.Entry e=FirmwareCatalog.ENTRIES[i];getSharedPreferences("firmware_library",MODE_PRIVATE).edit().putString("selected_id",e.id).apply();
            if(v instanceof TextView){((TextView)v).setTextColor(0xffeff5fa);((TextView)v).setTextSize(16);}
            details.setText(String.format(Locale.ROOT,"About %.1f MB to download from Tech2Wiki. Internet is needed for this step.\n\nWe’ll unpack the ZIP for you. The installed software uses 32 MB; you won’t need to download it each time you open the app.",e.downloadBytes/1048576.0)+(i==0?"\n\nRecommended starting choice: English · North American Operations (NAO).":"")+(FirmwareCatalog.runnable(e)?"":"\n\nThis version cannot run in this app yet. You can save it for later; it will not replace your installed software."));
        }});
        download=button(choosePanel,"Download and set up",()->{FirmwareCatalog.Entry e=(FirmwareCatalog.Entry)choices.getSelectedItem();job(()->downloadAndUse(e),true);});

        transferPanel=panel(root);stage=label(transferPanel,"Getting ready…",20);stage.setAccessibilityLiveRegion(android.view.View.ACCESSIBILITY_LIVE_REGION_POLITE);
        progress=new ProgressBar(this,null,android.R.attr.progressBarStyleHorizontal);progress.setMax(100);transferPanel.addView(progress);
        status=label(transferPanel,"",16);status.setAccessibilityLiveRegion(android.view.View.ACCESSIBILITY_LIVE_REGION_POLITE);
        cancel=new Button(this);cancel.setText("Cancel setup");cancel.setAllCaps(false);cancel.setOnClickListener(v->{cancelled=true;cancel.setEnabled(false);status.setText("Cancelling… Your completed steps will be kept.");HttpURLConnection c=connection;if(c!=null)c.disconnect();});transferPanel.addView(cancel);
        retry=new Button(this);retry.setText("Try again");retry.setAllCaps(false);retry.setOnClickListener(v->{if(retryWork!=null)job(retryWork,true);});transferPanel.addView(retry);
        root.removeView(transferPanel);root.addView(transferPanel,4);show(transferPanel,false);

        supportPanel=panel(root);label(supportPanel,"Let’s finish preparing the app",20);
        label(supportPanel,bundledSupport?"This development build includes communication firmware. Tap below to repair missing components.":"To run the original diagnostic software, import eprom.bin, opsys.dwn and candi.bin from a copy you are entitled to use. Select the extracted files, not the Windows MSI installer. We check each file before installing it.",16);
        supportStatus=label(supportPanel,"",14);
        if(bundledSupport)button(supportPanel,"Prepare app components",()->job(()->"App components are ready."));
        button(supportPanel,"Import communication files",()->pick(SUPPORT,true));

        readyPanel=panel(root);label(readyPanel,"You’re ready to connect",22);
        label(readyPanel,"Your software is installed and ready. You can now connect a supported USB adapter and tap Start, or select Run in emulation mode to explore without a vehicle.",16);
        label(readyPanel,"This software now opens from your phone. Internet may still be needed for online features such as security access.",14);
        button(readyPanel,"Continue to OpenSAAB",this::finish);

        more=button(root,"More options",()->{advancedPanel.setVisibility(advancedPanel.getVisibility()==android.view.View.VISIBLE?android.view.View.GONE:android.view.View.VISIBLE);more.setText(advancedPanel.getVisibility()==android.view.View.VISIBLE?"Hide options":"More options");});
        advancedPanel=panel(root);
        button(advancedPanel,"Choose a different version",()->{welcome=false;chooseRequested=true;render();});
        button(advancedPanel,"Import a ZIP or BIN instead",()->pick(CARD,false));
        button(advancedPanel,"Import communication files",()->pick(SUPPORT,true));
        label(advancedPanel,"Already have the software? Import a file without downloading. Changing software preserves a backup of your current working copy.",14);
        button(advancedPanel,"Installed versions / restore backup",this::installed);
        button(advancedPanel,"View Tech2Wiki catalog",()->{try{startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse("https://tech2wiki.com/content/tech2_pcmcia/available_tech2win_.bin-files/")));}catch(ActivityNotFoundException e){new AlertDialog.Builder(this).setMessage("Open tech2wiki.com in a browser to view the catalog.").setPositiveButton("OK",null).show();}});
        advancedPanel.setVisibility(android.view.View.GONE);
        next=new Button(this);next.setAllCaps(false);next.setText("Set up later");next.setOnClickListener(v->leave());root.addView(next);
        setContentView(scroll);render();job(()->{store.recover();return "";},false);
    }
    private int dp(int n){return Math.round(n*getResources().getDisplayMetrics().density);}
    private LinearLayout panel(LinearLayout root){LinearLayout p=new LinearLayout(this);p.setOrientation(LinearLayout.VERTICAL);p.setPadding(dp(16),dp(12),dp(16),dp(12));android.graphics.drawable.GradientDrawable bg=new android.graphics.drawable.GradientDrawable();bg.setColor(0xff192837);bg.setCornerRadius(dp(16));p.setBackground(bg);LinearLayout.LayoutParams lp=new LinearLayout.LayoutParams(-1,-2);lp.topMargin=dp(18);root.addView(p,lp);return p;}
    private TextView label(LinearLayout root,String text,int size){TextView v=new TextView(this);v.setText(text);v.setTextSize(size);v.setTextColor(0xffdce9f2);v.setPadding(0,dp(6),0,dp(6));v.setLineSpacing(dp(3),1);root.addView(v);return v;}
    private Button button(LinearLayout root,String text,Runnable action){Button b=new Button(this);b.setText(text);b.setAllCaps(false);b.setTextColor(0xff08271f);b.setBackgroundTintList(android.content.res.ColorStateList.valueOf(0xff73dcc4));b.setMinimumHeight(dp(52));b.setOnClickListener(v->action.run());root.addView(b);actions.add(b);return b;}
    private void show(android.view.View v,boolean visible){v.setVisibility(visible?android.view.View.VISIBLE:android.view.View.GONE);}
    private void render(){
        String missing=store.missing();boolean ready=missing.isEmpty();boolean cardReady=false;
        try{FirmwareFiles.validate("card.bin",new File(store.firmware,"card.bin"));cardReady=true;}catch(IOException ignored){}
        boolean choose=!welcome&&(!guided||chooseRequested||!cardReady);
        heading.setText(welcome?"Welcome to OpenSAAB":ready&&!choose?"Setup complete":choose?"Your Saab software":"Finish your setup");
        subtitle.setText(welcome?"Let’s get your diagnostic software ready.":ready?"Installed on your phone and ready to use.":choose?"Download once. Get ready to connect.":"Add the communication firmware to continue");
        try{current.setText(cardReady?"Installed: "+store.active().optString("label"):"No diagnostic software installed yet");}catch(Exception e){current.setText("Your setup needs attention. Open the options below to restore a backup.");}
        String supportMissing=store.missingSupport();supportStatus.setText("Still needed: "+supportMissing);
        show(welcomePanel,welcome&&!busy);show(choosePanel,choose&&!busy);show(supportPanel,!welcome&&!supportMissing.isEmpty()&&!busy);show(readyPanel,!welcome&&ready&&!busy);
        for(Button b:actions)b.setEnabled(!busy);choices.setEnabled(!busy);
        more.setVisibility(welcome||busy?android.view.View.GONE:android.view.View.VISIBLE);if(welcome||busy){advancedPanel.setVisibility(android.view.View.GONE);more.setText("More options");}
        next.setText(busy?"Leave setup":ready?"Back":"Set up later");
        show(cancel,busy);cancel.setEnabled(busy);show(retry,failed&&!busy&&retryWork!=null);
    }
    private void phase(String title,String text,boolean measured){runOnUiThread(()->{if(closed)return;show(transferPanel,true);stage.setText(title);status.setText(text);progress.setIndeterminate(!measured);if(measured)progress.setProgress(0);});}
    private void message(String text){phase("Preparing your software",text,false);}
    private String friendlyError(Exception e){
        if(e instanceof UnknownHostException||e instanceof SocketException||e instanceof SocketTimeoutException)return "We couldn’t reach the download server. Check your internet connection, then tap Try again. Anything already installed is kept.";
        String m=e.getMessage()==null?"Please try again.":e.getMessage();
        if(m.toLowerCase(Locale.ROOT).contains("sha")||m.toLowerCase(Locale.ROOT).contains("checksum")||m.contains("CRC"))return "The file did not pass its integrity check. Please download it again or choose another copy. Your installed software is kept.";
        return m;
    }
    private void leave(){if(!busy){finish();return;}new AlertDialog.Builder(this).setTitle("Leave setup?").setMessage("The current transfer will stop. Completed steps are saved, and you can return to setup later.").setPositiveButton("Leave setup",(d,w)->finish()).setNegativeButton("Keep setting up",null).show();}
    @android.annotation.SuppressLint("GestureBackNavigation") // API 33+ uses BackNavigation; this handles older Android.
    @Override public void onBackPressed(){leave();}
    @Override protected void onSaveInstanceState(Bundle out){super.onSaveInstanceState(out);out.putBoolean("welcome",welcome);out.putBoolean("choose_requested",chooseRequested);}
    private void job(Callable<String> work){job(work,true);}
    private void job(Callable<String> work,boolean visible){
        if(busy||closed)return;busy=true;cancelled=false;failed=false;retryWork=work;render();
        if(visible)phase("Getting ready…","Checking your setup before we begin.",false);
        worker.execute(()->{
            String result;boolean error=false;
            try(FirmwareGate.Lease lease=FirmwareGate.change()){
                if(SecurityAccessView.workflowBusy())throw new IOException("Finish security access processing first");
                // Also detect an orphan or manually launched native child from an earlier app process.
                java.lang.Process probe=new ProcessBuilder("pidof","libtech2_emu.so").start();
                if(!probe.waitFor(2,TimeUnit.SECONDS)){probe.destroy();throw new IOException("Cannot confirm the emulator is stopped");}
                if(probe.exitValue()==0)throw new IOException("Stop the active firmware session before changing images");
                if(probe.exitValue()!=1)throw new IOException("Cannot confirm the emulator is stopped");
                FirmwareFiles.check(()->cancelled);
                try{BundledSupport.ensure(getAssets(),store,()->cancelled);}
                catch(InterruptedIOException e){throw e;}
                catch(Exception e){android.util.Log.e("OpenSAAB","Bundled app components could not be prepared",e);throw new IOException("The app’s included components could not be prepared. Try again, or reinstall the app if this continues.",e);}
                result=work.call();
            }catch(Exception e){if(!cancelled)SupportReports.recordError(this,e,false);error=true;result=cancelled?"Setup paused. Completed steps are saved. You can try again when you’re ready.":friendlyError(e);}
            final String done=result;final boolean problem=error;
            runOnUiThread(()->{busy=false;if(closed)return;failed=problem;if(!problem&&visible){welcome=false;chooseRequested=false;}
                render();show(transferPanel,problem||!done.isEmpty());stage.setText(problem?(cancelled?"Setup paused":"Setup needs attention"):"Step complete");status.setText(done);progress.setIndeterminate(false);progress.setProgress(problem?0:100);
            });
        });
    }
    private String downloadAndUse(FirmwareCatalog.Entry e)throws Exception{
        File original=store.original(e.id);
        if(!original.isFile()){
            if(getFilesDir().getUsableSpace()<140L*1024*1024)throw new IOException("Need 140 MiB free to download, unpack and preserve your working card");
            File zip=File.createTempFile("firmware-",".zip",getCacheDir()),image=File.createTempFile("firmware-",".bin",getCacheDir());
            try{
                URL url=new URL(e.url);
                for(int redirects=0;;redirects++){
                    if(!url.getProtocol().equals("https")||!(url.getHost().equals("raw.githubusercontent.com")||url.getHost().equals("github.com")))throw new IOException("Unexpected download destination");
                    FirmwareFiles.check(()->cancelled);HttpURLConnection c=(HttpURLConnection)url.openConnection();connection=c;c.setConnectTimeout(15000);c.setReadTimeout(20000);c.setInstanceFollowRedirects(false);c.setRequestProperty("Accept-Encoding","identity");
                    phase("Connecting…","Connecting to Tech2Wiki. Please keep your internet connection on.",false);
                    int code=c.getResponseCode();
                    if(code==301||code==302||code==303||code==307||code==308){String location=c.getHeaderField("Location");c.disconnect();if(location==null||redirects>=3)throw new IOException("Download redirect failed");url=new URL(url,location);continue;}
                    if(code!=200)throw new IOException("Download server returned HTTP "+code);
                    long length=c.getContentLengthLong();if(length>FirmwareFiles.MAX_DOWNLOAD)throw new IOException("Download is too large");
                    phase("Downloading your software","Downloading "+e.label+". We’ll check and unpack it next.",true);
                    try(InputStream raw=c.getInputStream();InputStream in=new FilterInputStream(raw){long read;long last;public int read(byte[] b,int off,int len)throws IOException{int n=super.read(b,off,len);if(n>0){read+=n;long now=SystemClock.elapsedRealtime();if(now-last>200){last=now;final int percent=(int)Math.min(100,read*100/e.downloadBytes);runOnUiThread(()->{if(!closed){progress.setProgress(percent);status.setText("Downloaded "+percent+"% · We’ll unpack the software next.");}});}}return n;}}){FirmwareFiles.copy(in,zip,FirmwareFiles.MAX_DOWNLOAD,()->cancelled);}
                    finally{c.disconnect();connection=null;}break;
                }
                phase("Checking the download","Making sure the download is complete and matches the selected software.",false);
                FirmwareFiles.requireSha(zip,e.zipSha);
                phase("Unpacking for your first run","Extracting your software on this phone. This happens once for each version you install; there is no need to unzip anything yourself.",false);
                FirmwareFiles.card(zip,image,()->cancelled);
                phase("Checking the extracted software","Checking the unpacked files before installation.",false);FirmwareFiles.requireSha(image,e.imageSha);
                store.saveOriginal(image,e.id,e.label,e.imageSha,()->cancelled);
            }finally{if(connection!=null){connection.disconnect();connection=null;}zip.delete();image.delete();}
        }
        if(!FirmwareCatalog.runnable(e))return "Downloaded and verified. Russian 140.500 does not boot with the current profile, so your active version was preserved.";
        phase("Finishing installation","Saving your software on this phone and keeping a backup of any previous version.",false);store.activate(e.id,e.label,e.imageSha,()->cancelled);
        return "Download verified and unpacked. Your software is saved on this phone.";
    }
    private void pick(int request,boolean multiple){try{startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT).setType("*/*").addCategory(Intent.CATEGORY_OPENABLE).putExtra(Intent.EXTRA_ALLOW_MULTIPLE,multiple),request);}catch(ActivityNotFoundException e){new AlertDialog.Builder(this).setMessage("A file picker is needed to import your files. Install or enable the Files app, then try again.").setPositiveButton("OK",null).show();}}
    protected void onActivityResult(int request,int result,Intent data){
        super.onActivityResult(request,result,data);if(result!=RESULT_OK||data==null)return;
        List<Uri> uris=new ArrayList<>();if(data.getClipData()!=null){for(int i=0;i<data.getClipData().getItemCount();i++)uris.add(data.getClipData().getItemAt(i).getUri());}else if(data.getData()!=null)uris.add(data.getData());
        if(uris.isEmpty())return;
        if(request==SUPPORT){importSupport(uris);return;}
        if(request!=CARD)return;if(uris.size()!=1){phase("Choose one software file","Select a single Saab ZIP or BIN file.",false);progress.setIndeterminate(false);return;}
        job(()->{
            store.recover();for(Uri uri:uris){
                phase("Checking your files","Checking the selected files and saving them on your phone.",false);
                String name="imported-card.bin";try(Cursor c=getContentResolver().query(uri,new String[]{OpenableColumns.DISPLAY_NAME},null,null,null)){if(c!=null&&c.moveToFirst())name=c.getString(0);}
                if(name==null)name="imported-card.bin";name=name.length()>100?name.substring(0,100):name;
                File source=File.createTempFile("firmware-import-",".tmp",getCacheDir()),image=File.createTempFile("firmware-card-",".tmp",getCacheDir());
                try{
                    if(getFilesDir().getUsableSpace()<140L*1024*1024)throw new IOException("Need 140 MiB free for import and backup");
                    try(InputStream in=getContentResolver().openInputStream(uri)){if(in==null)throw new IOException("Cannot open selected file");FirmwareFiles.copy(in,source,FirmwareFiles.MAX_DOWNLOAD,()->cancelled);}
                    {FirmwareFiles.card(source,image,()->cancelled);String sha=FirmwareFiles.sha(image),id="imported-"+sha;String label="Imported · "+name;store.saveOriginal(image,id,label,sha,()->cancelled);store.activate(id,label,sha,()->cancelled);}
                }finally{source.delete();image.delete();}
            }
            return "Imported card activated; previous working card preserved. Header/size checked; compatibility is not guaranteed for non-Tech2Win images.";
        });
    }
    private void importSupport(List<Uri> uris){
        job(()->{
            if(uris.size()>3)throw new IOException("Select only eprom.bin, opsys.dwn and candi.bin");
            Map<String,File> staged=new LinkedHashMap<>();
            try{
                for(Uri uri:uris){
                    FirmwareFiles.check(()->cancelled);String name=null;
                    try(Cursor c=getContentResolver().query(uri,new String[]{OpenableColumns.DISPLAY_NAME},null,null,null)){if(c!=null&&c.moveToFirst())name=c.getString(0);}
                    if(name==null || !Arrays.asList(BundledSupport.NAMES).contains(name.toLowerCase(Locale.ROOT)))throw new IOException("Select extracted eprom.bin, opsys.dwn or candi.bin files");
                    name=name.toLowerCase(Locale.ROOT);
                    if(staged.containsKey(name))throw new IOException("Select only one copy of "+name);
                    File temp=File.createTempFile("support-import-",".tmp",getCacheDir());staged.put(name,temp);
                    phase("Checking communication firmware","Validating "+name+" before installation.",false);
                    try(InputStream in=getContentResolver().openInputStream(uri)){if(in==null)throw new IOException("Cannot open selected file");FirmwareFiles.copy(in,temp,262144,()->cancelled);}
                }
                SupportFirmware.install(getAssets(),store,staged,()->cancelled);
                return store.missingSupport().isEmpty()?"Communication firmware installed. Any remaining Saab software setup is shown below.":"Files installed. Still needed: "+store.missingSupport();
            }finally{for(File temp:staged.values())temp.delete();}
        });
    }
    private void installed(){
        job(()->{
            List<JSONObject> entries=new ArrayList<>();File[] dirs=store.library.listFiles();
            if(dirs!=null)for(File d:dirs){File info=new File(d,"image.json");if(info.isFile()&&entries.size()<100)entries.add(FirmwareStore.json(info));}
            File backups=new File(store.library,"backups");File[] saved=backups.listFiles();
            if(saved!=null){Arrays.sort(saved,(a,b)->b.getName().compareTo(a.getName()));for(File d:saved){if(entries.size()>=100)break;File info=new File(d,"active.json");if(info.isFile()&&new File(d,"card.bin").isFile()){JSONObject m=FirmwareStore.json(info);entries.add(new JSONObject().put("label","Backup "+d.getName()+" · "+m.optString("label")).put("backup",d.getName()).put("sha256",m.getString("backup_sha256")));}}}
            runOnUiThread(()->{if(closed)return;String[] labels=new String[entries.size()];for(int i=0;i<labels.length;i++)labels[i]=entries.get(i).optString("label");
                new AlertDialog.Builder(this).setTitle("Installed versions / backups").setItems(labels,(dialog,index)->job(()->{
                    JSONObject m=entries.get(index);String id=m.optString("id"),sha=m.getString("sha256"),label=m.getString("label");
                    if(m.has("backup")){File source=new File(new File(new File(store.library,"backups"),m.getString("backup")),"card.bin");id="restored-"+UUID.randomUUID();store.saveOriginal(source,id,label,sha,()->cancelled);}
                    store.activate(id,label,sha,()->cancelled);return "Activated "+label;
                })).setNegativeButton("Close",null).show();
            });return entries.isEmpty()?"No downloaded versions or backups yet.":"Choose a saved version or working-card backup.";
        });
    }
    protected void onDestroy(){closed=true;cancelled=true;if(connection!=null)connection.disconnect();worker.shutdownNow();super.onDestroy();}
}
