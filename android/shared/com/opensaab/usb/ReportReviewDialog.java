// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.app.*;
import android.os.Bundle;
import android.content.DialogInterface;
import android.view.*;
import android.widget.*;
import androidx.fragment.app.DialogFragment;
import androidx.lifecycle.ViewModelProvider;

/** Reusable lifecycle-managed review; configuration destruction is not user dismissal. */
public final class ReportReviewDialog extends DialogFragment {
    static final String TAG="support-report-review";
    private ReportReviewModel model;
    private TextView notice;
    private CheckBox consent;
    private Button send,options,keep;
    @Override public Dialog onCreateDialog(Bundle state){
        model=new ViewModelProvider(requireActivity()).get(ReportReviewModel.class);
        Activity host=requireActivity();LinearLayout panel=new LinearLayout(host);panel.setOrientation(LinearLayout.VERTICAL);
        int pad=SessionStyle.dp(host,12);panel.setPadding(pad,pad,pad,pad);
        TextView title=new TextView(host);title.setText("Review support report");title.setTextSize(20);panel.addView(title);
        TextView preview=new TextView(host);preview.setText(model.artifact==null?"Saved review unavailable":model.artifact.text);preview.setTextIsSelectable(true);
        ScrollView scroll=new ScrollView(host);scroll.addView(preview);panel.addView(scroll,new LinearLayout.LayoutParams(-1,0,1));
        notice=new TextView(host);notice.setTextIsSelectable(true);panel.addView(notice);
        consent=new CheckBox(host);consent.setText("I reviewed this report and agree to private upload");
        // Framework view hierarchy restoration must never restore affirmative consent.
        consent.setSaveEnabled(false);panel.addView(consent);
        LinearLayout actions=new LinearLayout(host);actions.setOrientation(LinearLayout.HORIZONTAL);panel.addView(actions);
        send=new Button(host);send.setText("Send to OpenSAAB");actions.addView(send,new LinearLayout.LayoutParams(0,-2,1));
        options=new Button(host);options.setText("Other options");actions.addView(options,new LinearLayout.LayoutParams(0,-2,1));
        keep=new Button(host);keep.setText("Keep private");actions.addView(keep,new LinearLayout.LayoutParams(0,-2,1));
        consent.setOnCheckedChangeListener((button,checked)->{if(model.consent!=checked)model.confirm(checked);});
        send.setOnClickListener(v->model.send());
        keep.setOnClickListener(v->{model.closeReview();dismiss();});
        options.setOnClickListener(v->{ReportArtifacts.Artifact a=model.artifact;model.closeReview();dismiss();if(a!=null)((SupportReportActivity)host).exportOptions(a.file,a.text);});
        model.changes.observe(this,ignored->refresh());refresh();
        return new AlertDialog.Builder(host).setView(panel).create();
    }
    private void refresh(){if(notice==null)return;notice.setText(model.notice);
        boolean sent=!model.receipt.isEmpty();consent.setVisibility(sent?View.GONE:View.VISIBLE);consent.setChecked(model.consent);consent.setEnabled(!model.busy);
        send.setEnabled(model.canSend());send.setVisibility(sent?View.GONE:View.VISIBLE);options.setEnabled(!model.busy);keep.setEnabled(!model.busy);setCancelable(!model.busy);
    }
    @Override public void onStart(){super.onStart();android.graphics.Rect area=new android.graphics.Rect();requireActivity().getWindow().getDecorView().getWindowVisibleDisplayFrame(area);
        Window window=requireDialog().getWindow();if(window!=null)window.setLayout((int)(area.width()*0.92),(int)(area.height()*0.94));}
    @Override public void onCancel(DialogInterface d){model.closeReview();super.onCancel(d);}
    @Override public void onDismiss(DialogInterface d){
        // AndroidX destroys the dialog window during rotation without dismissing the review.
        if(model!=null&&isRemoving()&&getActivity()!=null&&!requireActivity().isChangingConfigurations())model.closeReview();
        super.onDismiss(d);
    }
}
