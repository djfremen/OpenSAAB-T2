package com.opensaab.usb;

public final class FirmwareMenuNavigatorTest {
    static long now;
    static FirmwareMenuNavigator nav(FirmwareMenuNavigator.Target target,int year){return new FirmwareMenuNavigator(SecurityMenuNavigatorTest.car(year,"9440"),target);}
    static void key(FirmwareMenuNavigator n,String screen,int expected){
        now+=2000;n.next(screen,now);Integer k=n.next(screen,now+100);
        if(k==null||k!=expected)throw new AssertionError("Expected "+expected+", got "+k+" on "+screen);
        n.sent(now+100);if(n.next(screen,now+200)!=null)throw new AssertionError("Repeated key");
    }
    static void none(FirmwareMenuNavigator n,String screen){now+=2000;n.next(screen,now);if(n.next(screen,now+100)!=null)throw new AssertionError("Unexpected key on "+screen);}
    static void vehicle(FirmwareMenuNavigator n,int year){
        key(n,"Main Menu\nF2: Diagnostics",0x13);
        key(n,"Model Year\n"+year,0x10);
        key(n,"Vehicle Type\nSaab 9-3 Sport (9440)",0x10);
    }
    public static void main(String[] args){
        for(int year:new int[]{2004,2008})for(FirmwareMenuNavigator.Target target:new FirmwareMenuNavigator.Target[]{FirmwareMenuNavigator.Target.READ_DTC,FirmwareMenuNavigator.Target.CLEAR_DTC}){
            FirmwareMenuNavigator n=nav(target,year);vehicle(n,year);
            none(n,"Diagnostics\nChecking Key Position\nF4: All");
            key(n,"Diagnostics\nF0: Engine\nF4: All",0x03);
            key(n,"All\nF0: Diagnostic Trouble Codes (DTC)\nF1: ECU Information",0x18);
            key(n,"Diagnostic Trouble Codes\nF2: Read DTC\nF5: Clear DTC",target==FirmwareMenuNavigator.Target.READ_DTC?0x13:0x12);
            if(n.active())throw new AssertionError("Shortcut did not end");
            none(n,"Clear all codes?\nF0: Yes\nF1: No");
            none(n,"Turn key to ON\nPress ENTER");
        }
        FirmwareMenuNavigator engine=nav(FirmwareMenuNavigator.Target.ENGINE_DATA,2008);vehicle(engine,2008);
        key(engine,"Diagnostics\nF0: Engine\nF4: All",0x18);
        none(engine,"Customer Functions\nEngine Control\nTransmission");
        key(engine,"Customer Functions\nEngine Control",0x10);
        none(engine,"Checking Key Position\nWorking");
        key(engine,"Engine Control\nF0: Diagnostic Trouble Codes\nF2: Data Display",0x13);
        none(engine,"Engine Data Display");
        engine=nav(FirmwareMenuNavigator.Target.ENGINE_DATA,2004);vehicle(engine,2004);
        key(engine,"Diagnostics\nF0: Engine",0x18);key(engine,"Customer Functions\nEngine Control",0x10);
        none(engine,"Engine Data Display");if(engine.active())throw new AssertionError("Data display must finish shortcut");
        FirmwareMenuNavigator n=nav(FirmwareMenuNavigator.Target.CLEAR_DTC,2004);vehicle(n,2004);n.cancel();none(n,"Diagnostics\nF4: All");
        n=nav(FirmwareMenuNavigator.Target.READ_DTC,2004);n.next("Unexpected menu",0);n.next("Unexpected menu",90001);none(n,"Main Menu\nF0: Diagnostics");
        none(new FirmwareMenuNavigator(null,FirmwareMenuNavigator.Target.CLEAR_DTC),"Main Menu\nF0: Diagnostics");
        if(FirmwareMenuNavigator.Target.fromShortcut("native_manual")!=null)throw new AssertionError("Ordinary Start must not select a shortcut");
        System.out.println("Diagnostic shortcuts: year/platform, Read/Clear DTC, Engine Data, confirmations, manual takeover, timeout PASS");
    }
}
