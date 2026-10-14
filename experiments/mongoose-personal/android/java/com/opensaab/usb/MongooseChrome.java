// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;
import android.content.*;
import android.content.res.ColorStateList;
import android.graphics.drawable.*;
import android.net.Uri;
import android.view.*;
import android.widget.*;

/** SDK-only presentation for the explicitly scoped personal Mongoose test. */
final class MongooseChrome {
    static int dp(android.content.Context c,int n){return Math.round(n*c.getResources().getDisplayMetrics().density);}
    static String version(Activity a){try{return a.getPackageManager().getPackageInfo(a.getPackageName(),0).versionName;}catch(Exception e){return "Version unavailable";}}
    static String metadata(Activity a,String key){try{return a.getPackageManager().getApplicationInfo(a.getPackageName(),android.content.pm.PackageManager.GET_META_DATA).metaData.getString(key,"");}catch(Exception e){return "";}}
    static String identity(Activity a){return a.getApplicationInfo().loadLabel(a.getPackageManager()).toString();}
    static String releaseDate(Activity a){try{return java.time.LocalDate.parse(metadata(a,"release_date")).format(java.time.format.DateTimeFormatter.ofLocalizedDate(java.time.format.FormatStyle.MEDIUM).withLocale(java.util.Locale.getDefault()));}catch(Exception e){return "Release date unavailable";}}
    static void button(Button b,boolean primary){
        b.setAllCaps(false);b.setTextSize(14);b.setMinHeight(dp(b.getContext(),48));b.setMaxLines(2);
        b.setTextColor(new ColorStateList(new int[][]{new int[]{-android.R.attr.state_enabled},new int[]{}},new int[]{0xff7f8b99,0xfff3f6fb}));
        GradientDrawable d=new GradientDrawable();d.setColor(primary?0xff256c98:0xff293240);d.setCornerRadius(dp(b.getContext(),10));d.setStroke(dp(b.getContext(),1),0xff3b4656);
        b.setBackground(new RippleDrawable(ColorStateList.valueOf(0x557cbbe4),d,null));b.setStateListAnimator(null);
    }
    static LinearLayout header(Activity a,String title){
        LinearLayout row=new LinearLayout(a);row.setGravity(Gravity.CENTER_VERTICAL);row.setPadding(8,8,8,8);
        ImageView logo=new ImageView(a);logo.setImageDrawable(a.getApplicationInfo().loadIcon(a.getPackageManager()));logo.setContentDescription("OpenSAAB logo");
        row.addView(logo,new LinearLayout.LayoutParams(dp(a,48),dp(a,48)));
        LinearLayout labels=new LinearLayout(a);labels.setOrientation(LinearLayout.VERTICAL);labels.setPadding(dp(a,8),0,0,0);
        TextView name=new TextView(a);name.setText(identity(a));name.setTextColor(0xfff3f6fb);name.setTextSize(18);labels.addView(name);
        TextView scope=new TextView(a);scope.setText(title);scope.setTextColor(0xffadb8c6);scope.setTextSize(14);labels.addView(scope);
        row.addView(labels,new LinearLayout.LayoutParams(0,-2,1));return row;
    }
    static void footer(Activity a,java.util.function.BooleanSupplier busy){
        ViewGroup content=a.findViewById(android.R.id.content);if(content.getChildCount()!=1)return;
        View old=content.getChildAt(0);content.removeView(old);LinearLayout wrapper=new LinearLayout(a);wrapper.setOrientation(LinearLayout.VERTICAL);wrapper.setBackgroundColor(0xff101318);
        wrapper.addView(old,new LinearLayout.LayoutParams(-1,0,1));Button support=new Button(a);support.setText("Donate · Support OpenSAAB");button(support,false);
        support.setOnClickListener(v->{if(busy.getAsBoolean()){Toast.makeText(a,"Finish the current test first",Toast.LENGTH_SHORT).show();return;}
            new AlertDialog.Builder(a).setTitle("Support OpenSAAB").setMessage(identity(a)+"\n\nSupport development or join our Discord community. These links open in your browser.")
                .setNegativeButton("Close",null).setNeutralButton("Discord",(d,w)->open(a,"https://discord.gg/FxnFe8vQht"))
                .setPositiveButton("Donate",(d,w)->open(a,metadata(a,"support_url"))).show();});
        wrapper.addView(support,new LinearLayout.LayoutParams(-1,dp(a,48)));content.addView(wrapper,new ViewGroup.LayoutParams(-1,-1));
        wrapper.setOnApplyWindowInsetsListener((v,i)->{v.setPadding(i.getSystemWindowInsetLeft(),i.getSystemWindowInsetTop(),i.getSystemWindowInsetRight(),i.getSystemWindowInsetBottom());return i.consumeSystemWindowInsets();});wrapper.requestApplyInsets();
    }
    private static void open(Activity a,String url){if(!url.startsWith("https://"))return;try{a.startActivity(new Intent(Intent.ACTION_VIEW,Uri.parse(url)).addCategory(Intent.CATEGORY_BROWSABLE));}catch(ActivityNotFoundException e){Toast.makeText(a,"No browser available",Toast.LENGTH_SHORT).show();}}
    private MongooseChrome(){}
}
