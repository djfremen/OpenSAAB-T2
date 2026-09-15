// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.*;
import android.net.Uri;
import android.os.*;
import android.widget.*;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.UUID;
import java.util.concurrent.*;
import java.util.function.Supplier;

/** Passive observer: never sends keys, CAN commands, mail or network requests. */
public final class DtcReportView extends Button implements AutoCloseable {
    private final Activity activity;
    private final Supplier<File> directory;
    private final String adapter;
    private final Handler ui=new Handler(Looper.getMainLooper());
    private final ScheduledExecutorService worker=Executors.newSingleThreadScheduledExecutor();
    private volatile boolean closed;
    private File session;
    private String previous="", stem;
    private DtcReport current;
    private boolean dirty, reportedError;
    private Snapshot visible;
    private static final class Snapshot {
        final String text,summary;final File file;final int count,total;
        Snapshot(DtcReport r,File f){text=r.text();summary=r.summary();file=f;count=r.pages.size();total=r.total;}
    }
    public DtcReportView(Activity a,String adapter,Supplier<File> directory){
        super(a);activity=a;this.adapter=adapter;this.directory=directory;
        setTag("tech2-dtc-report");setAllCaps(false);setTextSize(13);setVisibility(GONE);
        setMinHeight(Math.round(48*getResources().getDisplayMetrics().density));
        setOnClickListener(v->review());
        worker.scheduleWithFixedDelay(this::observe,0,250,TimeUnit.MILLISECONDS);
    }
    private void observe(){
        if(closed)return;
        try{
            File dir=directory.get();if(dir==null)return;
            if(!dir.equals(session)){session=dir;current=null;dirty=false;reportedError=false;previous="";ui.post(()->{if(!closed){visible=null;setVisibility(GONE);}});}
            File screen=new File(dir,"native-dtc-screen.txt");
            if(!screen.isFile() || screen.length()>DtcReport.MAX_SCREEN)return;
            String text=new String(Files.readAllBytes(screen.toPath()),StandardCharsets.UTF_8);
            // Require two matching reads; guest text is currently replaced with truncate/write.
            if(!text.equals(previous)){previous=text;return;}
            if(text.trim().isEmpty())return;
            DtcReport.Page page=DtcReport.parse(text);
            if(page==null){
                if(current==null)return;
                String before=current.text();
                if(!current.addDetail(text)){current=null;dirty=false;return;}
                dirty |= !before.equals(current.text());
            }else{
                boolean changed;
                if(current==null || current.total!=page.total){
                    current=new DtcReport(adapter,dir.getName(),page,VehicleSession.read(new File(dir,VehicleSession.FILE)));
                    stem=reportName(adapter);changed=true;
                }else changed=current.add(page);
                dirty |= changed;
            }
            if(!dirty)return;
            File reports=new File(activity.getFilesDir(),"dtc-reports");Files.createDirectories(reports.toPath());
            File txt=new File(reports,stem+".txt");
            write(new File(reports,stem+".json"),current.json());write(txt,current.text());
            dirty=false;reportedError=false;
            Snapshot snapshot=new Snapshot(current,txt);
            android.util.Log.i("OpenSaabDtc","DTC_REPORT saved="+txt.getName()+" positions="+snapshot.count+"/"+snapshot.total);
            final int codes=current.codeCount();
            ui.post(()->{if(!closed){visible=snapshot;setText("DTC report · "+codes+" codes"+(snapshot.count<snapshot.total?" · Partial":"")+" · Review / share");setContentDescription("DTC report: "+codes+" codes; "+snapshot.count+"/"+snapshot.total+" list positions captured. Review or share.");setVisibility(VISIBLE);}});
        }catch(Exception e){
            if(reportedError)return;reportedError=true;
            android.util.Log.w("OpenSaabDtc","DTC report could not be saved",e);
            ui.post(()->{if(!closed){setText("DTC report save failed · tap for last saved report");setVisibility(VISIBLE);}});
        }
    }
    private static void write(File f,String value)throws IOException{
        File temp=new File(f.getParentFile(),f.getName()+".tmp");
        try(FileOutputStream out=new FileOutputStream(temp)){out.write(value.getBytes(StandardCharsets.UTF_8));out.getFD().sync();}
        Files.move(temp.toPath(),f.toPath(),StandardCopyOption.REPLACE_EXISTING,StandardCopyOption.ATOMIC_MOVE);
    }
    private void review(){
        Snapshot s=visible;
        if(s==null){Toast.makeText(activity,"No saved DTC report yet",Toast.LENGTH_LONG).show();return;}
        TextView text=new TextView(activity);text.setText(s.text+(adapter.equals("nano")?"\nOpening another app to share stops the Nano USB session. Finish collecting the list first.":""));
        text.setTextIsSelectable(true);text.setPadding(24,12,24,12);ScrollView scroll=new ScrollView(activity);scroll.addView(text);
        new AlertDialog.Builder(activity).setTitle("DTC report").setView(scroll)
            .setPositiveButton("Share / email",(d,w)->share(s)).setNegativeButton("Back to codes",null).show();
    }
    private void share(Snapshot s){
        worker.execute(()->{try{
            File frozen=new File(s.file.getParentFile(),reportName(adapter)+".txt");
            write(frozen,s.text);
            ui.post(()->{if(!closed)shareFile(activity,frozen,s.summary,adapter);});
        }catch(IOException e){ui.post(()->{if(!closed)Toast.makeText(activity,"Could not prepare report for sharing",Toast.LENGTH_LONG).show();});}});
    }
    private static String reportName(String adapter){return "android_"+adapter+"_dtc_"+java.time.Instant.now().toString().replaceAll("[^0-9TZ]","")+"_"+UUID.randomUUID();}
    static Intent sharingIntent(Activity activity,File file,String summary,String adapter){
        Uri uri=new Uri.Builder().scheme("content").authority(activity.getPackageName()+".dtc-reports").appendPath(file.getName()).build();
        Intent intent=new Intent(Intent.ACTION_SEND).setType("text/plain")
            .putExtra(Intent.EXTRA_SUBJECT,"OpenSAAB DTC report · Android · "+adapter)
            .putExtra(Intent.EXTRA_TEXT,summary).putExtra(Intent.EXTRA_STREAM,uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        intent.setClipData(ClipData.newRawUri("DTC report",uri));
        return intent;
    }
    private static void shareFile(Activity activity,File file,String summary,String adapter){
        try{activity.startActivity(Intent.createChooser(sharingIntent(activity,file,summary,adapter),"Share DTC report / email"));}
        catch(ActivityNotFoundException e){Toast.makeText(activity,"No sharing app installed. Report remains saved on this phone.",Toast.LENGTH_LONG).show();}
    }
    /** Reopen durable reports after leaving diagnostics or restarting the app. */
    public static void showSavedReports(Activity activity){
        new Thread(()->{
            File directory=new File(activity.getFilesDir(),"dtc-reports");
            File[] files=directory.listFiles((d,n)->n.endsWith(".txt"));
            if(files==null)files=new File[0];
            java.util.Arrays.sort(files,(a,b)->Long.compare(b.lastModified(),a.lastModified()));
            final File[] latest=java.util.Arrays.copyOf(files,Math.min(100,files.length));
            final String[] names=new String[latest.length];for(int i=0;i<latest.length;i++)names[i]=latest[i].getName();
            activity.runOnUiThread(()->{
                if(activity.isFinishing() || activity.isDestroyed())return;
                if(latest.length==0){new AlertDialog.Builder(activity).setTitle("DTC reports").setMessage("Reports are saved automatically when the original firmware displays a numbered DTC Information list. Scroll through the list to capture every position.").setPositiveButton("OK",null).show();return;}
                new AlertDialog.Builder(activity).setTitle("Saved DTC reports · latest 100").setItems(names,(d,which)->{
                    new Thread(()->{try{
                        File f=latest[which];if(f.length()>2000000)throw new IOException("Report too large");
                        String contents=new String(Files.readAllBytes(f.toPath()),StandardCharsets.UTF_8);
                        activity.runOnUiThread(()->{
                            if(activity.isFinishing() || activity.isDestroyed())return;
                            TextView view=new TextView(activity);view.setText(contents);view.setTextIsSelectable(true);view.setPadding(24,12,24,12);
                            ScrollView scroll=new ScrollView(activity);scroll.addView(view);
                            new AlertDialog.Builder(activity).setTitle("Saved DTC report").setView(scroll)
                                .setPositiveButton("Share / email",(dialog,w)->new Thread(()->{try{
                                    File frozen=new File(f.getParentFile(),reportName(f.getName().contains("chipsoft")?"chipsoft":"nano")+".txt");write(frozen,contents);
                                    activity.runOnUiThread(()->{if(!activity.isFinishing()&&!activity.isDestroyed())shareFile(activity,frozen,"Original firmware DTC report. See attachment for capture coverage.",f.getName().contains("chipsoft")?"chipsoft":"nano");});
                                }catch(IOException e){activity.runOnUiThread(()->Toast.makeText(activity,"Could not prepare report",Toast.LENGTH_LONG).show());}},"dtc-report-share").start())
                                .setNegativeButton("Close",null).show();
                        });
                    }catch(IOException e){activity.runOnUiThread(()->Toast.makeText(activity,"Report unavailable",Toast.LENGTH_LONG).show());}},"dtc-report-open").start();
                }).setNegativeButton("Close",null).show();
            });
        },"dtc-report-list").start();
    }
    public void close(){closed=true;worker.shutdown();ui.removeCallbacksAndMessages(null);}
}
