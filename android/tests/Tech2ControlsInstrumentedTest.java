// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.content.Intent;
import android.graphics.Rect;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.*;
import android.widget.*;
import java.util.*;

/** Checks real Android layout and callback behavior without opening an adapter. */
public final class Tech2ControlsInstrumentedTest extends Instrumentation {
    public void onCreate(Bundle args) { super.onCreate(args); start(); }
    private static void check(boolean ok, String message) { if (!ok) throw new AssertionError(message); }
    private void checkedMain(Runnable task) {
        final Throwable[] failure={null};
        runOnMainSync(()->{try{task.run();}catch(Throwable e){failure[0]=e;}});
        if(failure[0]!=null)throw new AssertionError("UI check failed",failure[0]);
    }
    private final ArrayList<Integer> sent = new ArrayList<>();
    private void exercise(int widthDp, int heightDp) {
        android.content.Context context = getTargetContext();
        float density = context.getResources().getDisplayMetrics().density;
        ScrollView console = new ScrollView(context); console.addView(new TextView(context));
        Tech2Controls panel = new Tech2Controls(context, new ImageView(context), console, sent::add);
        int width = Math.round(widthDp * density), height = Math.round(heightDp * density);
        panel.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
        panel.layout(0, 0, width, height);
        View display=panel.findViewWithTag("tech2-gestures");
        int largeDisplayHeight=display.getHeight();
        ScrollView body=panel.findViewWithTag("tech2-keypad");
        Button keys=panel.findViewWithTag("tech2-keypad-toggle");
        check(body.getVisibility()==View.GONE,"Keypad should start hidden");
        keys.performClick();
        panel.measure(View.MeasureSpec.makeMeasureSpec(width,View.MeasureSpec.EXACTLY),View.MeasureSpec.makeMeasureSpec(height,View.MeasureSpec.EXACTLY));panel.layout(0,0,width,height);
        check(display.getHeight()<largeDisplayHeight,"Keypad toggle did not resize display");
        String[] labels = {"EXIT","S1","S2","S3","S4","HELP","↑","YES","←","ENTER","→","↓","NO","F0","F1","F2","F3","F4","F5","F6","F7","F8","F9"};
        int[] expected = {1,10,6,7,8,25,9,14,11,16,13,12,20,24,4,19,23,3,18,22,2,17,21};
        sent.clear();
        for (int i=0;i<labels.length;i++) {
            Button key = panel.findViewWithTag("tech2-key-" + expected[i]);
            check(key != null && labels[i].contentEquals(key.getText()), "Missing/wrong key: " + labels[i]);
            check(key.getHeight() >= Math.round(48 * density), "Small touch target: " + labels[i]);
            key.performClick();
            check(sent.size() == i+1 && sent.get(i) == expected[i], "Duplicate/wrong callback: " + labels[i]);
        }
        Button exit=panel.findViewWithTag("tech2-key-1"), toggle=panel.findViewWithTag("tech2-console-toggle");
        check(console.getVisibility()==View.GONE,"Console should start collapsed");
        for(int i=0;i<2;i++) {
            toggle.performClick();
            panel.measure(View.MeasureSpec.makeMeasureSpec(width,View.MeasureSpec.EXACTLY),View.MeasureSpec.makeMeasureSpec(height,View.MeasureSpec.EXACTLY));panel.layout(0,0,width,height);
            body.fullScroll(View.FOCUS_DOWN);
            Rect bounds=new Rect(0,0,exit.getWidth(),exit.getHeight());panel.offsetDescendantRectToMyCoords(exit,bounds);
            check(bounds.top>=0 && bounds.bottom<=height,"EXIT clipped by scrolling/console");
            check(body.getHeight()>0,"Console consumed entire body");
        }
        check(console.getVisibility()==View.GONE,"Console failed to collapse");
        keys.performClick();
        panel.measure(View.MeasureSpec.makeMeasureSpec(width,View.MeasureSpec.EXACTLY),View.MeasureSpec.makeMeasureSpec(height,View.MeasureSpec.EXACTLY));panel.layout(0,0,width,height);
        check(body.getVisibility()==View.GONE && display.getHeight()==largeDisplayHeight,"Large display failed to restore");
        check(sent.size()==expected.length,"Console/scroll/toggle emitted a firmware key");
    }
    private void touch(View v, int action, float x, float y) {
        long t=SystemClock.uptimeMillis();
        MotionEvent e=MotionEvent.obtain(t,t,action,x,y,0);
        v.dispatchTouchEvent(e);e.recycle();
    }
    private void gestures(Activity activity) {
        final FirmwareGestureView[] area={null};
        checkedMain(()->{
            Tech2Controls panel=new Tech2Controls(activity,new ImageView(activity),new ScrollView(activity),sent::add);
            activity.setContentView(panel);area[0]=panel.findViewWithTag("tech2-gestures");sent.clear();
        });
        waitForIdleSync();
        float d=activity.getResources().getDisplayMetrics().density;
        checkedMain(()->{
            View v=area[0];
            touch(v,MotionEvent.ACTION_DOWN,100*d,140*d);
            touch(v,MotionEvent.ACTION_MOVE,100*d,60*d);
            touch(v,MotionEvent.ACTION_UP,100*d,40*d);
            touch(v,MotionEvent.ACTION_DOWN,100*d,40*d);
            touch(v,MotionEvent.ACTION_MOVE,100*d,100*d);
            touch(v,MotionEvent.ACTION_UP,100*d,140*d);
            check(sent.equals(Arrays.asList(9,12)),"Swipes must emit one directional key each");
            sent.clear();
            // Tap, horizontal swipe, diagonal, interrupted stroke and multiple pointers.
            touch(v,0,100*d,100*d);touch(v,1,100*d,100*d);
            touch(v,0,100*d,100*d);touch(v,2,200*d,100*d);touch(v,1,220*d,100*d);
            touch(v,0,100*d,100*d);touch(v,2,180*d,180*d);touch(v,1,200*d,200*d);
            touch(v,0,100*d,140*d);touch(v,3,100*d,140*d);touch(v,1,100*d,40*d);
            touch(v,0,100*d,140*d);touch(v,5,100*d,140*d);touch(v,1,100*d,40*d);
            check(sent.isEmpty(),"Ambiguous/cancelled gesture emitted a key");
            touch(v,0,100*d,100*d);
        });
        SystemClock.sleep(ViewConfiguration.getLongPressTimeout()+180);
        checkedMain(()->{
            check(sent.equals(Arrays.asList(16)),"Hold did not emit exactly one ENTER");
            // Drag/release after ENTER must not produce another action.
            touch(area[0],2,100*d,40*d);touch(area[0],1,100*d,40*d);
            check(sent.equals(Arrays.asList(16)),"Hold/release duplicated command");sent.clear();
            touch(area[0],0,100*d,100*d);area[0].onWindowFocusChanged(false);
        });
        SystemClock.sleep(ViewConfiguration.getLongPressTimeout()+180);
        checkedMain(()->{
            touch(area[0],1,100*d,100*d);check(sent.isEmpty(),"Focus loss left pending ENTER");
            touch(area[0],0,100*d,100*d);
            // Replacing the screen detaches the gesture view and cancels its timer.
            activity.setContentView(new TextView(activity));
        });
        SystemClock.sleep(ViewConfiguration.getLongPressTimeout()+180);
        checkedMain(()->check(sent.isEmpty(),"Detached display emitted ENTER"));
    }
    public void onStart() {
        Bundle result=new Bundle();int status=-1;Activity activity=null;
        try {
            checkedMain(()->{exercise(360,440);exercise(800,220);});
            for (String name : new String[]{"com.opensaab.tech2.MainActivity","com.opensaab.usb.ChipsoftUsbActivity","com.opensaab.usb.NanoProbeActivity"}) {
                Intent intent=new Intent().setClassName(getTargetContext(),name).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                // Deliberately omit auto_start. These activities stay idle and never claim USB.
                if(name.contains("Chipsoft"))intent.putExtra("full_native",true);
                if(name.contains("Nano"))intent.putExtra("native_dtc",true);
                activity=startActivitySync(intent);waitForIdleSync();
                final Activity shown=activity;
                checkedMain(()->{
                    View exit=shown.getWindow().getDecorView().findViewWithTag("tech2-key-1");Rect visible=new Rect();
                    check(exit!=null && exit.getGlobalVisibleRect(visible) && visible.height()==exit.getHeight(),"EXIT not fully visible in "+name);
                    if(shown instanceof ChipsoftUsbActivity)check(!((ChipsoftUsbActivity)shown).running.get(),"Unexpected Chipsoft session");
                    if(shown instanceof NanoProbeActivity)check(!((NanoProbeActivity)shown).running.get(),"Unexpected Nano session");
                });
                if(name.endsWith("MainActivity"))gestures(activity);
                checkedMain(shown::finish);activity=null;waitForIdleSync();
            }
            result.putString("stream","PASS: 23 verified key callbacks; compact/full keypad resizing; 48dp targets; short/wide layouts; persistent EXIT; swipe direction and one-event limit; long-press ENTER; taps/horizontal/diagonal/multitouch/cancel/focus-loss/detach emit no stray keys; all three activity layouts; no USB sessions started\n");
        }catch(Throwable e){status=0;result.putString("stream","FAIL: "+e+"\n");}
        finally{if(activity!=null){final Activity a=activity;runOnMainSync(a::finish);}finish(status,result);}
    }
}
