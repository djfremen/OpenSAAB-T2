// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.Context;
import android.view.*;
import android.widget.FrameLayout;
import java.util.function.IntConsumer;

/** Converts one deliberate LCD gesture into one original keypad event. */
public final class FirmwareGestureView extends FrameLayout {
    private final IntConsumer send;
    private final int slop;
    private final float swipeDistance;
    private float startX, startY;
    private boolean tracking, moved, held;
    private final Runnable hold = () -> {
        if (tracking && !moved && !held) {
            held = true;
            performLongClick();
        }
    };

    public FirmwareGestureView(Context context, IntConsumer send) {
        super(context);
        this.send = send;
        slop = ViewConfiguration.get(context).getScaledTouchSlop();
        swipeDistance = 32 * getResources().getDisplayMetrics().density;
        setClickable(true);
        setFocusable(true);
        setContentDescription("Original Tech2 display. Swipe up or down to move one menu item. Hold to press ENTER. Use EXIT to return.");
        setOnLongClickListener(v -> { send.accept(0x10); return true; });
    }
    @Override public boolean onInterceptTouchEvent(MotionEvent event) { return true; }
    @Override public boolean onTouchEvent(MotionEvent event) {
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
                cancelGesture();
                if (event.getPointerCount() != 1) return true;
                tracking = true;
                startX = event.getX(); startY = event.getY();
                if (getParent() != null) getParent().requestDisallowInterceptTouchEvent(true);
                postDelayed(hold, ViewConfiguration.getLongPressTimeout());
                return true;
            case MotionEvent.ACTION_POINTER_DOWN:
            case MotionEvent.ACTION_CANCEL:
                cancelGesture(); return true;
            case MotionEvent.ACTION_MOVE:
                if (!tracking) return true;
                if (event.getPointerCount() != 1) { cancelGesture(); return true; }
                if (Math.abs(event.getX()-startX) > slop || Math.abs(event.getY()-startY) > slop) {
                    moved = true; removeCallbacks(hold);
                }
                return true;
            case MotionEvent.ACTION_UP:
                if (tracking && !held) {
                    float dx = event.getX()-startX, dy = event.getY()-startY;
                    // One event per stroke, no velocity-based repeat or horizontal shortcuts.
                    if (Math.abs(dy) >= swipeDistance && Math.abs(dy) > Math.abs(dx)*1.5f)
                        send.accept(dy < 0 ? 0x09 : 0x0c);
                    else if (!moved) performClick(); // A tap never confirms a firmware action.
                }
                cancelGesture(); return true;
            default: return true;
        }
    }
    @Override public boolean performClick() { super.performClick(); return true; }
    public void cancelGesture() {
        removeCallbacks(hold); tracking = false; moved = false; held = false;
        if (getParent() != null) getParent().requestDisallowInterceptTouchEvent(false);
    }
    @Override public void onWindowFocusChanged(boolean focus) {
        super.onWindowFocusChanged(focus); if (!focus) cancelGesture();
    }
    @Override protected void onDetachedFromWindow() { cancelGesture(); super.onDetachedFromWindow(); }
}
