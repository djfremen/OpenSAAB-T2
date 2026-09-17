// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;import android.view.*;import android.widget.*;import android.graphics.*;import java.io.*;
/** Synthetic presentation fixture; never starts USB or changes the working card. */
public final class SessionLayoutInstrumentedTest extends Instrumentation {
 public void onCreate(Bundle b){super.onCreate(b);start();}
 void headunit(android.content.Context original){
  android.content.res.Configuration config=new android.content.res.Configuration(original.getResources().getConfiguration());
  config.screenWidthDp=1024;config.screenHeightDp=600;config.densityDpi=160;
  android.content.Context context=new android.content.ContextWrapper(original.createConfigurationContext(config)){
   public String getPackageName(){return "com.opensaab.tech2.headunit32";}
  };
  LinearLayout root=new LinearLayout(context);root.setOrientation(LinearLayout.VERTICAL);
  root.addView(new BrandHeader(context,"OpenSAAB T2 · Chipsoft"));
  LinearLayout actions=new LinearLayout(context);for(String name:new String[]{"Start firmware","Stop USB","Back"}){Button b=new Button(context);b.setText(name);actions.addView(b);}
  SessionStyle.row(actions);root.addView(actions,new LinearLayout.LayoutParams(-1,56));
  ScrollView console=new ScrollView(context);console.addView(new TextView(context));
  Tech2Controls controls=new Tech2Controls(context,new ImageView(context),console,key->{});root.addView(controls,new LinearLayout.LayoutParams(-1,0,1));
  SessionStyle.stack(root);HeadunitLayout.apply(root);SessionStyle.fitPortrait(root);
  root.measure(View.MeasureSpec.makeMeasureSpec(1024,View.MeasureSpec.EXACTLY),View.MeasureSpec.makeMeasureSpec(600,View.MeasureSpec.EXACTLY));root.layout(0,0,1024,600);
  View lcd=root.findViewWithTag("tech2-gestures");if(root.findViewWithTag("headunit-actions")==null||lcd.getWidth()<480||lcd.getHeight()<280)throw new AssertionError("Head-unit firmware area too small");
 }
 public void onStart(){Bundle out=new Bundle();ChipsoftUsbActivity a=null;try{
  a=(ChipsoftUsbActivity)startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.usb.ChipsoftUsbActivity").putExtra("full_native",true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
  final ChipsoftUsbActivity v=a;runOnMainSync(()->{headunit(v);
   v.lcdHandler.removeCallbacksAndMessages(null);v.status.setText("USB disconnected");v.vinSummary.setText("Last vehicle · 2004 Saab 9-3 Sport Sedan\nB207R · Turbo · Silver Metallic");
   v.securityAccess.setVisibility(View.VISIBLE);TextView state=v.securityAccess.findViewWithTag("security-state");state.setText("Security status  [POST-AUTH]");state.setTextColor(SsaState.POST_AUTH.color);
   ((TextView)v.securityAccess.findViewWithTag("security-message")).setText("Post-auth written · Sep 16, 17:06:23 PDT");
   ((Button)v.securityAccess.findViewWithTag("security-action")).setText("Details");v.reportConnection.setVisibility(View.VISIBLE);
  });waitForIdleSync();SystemClock.sleep(300);
  runOnMainSync(()->{
   View exit=v.getWindow().getDecorView().findViewWithTag("tech2-key-1");Rect r=new Rect();if(!exit.getGlobalVisibleRect(r)||r.height()!=exit.getHeight())throw new AssertionError("EXIT clipped");
   Button b=v.securityAccess.findViewWithTag("security-action");LinearLayout row=(LinearLayout)b.getParent();View next=row.getChildAt(1);
   if(b.getHeight()!=next.getHeight()||Math.abs(b.getWidth()-next.getWidth())>1||next.getLeft()-b.getRight()<SessionStyle.dp(v,8))throw new AssertionError("Uneven/colliding security controls");
   if(v.running.get()||SecurityAccessView.workflowBusy())throw new AssertionError("Unexpected vehicle work");
  });
  File file=new File(getTargetContext().getExternalFilesDir(null),"session-layout.png");try(FileOutputStream f=new FileOutputStream(file)){getUiAutomation().takeScreenshot().compress(Bitmap.CompressFormat.PNG,100,f);}
  out.putString("stream","PASS: equal security button widths/heights, 8dp gap, visible EXIT; synthetic screenshot; no card/USB/API operations");finish(-1,out);
 }catch(Throwable e){out.putString("stream","FAIL: "+e);finish(0,out);}finally{if(a!=null){final Activity v=a;runOnMainSync(v::finish);}}}
}
