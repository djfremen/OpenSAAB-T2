// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;

/** Pure admission-policy fixtures; no Android, Bluetooth API, or radio use. */
public final class SppTransportAdmissionTest {
    private static final class Owner implements SppTransportAdmission.Owner {
        boolean released;
        public boolean closeConfirmed(){return released;}
    }
    private static void check(boolean value,String reason){if(!value)throw new AssertionError(reason);}
    private static void blocked(SppTransportAdmission guard,Owner owner)throws Exception{
        try{guard.reserve(owner);throw new AssertionError("Unconfirmed physical owner was replaced");}
        catch(IOException expected){check(expected.getMessage().contains("force stop OpenSAAB in Android settings"),"Missing actionable recovery");}
    }
    public static void main(String[] args)throws Exception{
        SppTransportAdmission guard=new SppTransportAdmission();Owner first=new Owner(),second=new Owner();
        guard.reserve(first);
        blocked(guard,second); // Construction itself owns admission.
        first.released=true;blocked(guard,second);first.released=false;
        guard.constructed(first);guard.requireOwner(first);
        blocked(guard,second); // Active or zombie reader cannot be bypassed.
        try{guard.requireOwner(second);throw new AssertionError("Wrong transport opened");}catch(IOException expected){}
        first.released=true; // A transient writer unwind has now completed.
        guard.reserve(second);guard.constructed(second);guard.requireOwner(second);
        try{guard.requireOwner(first);throw new AssertionError("Old owner reopened after replacement");}catch(IOException expected){}
        blocked(guard,new Owner()); // Sticky failure is still unconfirmed.
        blocked(guard,new Owner());
        try{guard.constructionFailed(second);throw new AssertionError("Constructed owner bypassed sticky failure");}catch(IllegalStateException expected){}
        SppTransportAdmission construction=new SppTransportAdmission();Owner failed=new Owner(),replacement=new Owner();
        construction.reserve(failed);construction.constructionFailed(failed);
        construction.reserve(replacement);construction.constructed(replacement);construction.requireOwner(replacement);
        SppTransportAdmission concurrent=new SppTransportAdmission();
        java.util.concurrent.CountDownLatch start=new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.atomic.AtomicInteger admitted=new java.util.concurrent.atomic.AtomicInteger();
        Runnable claim=()->{try{start.await();Owner owner=new Owner();concurrent.reserve(owner);concurrent.constructed(owner);admitted.incrementAndGet();}catch(IOException expected){}catch(InterruptedException interrupted){throw new AssertionError(interrupted);}};
        Thread a=new Thread(claim),b=new Thread(claim);a.start();b.start();start.countDown();a.join();b.join();
        check(admitted.get()==1,"Concurrent physical constructors acquired two owners");
        System.out.println("PASS: physical admission retains active/unconfirmed owner, admits later confirmed release, rejects stale opens and close-failure bypass, and abandons only failed pre-socket construction; synthetic policy fixtures only");
    }
}
