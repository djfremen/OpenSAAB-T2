// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.util.*;
import java.util.regex.*;

/** A bounded transcription of original firmware results, never a diagnostic client. */
public final class DtcReport {
    public static final int MAX_ITEMS = 200, MAX_SCREEN = 8192;
    private static final Pattern COUNTER = Pattern.compile("(?m)^\\s*(\\d{1,3})\\s*/\\s*(\\d{1,3})\\s*$");
    private static final Pattern ROW = Pattern.compile("^\\s*([A-Za-z0-9_-]{1,12})\\s+([BCPU][0-9A-F]{4})\\s+([0-9A-F]{2})\\s+(.+?)\\s*$");
    public static final class Page {
        public final int index, total;
        public final String screen, observedUtc;
        public final List<String[]> rows;
        private Page(int i,int n,String s,List<String[]> r){index=i;total=n;screen=s;rows=r;observedUtc=java.time.Instant.now().toString();}
    }
    public static Page parse(String text) {
        if(text==null || text.length()>MAX_SCREEN || !text.split("\n",2)[0].trim().equals("DTC Information"))return null;
        Matcher counter=COUNTER.matcher(text);
        if(!counter.find())return null;
        int index=Integer.parseInt(counter.group(1)), total=Integer.parseInt(counter.group(2));
        if(index<1 || index>total || total>MAX_ITEMS || counter.find())return null;
        List<String[]> rows=new ArrayList<>();
        for(String line:text.split("\n")){
            Matcher row=ROW.matcher(line);
            if(row.matches())rows.add(new String[]{row.group(1),row.group(2),row.group(3),row.group(4)});
        }
        if(rows.isEmpty())return null; // A progress screen or empty capture is not a clean scan.
        return new Page(index,total,text,rows);
    }
    public final String adapter, session, created;
    public final VehicleIdentity vehicle;
    public final int total;
    public final SortedMap<Integer,Page> pages=new TreeMap<>();
    private final Map<String,String> details=new HashMap<>();
    private static String key(String[] r){return r[0]+"/"+r[1]+"/"+r[2];}
    /** Only accept the firmware's separate, unnumbered code-description screen. */
    public boolean addDetail(String text){
        if(text==null || text.length()>MAX_SCREEN || COUNTER.matcher(text).find())return false;
        List<String> lines=new ArrayList<>();
        for(String line:text.split("\n"))if(!line.trim().isEmpty())lines.add(line.trim());
        if(lines.size()<4 || !lines.get(0).equals("DTC Information")
            || !lines.get(1).matches("[A-Za-z0-9_-]{1,12}")
            || !lines.get(2).matches("[BCPU][0-9A-F]{4}\\s+[0-9A-F]{2}"))return false;
        String[] code=lines.get(2).split("\\s+");
        String k=lines.get(1)+"/"+code[0]+"/"+code[1];
        if(!entries().containsKey(k))return false;
        String description=String.join(" ",lines.subList(3,lines.size()));
        details.put(k,description);
        return true;
    }
    /** Overlapping screens are observations, not additional faults. */
    private SortedMap<String,String[]> entries(){
        SortedMap<String,String[]> found=new TreeMap<>();
        for(Page p:pages.values())for(String[] r:p.rows){
            String[] old=found.get(key(r));
            if(old==null || r[3].length()>old[3].length())found.put(key(r),r);
        }
        return found;
    }
    private boolean repeated(String key){
        for(Page p:pages.values()){
            int count=0;
            for(String[] r:p.rows)if(key(r).equals(key) && ++count>1)return true;
        }
        return false;
    }
    public int codeCount(){return entries().size();}
    public DtcReport(String adapter,String session,Page first){
        this(adapter,session,first,null);
    }
    public DtcReport(String adapter,String session,Page first,VehicleIdentity vehicle){
        this.vehicle=vehicle;
        this.adapter=adapter;this.session=session;total=first.total;
        created=java.time.Instant.now().toString();add(first);
    }
    public boolean add(Page page){
        if(page.total!=total)throw new IllegalArgumentException("DTC count changed");
        Page old=pages.get(page.index);
        if(old!=null && old.screen.equals(page.screen))return false;
        pages.put(page.index,page);return true;
    }
    public boolean complete(){return pages.size()==total;}
    public String summary(){return codeCount()+" distinct fault code"+(codeCount()==1?"":"s")+" captured.\n"
        +(complete()?"All "+total+" entries from the displayed list were captured.":"Partial list — scroll through the remaining codes to finish capturing results.")+"\n"
        +"This is coverage of the displayed list, not a count of vehicle modules scanned.";}
    public String text(){
        StringBuilder out=new StringBuilder("OpenSAAB DTC report\n").append(summary())
            .append("\nAndroid · ").append(adapter).append(" · Original Tech2 firmware")
            .append("\nDate (UTC): ").append(created)
            .append("\nVIN: ").append(vehicle==null?"not captured":vehicle.vin).append('\n');
        if(vehicle!=null)out.append("Vehicle: ").append(vehicle.description()).append("\nVIN read at (UTC): ").append(vehicle.observedUtc).append('\n');
        String module="";
        for(Map.Entry<String,String[]> e:entries().entrySet()){
            String[] r=e.getValue();
            if(!module.equals(r[0])){module=r[0];out.append('\n').append(module).append('\n');}
            out.append("  ").append(r[1]).append('-').append(r[2]).append(" — ")
                .append(details.containsKey(e.getKey())?details.get(e.getKey()):r[3]+"…")
                .append(repeated(e.getKey())?" [repeated in firmware list]":"").append('\n');
        }
        out.append("\nCodes are grouped by module. A repeated code is listed once; different symptom suffixes stay separate.\n")
            .append("… means only list text was captured; open that code in the firmware for its full description.\n")
            .append("Current versus historical fault status is not provided by these screens.\n")
            .append("Report collection does not clear codes. Raw screen observations remain in the saved JSON log.\n")
            .append("\n---\nSupport OpenSAAB\n")
            .append("If OpenSAAB helped you, please consider supporting the project. Your donation helps fund development and adapter testing.\n")
            .append("https://ko-fi.com/djfremen\n")
            .append("Donations are always optional. Thank you for your support!\n");
        return out.toString();
    }
    public String json(){
        StringBuilder out=new StringBuilder("{\"schema\":1,\"source\":\"original-firmware-lcd\",\"platform\":\"Android\",\"adapter\":")
            .append(quote(adapter)).append(",\"session\":").append(quote(session)).append(",\"created_utc\":").append(quote(created))
            .append(",\"vin\":").append(vehicle==null?"null":quote(vehicle.vin))
            .append(",\"vehicle_description\":").append(vehicle==null?"null":quote(vehicle.description()))
            .append(",\"vin_observed_utc\":").append(vehicle==null?"null":quote(vehicle.observedUtc))
            .append(",\"all_list_positions_captured\":").append(complete()).append(",\"total_positions\":").append(total)
            .append(",\"captured_positions\":").append(pages.size()).append(",\"unique_codes\":").append(codeCount())
            .append(",\"full_descriptions\":{");
        boolean dc=false;
        for(Map.Entry<String,String> d:new TreeMap<>(details).entrySet()){
            if(dc)out.append(',');dc=true;out.append(quote(d.getKey())).append(':').append(quote(d.getValue()));
        }
        out.append("},\"snapshots\":[");
        boolean comma=false;
        for(Page p:pages.values()){
            if(comma)out.append(',');comma=true;
            out.append("{\"selected_item\":").append(p.index).append(",\"observed_utc\":").append(quote(p.observedUtc)).append(",\"screen\":").append(quote(p.screen)).append(",\"visible_rows\":[");
            boolean rc=false;
            for(String[] r:p.rows){if(rc)out.append(',');rc=true;out.append("{\"module\":").append(quote(r[0])).append(",\"dtc\":").append(quote(r[1])).append(",\"suffix\":").append(quote(r[2])).append(",\"description_as_displayed\":").append(quote(r[3])).append('}');}
            out.append("]}");
        }
        return out.append("]}\n").toString();
    }
    private static String quote(String s){
        StringBuilder out=new StringBuilder("\"");
        for(char c:s.toCharArray()){
            if(c=='"'||c=='\\')out.append('\\').append(c);
            else if(c<32)out.append(String.format(Locale.ROOT,"\\u%04x",(int)c));else out.append(c);
        }
        return out.append('"').toString();
    }
}
