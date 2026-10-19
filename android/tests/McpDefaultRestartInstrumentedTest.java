// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import android.app.*;import android.content.*;import android.os.*;
import java.io.*;import java.nio.charset.StandardCharsets;import java.nio.file.*;import java.security.*;
/** Three separate app processes prove default-on, retained pairing and explicit-off persistence. */
public final class McpDefaultRestartInstrumentedTest extends Instrumentation {
    private Bundle arguments;
    public void onCreate(Bundle args){super.onCreate(args);arguments=args;start();}
    private static void check(boolean ok,String message){if(!ok)throw new AssertionError(message);}
    private String digest(File token)throws Exception{
        byte[] data=MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(token.toPath()));StringBuilder b=new StringBuilder();for(byte v:data)b.append(String.format(java.util.Locale.ROOT,"%02x",v&255));return b.toString();
    }
    public void onStart(){Bundle result=new Bundle();int code=-1;Activity activity=null;
        try{
            Context context=getTargetContext();DisposableEmulator.require(context);DisposableEmulator.requireInstalledVersion(context,arguments);
            String phase=arguments.getString("phase");File token=new File(context.getNoBackupFilesDir(),"mcp-pairing-token"),fixture=new File(context.getCacheDir(),"mcp-restart-fixture.sha256");
            if("seed".equals(phase)){context.getSharedPreferences("mcp_control",0).edit().clear().commit();token.delete();fixture.delete();}
            Intent intent=new Intent(Intent.ACTION_MAIN).setClassName(context,"com.opensaab.tech2.MainActivity").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("mcp_test",true);
            activity=startActivitySync(intent);waitForIdleSync();final Activity a=activity;
            runOnMainSync(()->{android.view.View notice=a.findViewById(android.R.id.content).findViewWithTag("preview-continue");if(notice!=null)notice.performClick();});waitForIdleSync();
            if("seed".equals(phase)){
                check(AndroidMcp.enabled()&&token.isFile(),"New install policy did not start foreground MCP");Files.write(fixture.toPath(),digest(token).getBytes(StandardCharsets.US_ASCII));
            }else if("resume".equals(phase)){
                check(AndroidMcp.enabled()&&token.isFile(),"Cold restart did not start foreground MCP");
                check(digest(token).equals(new String(Files.readAllBytes(fixture.toPath()),StandardCharsets.US_ASCII)),"Cold restart rotated paired credential");
                runOnMainSync(AndroidMcp::disable);check(!token.exists(),"Disable retained pairing credential");
            }else if("disabled".equals(phase)){
                check(!AndroidMcp.enabled()&&!token.exists(),"Cold restart ignored explicit Disable");
                runOnMainSync(()->{try{AndroidMcp.enable(a);}catch(IOException e){throw new RuntimeException(e);}});
                check(AndroidMcp.enabled()&&!digest(token).equals(new String(Files.readAllBytes(fixture.toPath()),StandardCharsets.US_ASCII)),"Re-enable failed to rotate revoked credential");fixture.delete();
            }else throw new AssertionError("Unknown phase");
            result.putInt("app_pid",android.os.Process.myPid());result.putString("stream","PASS: MCP cold-process policy "+phase+"; no adapter or vehicle commands\n");
        }catch(Throwable error){code=0;result.putString("stream","FAIL: "+error+"\n");}
        finally{final Activity a=activity;if(a!=null)runOnMainSync(a::finish);}finish(code,result);
    }
}
