// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;import android.view.*;import android.widget.*;

/** Exact installed normal-session UI and legacy-preference migration; opens no adapter. */
public final class SessionModePickerInstrumentedTest extends Instrumentation {
 private Bundle arguments;
 public void onCreate(Bundle args){super.onCreate(args);arguments=args;start();}
 private static void check(boolean ok,String reason){if(!ok)throw new AssertionError(reason);}
 private void ui(Runnable action){final Throwable[] error={null};runOnMainSync(()->{try{action.run();}catch(Throwable e){error[0]=e;}});if(error[0]!=null)throw new AssertionError(error[0]);}
 public void onStart(){Bundle result=new Bundle();Activity host=null;try{
  DisposableEmulator.require(getTargetContext());DisposableEmulator.requireInstalledVersion(getTargetContext(),arguments);
  // Reproduce upgrading a device whose saved choice was Read-only.
  getTargetContext().getSharedPreferences("session_mode",Context.MODE_PRIVATE).edit().putString("selected","READ").commit();
  host=startActivitySync(new Intent().setClassName(getTargetContext(),"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));final Activity activity=host;
  waitForIdleSync();ui(()->{View continueButton=activity.findViewById(android.R.id.content).findViewWithTag("preview-continue");if(continueButton!=null)continueButton.performClick();});
  ui(()->{
   View root=activity.findViewById(android.R.id.content);
   SessionModeControl home=root.findViewWithTag("session-mode-control");check(home!=null,"Session Mode missing from main screen");
   check(root.findViewWithTag("session-mode-read")==null&&root.findViewWithTag("session-mode-full")==null,"Normal connection still offers authority checkboxes");
   check(home.selected()==SessionModes.Mode.FULL,"Legacy Read-only preference narrowed the normal session");
   TextView preview=root.findViewWithTag("session-mode-preview");
   check(preview.getText().toString().contains("FULL SESSION"),"Normal session authority is not visible");
   for(String saved:new String[]{null,"READ","FULL","unknown"}){
    android.content.SharedPreferences prefs=activity.getSharedPreferences("session_mode",Context.MODE_PRIVATE);
    if(saved==null)prefs.edit().clear().commit();else prefs.edit().putString("selected",saved).commit();
    home.reload();
    check(home.selected()==SessionModes.Mode.FULL,"Saved preference changed authority: "+saved);
    check(new SessionModeControl(activity).selected()==SessionModes.Mode.FULL,"Recreated control changed authority: "+saved);
   }
   home.available(false);check(home.selected()==SessionModes.Mode.FULL,"Busy state changed authority");home.available(true);
   for(String family:new String[]{"mxplus","mdi","chipsoft","nano"}){
    SessionModes.Mode mode=SessionModes.normalConnectionMode(SessionModes.forAdapter(family,false));
    check(mode==SessionModes.Mode.FULL,"Normal connection is not Full: "+family);
    if(family.equals("chipsoft"))check(SessionModes.usbFlag(family,mode,false).equals("full_native"),"Chipsoft selects restricted worker");
    if(family.equals("nano"))check(SessionModes.usbFlag(family,mode,false).equals("nano_full_native"),"Nano selects restricted worker");
    if(family.equals("mxplus"))check(SessionModes.emulationProfile(mode).equals("mxp_full_native"),"MX+ selects restricted context");
   }
   check(SessionModes.normalConnectionMode(SessionModes.forAdapter("unverified",false))==null,"Missing Full binding was substituted");
  });
  ui(()->{
   for(int[] size:new int[][]{{360,660},{393,780},{800,480},{1024,600}}){
    ScrollView log=new ScrollView(activity);log.addView(new TextView(activity));
    Tech2Controls controls=new Tech2Controls(activity,new ImageView(activity),log,key->{throw new AssertionError("Mode selection sent a firmware key");});
    SessionWorkspace workspace=new SessionWorkspace(activity,"Preview",controls,()->{},()->{});
    SessionModeControl mode=new SessionModeControl(activity);workspace.addLaunchContent(mode);
    Button connect=new Button(activity);connect.setText("Connect and start");workspace.addLaunch(connect);
    workspace.addLaunch(new Button(activity));workspace.addLaunch(new Button(activity));
    int width=SessionStyle.dp(activity,size[0]),height=SessionStyle.dp(activity,size[1]);
    workspace.measure(View.MeasureSpec.makeMeasureSpec(width,View.MeasureSpec.EXACTLY),View.MeasureSpec.makeMeasureSpec(height,View.MeasureSpec.EXACTLY));workspace.layout(0,0,width,height);
    android.graphics.Rect control=new android.graphics.Rect(),launch=new android.graphics.Rect();mode.getDrawingRect(control);connect.getDrawingRect(launch);workspace.offsetDescendantRectToMyCoords(mode,control);workspace.offsetDescendantRectToMyCoords(connect,launch);
    View viewport=workspace.findViewWithTag("session-header-scroll");
    check(control.top>=0&&launch.bottom<=viewport.getBottom(),"Primary mode or Connect requires scrolling: "+size[0]+"x"+size[1]);
    check(controls.getHeight()>=SessionStyle.dp(activity,size[0]>size[1]?260:280),"Primary chooser squeezes firmware controls: "+size[0]+"x"+size[1]);
   }
  });
  result.putString("stream","PASS: normal Full session without authority checkboxes; saved READ/FULL/unknown/absent preferences cannot narrow authority; all four adapters map to Full bindings; unsupported Full binding remains unavailable; 360/393 portrait and 800/1024 landscape launch controls visible; zero adapter or firmware operations");finish(-1,result);
 }catch(Throwable error){result.putString("stream","FAIL: "+error);finish(0,result);}finally{if(host!=null){final Activity activity=host;runOnMainSync(activity::finish);}}}
}
