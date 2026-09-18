#!/usr/bin/env python3
"""Exercise deferred bridge wait, cancellation, child exit, and package isolation."""
from pathlib import Path
import subprocess,tempfile
repo=Path(__file__).resolve().parents[2]
jdk=Path('/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin')
with tempfile.TemporaryDirectory(prefix='opensaab-demand-') as tmp:
 root=Path(tmp)
 sources={
 'android/content/Context.java':'''package android.content;
 public class Context { public String name="com.opensaab.tech2.headunit32.test";
 public String getPackageName(){return name;}
 public android.content.pm.ApplicationInfo getApplicationInfo(){return new android.content.pm.ApplicationInfo();}}''',
 'android/content/pm/ApplicationInfo.java':'''package android.content.pm;
 public class ApplicationInfo {public static final int FLAG_DEBUGGABLE=2;public int flags=2;}''',
 'android/os/SystemClock.java':'''package android.os;public class SystemClock {
 public static long elapsedRealtime(){return System.nanoTime()/1000000;}}''',
 'DemandStartupTest.java':'''import java.net.*;import java.io.*;import java.util.concurrent.*;
 import java.util.concurrent.atomic.*;import com.opensaab.usb.DemandStartup;
 public class DemandStartupTest {
 static class Child extends Process {
 boolean alive=true;public boolean isAlive(){return alive;}
 public InputStream getInputStream(){return new ByteArrayInputStream(new byte[0]);}
 public InputStream getErrorStream(){return getInputStream();}
 public OutputStream getOutputStream(){return new ByteArrayOutputStream();}
 public int waitFor(){return 0;}public int exitValue(){return 0;}public void destroy(){alive=false;}}
 static void check(boolean ok){if(!ok)throw new AssertionError();}
 public static void main(String[] args)throws Exception {
 android.content.Context context=new android.content.Context();
 ProcessBuilder command=new ProcessBuilder("native","--candi-native-link");
 DemandStartup.configure(context,command);check(command.command().contains("--candi-on-demand"));
 check(command.environment().get("LOAD_TEST_ACCELERATE").equals("1"));
 ExecutorService pool=Executors.newSingleThreadExecutor();
 try(ServerSocket server=new ServerSocket(0,1,InetAddress.getLoopbackAddress())){
 Child child=new Child();AtomicBoolean cancelled=new AtomicBoolean();
 Future<Socket> accepted=pool.submit(()->DemandStartup.accept(context,server,command,child,cancelled::get));
 Thread.sleep(1100);check(!accepted.isDone()); // crossed the one-second poll timeout
 try(Socket client=new Socket(InetAddress.getLoopbackAddress(),server.getLocalPort());Socket peer=accepted.get(2,TimeUnit.SECONDS)){check(peer.isConnected());}
 Future<Boolean> stopped=pool.submit(()->{try{DemandStartup.accept(context,server,command,child,cancelled::get);return false;}catch(IOException expected){return true;}});
 cancelled.set(true);check(stopped.get(2,TimeUnit.SECONDS));
 cancelled.set(false);child.alive=false;
 try{DemandStartup.accept(context,server,command,child,cancelled::get);throw new AssertionError();}catch(IOException expected){}
 context.name="com.opensaab.tech2";
 ProcessBuilder release=new ProcessBuilder("native","--candi-native-link");DemandStartup.configure(context,release);
 check(!release.command().contains("--candi-on-demand"));
 server.setSoTimeout(30);
 try{DemandStartup.accept(context,server,release,child,()->false);throw new AssertionError();}catch(SocketTimeoutException expected){}
 }finally{pool.shutdownNow();}
 System.out.println("PASS: deferred connection, cancellation, child exit, production isolation");
 }}'''}
 for name,source in sources.items():
  p=root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(source)
 subprocess.run([str(jdk/'javac'),'-d',str(root),*[str(root/name) for name in sources],str(repo/'android/shared/com/opensaab/usb/DemandStartup.java')],check=True)
 subprocess.run([str(jdk/'java'),'-cp',str(root),'DemandStartupTest'],check=True,timeout=10)
