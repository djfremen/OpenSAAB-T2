// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Immutable identity of one live vehicle connection; never inferred from a previous car. */
public final class VehicleIdentity {
    public final String vin, observedUtc, source, lookupStatus, platform, engine, color, gearbox, body;
    public final int modelYear;
    public VehicleIdentity(String vin,String observedUtc,String source,int year,String status,
            String platform,String engine,String color,String gearbox,String body){
        if(vin==null || !vin.matches("YS3[A-HJ-NPR-Z0-9]{14}"))throw new IllegalArgumentException("Expected a complete Saab VIN from the vehicle");
        this.vin=vin;this.observedUtc=observedUtc;this.source=source;modelYear=year;
        lookupStatus=status;this.platform=platform;this.engine=engine;this.color=color;this.gearbox=gearbox;this.body=body;
    }
    public static int year(String vin){
        if(vin==null || vin.length()!=17)return 0;
        char c=vin.charAt(9);
        if(c=='W')return 1998;if(c=='X')return 1999;if(c=='Y')return 2000;
        if(c>='1'&&c<='9')return 2000+c-'0';if(c>='A'&&c<='C')return 2010+c-'A';return 0;
    }
    public String description(){
        StringBuilder s=new StringBuilder();
        if(modelYear>0)s.append(modelYear);
        for(String v:new String[]{platform,engine,color})if(v!=null&&!v.isEmpty()){
            if(s.length()>0)s.append(" · ");s.append(v);
        }
        return s.length()==0?"Vehicle identified":s.toString();
    }
}
