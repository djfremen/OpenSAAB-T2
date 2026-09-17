// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;import android.view.*;import android.widget.*;import android.graphics.*;import java.io.*;
/** Runs idle UI only. No adapter, card writes, API calls or firmware commands. */
public final class SessionLayoutInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle b){super.onCreate(b);start();}
 void ui(Runnable task){final Throwable[] error={null};runOnMainSync(()->{try{task.run();}catch(Throwable e){error[0]=e;}});if(error[0]!=null)throw new AssertionError(error[0]);}
 static void check(boolean b,String message){if(!b)throw new AssertionError(message);}
 static TextView text(View v,String label){if(v instanceof TextView&&label.contentEquals(((TextView)v).getText()))return (TextView)v;if(v instanceof ViewGroup)for(int i=0;i<((ViewGroup)v).getChildCount();i++){TextView f=text(((ViewGroup)v).getChildAt(i),label);if(f!=null)return f;}return null;}
 void visible(View root,String tag){View v=root.findViewWithTag(tag);Rect r=new Rect();check(v!=null&&v.getGlobalVisibleRect(r)&&r.height()==v.getHeight()&&r.width()==v.getWidth(),"Clipped: "+tag);}
 void layouts(Activity a){
  for(int[] size:new int[][]{{360,660},{360,724},{393,780},{1024,600},{800,480}}){
   ScrollView log=new ScrollView(a);log.addView(new TextView(a));
   Tech2Controls display=new Tech2Controls(a,new ImageView(a),log,key->{throw new AssertionError("Unexpected firmware input");});display.setActions(()->{});
   SessionWorkspace w=new SessionWorkspace(a,"OpenSAAB T2 · Chipsoft",display,()->{},()->{});w.summary("2004 9-3 · Connected\nSecurity: [POST-AUTH] · Fresh");
   int width=SessionStyle.dp(a,size[0]),height=SessionStyle.dp(a,size[1]);
   w.measure(View.MeasureSpec.makeMeasureSpec(width,View.MeasureSpec.EXACTLY),View.MeasureSpec.makeMeasureSpec(height,View.MeasureSpec.EXACTLY));w.layout(0,0,width,height);
   View lcd=w.findViewWithTag("tech2-gestures");check(lcd.getHeight()>=SessionStyle.dp(a,size[0]>size[1]?260:280),"Firmware too short "+size[0]+"x"+size[1]);
   check(lcd.getWidth()>=SessionStyle.dp(a,size[0]>size[1]?500:330),"Firmware too narrow");
   check(w.getOrientation()==(size[0]>=720&&size[0]>size[1]?LinearLayout.HORIZONTAL:LinearLayout.VERTICAL),"Wrong responsive layout");
   int old=lcd.getHeight();w.toggleExpanded();w.measure(View.MeasureSpec.makeMeasureSpec(width,View.MeasureSpec.EXACTLY),View.MeasureSpec.makeMeasureSpec(height,View.MeasureSpec.EXACTLY));w.layout(0,0,width,height);
   check(lcd.getHeight()>=old,"Expanded screen shrank");check(w.findViewWithTag("tech2-actions").getVisibility()==View.VISIBLE,"Expanded screen has no way back");
  }
 }
 public void onStart(){Bundle out=new Bundle();Activity current=null;try{
  for(String name:new String[]{"com.opensaab.tech2.MainActivity","com.opensaab.usb.ChipsoftUsbActivity"}){
   Intent intent=new Intent().setClassName(getTargetContext(),name).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);if(name.contains("Chipsoft"))intent.putExtra("full_native",true);
   current=startActivitySync(intent);final Activity a=current;waitForIdleSync();
   ui(()->{
    if(a instanceof ChipsoftUsbActivity){ChipsoftUsbActivity c=(ChipsoftUsbActivity)a;c.lcdHandler.removeCallbacksAndMessages(null);check(!c.running.get(),"Unexpected USB session");}
    layouts(a);View root=a.getWindow().getDecorView();visible(root,"tech2-navigation-hint");visible(root,"tech2-key-1");visible(root,"tech2-actions");visible(root,"session-menu");visible(root,"session-summary");
    check(text(root,"Read DTC")==null&&text(root,"Check for updates")==null,"Secondary actions consume screen");
    check(root.findViewWithTag("tech2-gestures").getHeight()>=SessionStyle.dp(a,280),"Live firmware space too small");
   });
   File file=new File(getTargetContext().getExternalFilesDir(null),name.contains("Chipsoft")?"workspace-chipsoft.png":"workspace-main.png");try(FileOutputStream f=new FileOutputStream(file)){getUiAutomation().takeScreenshot().compress(Bitmap.CompressFormat.PNG,100,f);}
   final Dialog[] sheet={null};
   ui(()->{sheet[0]=a instanceof ChipsoftUsbActivity?((ChipsoftUsbActivity)a).showActions():((com.opensaab.tech2.MainActivity)a).showActions();});waitForIdleSync();
   ui(()->{View root=sheet[0].getWindow().getDecorView();for(String title:new String[]{"Get security access","Read DTC","Clear DTC","Engine Data","Saved DTC reports"})check(text(root,title)!=null,"Missing action "+title);sheet[0].dismiss();});
   ui(a::finish);waitForIdleSync();current=null;
  }
  out.putString("stream","PASS: Main and Chipsoft firmware-first workspaces; 360dp Mate/phone and 800/1024 landscape sizing; visible EXIT/Actions/menu/status; expansion retains controls; no USB/card/API operations");finish(-1,out);
 }catch(Throwable e){out.putString("stream","FAIL: "+e);finish(0,out);}finally{if(current!=null){final Activity a=current;runOnMainSync(a::finish);}}}
}
