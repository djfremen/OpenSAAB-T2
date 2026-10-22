// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;

/** Read-only refusal recognition from existing native log wording; no Android or USB. */
public final class PolicyBlockTest {
    static int checks;
    static void check(boolean ok,String why){checks++;if(!ok)throw new AssertionError(why);}
    public static void main(String[] args)throws Exception{
        // Exact preview.26 Fire HD 10 Chipsoft line (2026-10-21), ANSI color retained.
        String chipsoft="\u001b[33m[ 48.345s | CANDI] [WARN ] Native adapter stopped: Chipsoft command guard blocked before USB: controller=0 CAN_ID=7E0 service=27 profile=Read; no ECU rejection\u001b[0m";
        PolicyBlock b=PolicyBlock.fromLine(chipsoft);
        check(b!=null,"Chipsoft Read refusal not recognized");
        check("27".equals(b.service)&&"7E0".equals(b.canId)&&"Chipsoft".equals(b.adapter),"Wrong fields: "+b.service+" "+b.canId);
        check(b.message().contains("security access ($27)")&&b.message().contains("not sent to the vehicle"),"Message lost the service or the no-TX statement");
        check(b.status().contains("Restart in Full session"),"Status does not name the fix");
        check(PolicyBlock.fromLine(chipsoft.replace("profile=Read","profile=Seeds"))==null,"Non-Read profile treated as Read-only refusal");
        check(PolicyBlock.fromLine(chipsoft.replace("service=27","service=unknown")).service==null,"Unknown service not tolerated");
        check(PolicyBlock.fromLine("ERROR: Native USB command rejected by collection profile")!=null,"Nano Read refusal not recognized");
        check(PolicyBlock.fromLine("UART reply timeout; guest decides retry or recovery.")==null,"Unrelated line matched");
        check(PolicyBlock.fromLine(null)==null,"Null line matched");
        check(PolicyBlock.serviceName("3B").contains("$3B")&&PolicyBlock.serviceName(null).equals("a vehicle request"),"Service naming changed");
        File dir=Files.createTempDirectory("policy-block").toFile();
        try{
            File log=new File(dir,"rust.log");
            StringBuilder big=new StringBuilder();for(int i=0;i<20000;i++)big.append("HEARTBEAT insns=").append(i).append('\n');
            Files.write(log.toPath(),(big+chipsoft+"\nREPORT stop\n").getBytes(StandardCharsets.UTF_8));
            PolicyBlock tail=PolicyBlock.scan(log);check(tail!=null&&"27".equals(tail.service),"Tail scan missed the refusal");
            Files.write(log.toPath(),big.toString().getBytes(StandardCharsets.UTF_8));
            check(PolicyBlock.scan(log)==null,"Clean log reported a refusal");
            check(PolicyBlock.scan(new File(dir,"missing.log"))==null,"Missing log reported a refusal");
        }finally{for(File f:dir.listFiles())f.delete();dir.delete();}
        // Display text: every write-capable mode is flagged, Full is explicit.
        check(SessionModes.writes(SessionModes.Mode.FULL)&&SessionModes.writes(SessionModes.Mode.CLEAR)&&SessionModes.writes(SessionModes.Mode.RESET),"Write mode not flagged");
        check(!SessionModes.writes(SessionModes.Mode.READ)&&!SessionModes.writes(SessionModes.Mode.SEEDS),"Read/Seeds flagged as writes");
        check(SessionModes.headline(SessionModes.Mode.FULL).startsWith("FULL SESSION")&&SessionModes.headline(SessionModes.Mode.FULL).contains("Vehicle changes possible"),"Full headline unclear");
        check(SessionModes.headline(SessionModes.Mode.READ).startsWith("READ-ONLY")&&SessionModes.headline(null).startsWith("OFFLINE"),"Read/offline headline unclear");
        for(SessionModes.Mode m:SessionModes.Mode.values())check(!SessionModes.detail(m).isEmpty()&&SessionModes.headline(m).length()<=44,"Badge text missing or too long: "+m);
        System.out.println("PASS PolicyBlockTest "+checks+" checks");
    }
}
