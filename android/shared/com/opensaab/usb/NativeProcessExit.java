// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.util.concurrent.TimeUnit;

/** Worker-thread termination barrier. Image/adapter ownership outlives the actual process. */
public final class NativeProcessExit {
    public static final class Result {
        public final Integer exitCode;
        public final boolean forced;
        Result(Integer exitCode,boolean forced){this.exitCode=exitCode;this.forced=forced;}
    }
    private NativeProcessExit(){}
    public static Result await(Process child,long graceMillis){
        if(child==null)return new Result(null,false);
        boolean interrupted=false,forced=false;
        long deadline=System.nanoTime()+TimeUnit.MILLISECONDS.toNanos(Math.max(0,graceMillis));
        while(child.isAlive()){
            long left=deadline-System.nanoTime();if(left<=0)break;
            try{child.waitFor(Math.max(1,Math.min(250,TimeUnit.NANOSECONDS.toMillis(left))),TimeUnit.MILLISECONDS);}
            catch(InterruptedException stop){interrupted=true;break;}
        }
        if(child.isAlive()){
            forced=true;child.destroyForcibly();
            // destroyForcibly is asynchronous. Never release the image lease on its return.
            while(child.isAlive())try{child.waitFor(250,TimeUnit.MILLISECONDS);}
                catch(InterruptedException stop){interrupted=true;}
        }
        Integer exit=child.exitValue();if(interrupted)Thread.currentThread().interrupt();
        return new Result(exit,forced);
    }
}
