// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

public final class DtcReportTest {
    static void check(boolean value,String message){if(!value)throw new AssertionError(message);}
    static String screen(int index,int total){return "            DTC Information             \n\n"
        +"ACC       B0260  06 Vent Door Motor Left\n"
        +"ICM2      B1000  08 Control Module. Inte\n"
        +"ICM2      B1000  08 Control Module. Inte\n"
        +"                               "+index+" / "+total+"\n\n";}
    public static void main(String[] args){
        check(DtcReport.parse("DTC Information\nACC") == null,"progress accepted");
        check(DtcReport.parse("Main Menu\nF0 Read DTC Information\n1 / 2")==null,"menu accepted");
        check(DtcReport.parse(screen(0,3))==null,"zero index");
        check(DtcReport.parse(screen(4,3))==null,"index beyond total");
        check(DtcReport.parse(screen(1,201))==null,"unbounded capture");
        check(DtcReport.parse(screen(1,3)+"2 / 3\n")==null,"ambiguous counter");
        check(DtcReport.parse("DTC Information\nNo DTCs\n1 / 1")==null,"unrecognized empty list claimed clean");
        DtcReport.Page page=DtcReport.parse(screen(2,3));
        check(page!=null && page.rows.size()==3,"duplicate rows lost");
        check(page.rows.get(1)[3].equals("Control Module. Inte"),"clipped description invented");
        DtcReport report=new DtcReport("chipsoft","synthetic",page);
        check(!report.complete() && report.pages.size()==1,"partial claimed complete");
        check(!report.add(page),"unchanged screen duplicates");
        report.add(DtcReport.parse(screen(1,3)));report.add(DtcReport.parse(screen(3,3)));
        check(report.complete(),"coverage incomplete");
        check(report.json().contains("\"vin\":null"),"invented VIN");
        check(report.json().contains("\\u000a"),"invalid JSON newline escaping");
        check(report.text().contains("not a count of vehicle modules scanned"),"coverage overstated");
        check(report.codeCount()==2,"overlapping screens inflated code count");
        check(report.text().split("B1000-08",-1).length==2,"duplicate code printed more than once");
        check(report.text().contains("[repeated in firmware list]"),"real duplicate not marked");
        check(!report.text().contains("--- Selected item"),"raw screen dump leaked into report");
        check(report.json().contains("Selected item")==false && report.json().contains("visible_rows"),"raw evidence lost");
        check(!report.addDetail("DTC Information\nUEC\nB2699 04\nLow Beam Right Circuit, Open\n"),"unrelated detail merged");
        check(report.addDetail("DTC Information\nICM2\nB1000 08\nControl Module.\nInternal fault\n"),"full description not captured");
        check(report.addDetail("DTC Information\nICM2\nB1000 08\nControl Module.\nInternal fault\n"),"stable detail screen no longer recognized");
        check(report.text().contains("B1000-08 — Control Module. Internal fault [repeated"),"full description not used");
        check(report.json().contains("full_descriptions"),"full description evidence not saved");
        check(!report.addDetail("Main Menu\nICM2\nB1000 08\nUnknown"),"wrong screen accepted as detail");
        DtcReport variant=new DtcReport("nano","different-symptoms",DtcReport.parse("DTC Information\nUEC B1000 08 Internal\nUEC B1000 04 Other\nACC B1000 08 Different module\n1 / 3\n"));
        check(variant.codeCount()==3,"different modules or symptom suffixes merged");
        check(!variant.text().contains("[repeated"),"distinct faults labeled duplicates");
        check(variant.summary().contains("Partial list"),"partial coverage hidden");
        VehicleIdentity vehicle=new VehicleIdentity("YS3FD49YX41000001","2026-09-10T00:00:00Z","test ECU",2004,"available","9440","B207R","Silver","FA57","Sedan");
        check(VehicleIdentity.year(vehicle.vin)==2004,"VIN suffix was mistaken for year");
        DtcReport identified=new DtcReport("chipsoft","vehicle-A",page,vehicle);
        check(identified.text().contains(vehicle.vin)&&identified.json().contains("\"vin\":\""+vehicle.vin+"\""),"VIN missing from report");
        check(new DtcReport("chipsoft","vehicle-B",page).json().contains("\"vin\":null"),"VIN leaked across sessions");
        try{new VehicleIdentity("2017","now","test",2017,"pending","","","","","");throw new AssertionError("VIN suffix accepted as VIN");}catch(IllegalArgumentException expected){}
        try{report.add(DtcReport.parse(screen(1,4)));throw new AssertionError("mixed totals");}catch(IllegalArgumentException expected){}
        System.out.println("PASS: DTC detection, partial/full coverage, overlapping screens, genuine duplicates, distinct symptoms/modules, full descriptions, raw evidence and bounded results");
    }
}
