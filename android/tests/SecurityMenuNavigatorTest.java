package com.opensaab.usb;

public final class SecurityMenuNavigatorTest {
    static VehicleIdentity car(int year,String platform){return new VehicleIdentity("YS3FD49YX41000001","now","test",year,"available",platform,"","","","");}
    static long now;
    static Integer observe(SecurityMenuNavigator n,String s){now+=1000;n.next(s,now);return n.next(s,now+600);}
    static void key(SecurityMenuNavigator n,String screen,int expected){
        Integer k=observe(n,screen);if(k==null||k!=expected)throw new AssertionError("Expected "+expected+", got "+k+" on "+screen);
        n.sent(now+600);if(n.next(screen,now+2000)!=null)throw new AssertionError("Repeated stale key");
    }
    static void none(SecurityMenuNavigator n,String screen){if(observe(n,screen)!=null)throw new AssertionError("Unexpected key on "+screen);}
    static void runYear(int year){
        SecurityMenuNavigator n=new SecurityMenuNavigator(car(year,"9-3 Sport Sedan / 9440"));
        key(n,"Main Menu\nF0: Diagnostics\nF1: Service",0x18);
        for(int shown=2012;shown>=year;shown--)key(n,"Model Year "+(2013-shown)+" / 15\n("+(shown==2004?"4":"8")+") "+shown,shown==year?0x10:0x0c);
        key(n,"Vehicle Type 1 / 2\nSaab 9-3 Sport (9440)\nSAAB 9-5",0x0c);
        key(n,"Vehicle Type 2 / 2\nSAAB 9-5\nSaab 9-3 Sport (9440)",0x10);
        key(n,"Diagnostics\nF0: Engine\nF4: All",0x03);
        key(n,"All\nF0: Diagnostic Trouble Codes (DTC)\nF1: ECU Information\nF6: Get Security Access",0x16);
        none(n,"Turn Ignition key to LOCK position\nPress ENTER");
        none(n,"Remove Ignition key\nPress ENTER");
        none(n,"Main Menu\nF0: Diagnostics");
    }
    public static void main(String[] args){
        runYear(2004);runYear(2008);
        SecurityMenuNavigator n=new SecurityMenuNavigator(car(2004,"9440"));
        none(n,"Main Menu\nF0: Erase all");
        key(n,"Main Menu\nF2: Diagnostics",0x13);
        none(n,"Model Year 9 / 15\n(4) 2004\nUnknown footer");
        key(n,"Model Year 10 / 15\n(3) 2003",0x09);
        key(n,"Model Year 9 / 15\n(4) 2004",0x10);
        none(n,"Vehicle Type\nSaab 9-3 Sport (9440)\nSAAB 9-3 Convertible");
        n.cancel();none(n,"Vehicle Type\nSaab 9-3 Sport (9440)");
        none(new SecurityMenuNavigator(car(2004,"")),"Main Menu\nF0: Diagnostics");
        none(new SecurityMenuNavigator(car(2004,"9400")),"Main Menu\nF0: Diagnostics");
        n=new SecurityMenuNavigator(car(2004,"9440"));
        n.next("unknown",0);n.next("unknown",90001);none(n,"Main Menu\nF0: Diagnostics");
        n=new SecurityMenuNavigator(car(2008,"9440"));
        String menu="Main Menu\nF0: Diagnostics";
        Integer proposed=observe(n,menu);
        if(proposed==null||n.next(menu,now+700)==null)throw new AssertionError("Unaccepted key must remain available");
        System.out.println("Security menu navigation: 2004/2008, label selection, stale screens, manual takeover and timeout PASS");
    }
}
