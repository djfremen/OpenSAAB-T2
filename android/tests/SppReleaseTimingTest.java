// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Exact fake monotonic clock receipts, with no Android API/radio or sleeping. */
public final class SppReleaseTimingTest {
    private static final class Clock implements SppReleaseTiming.Clock {long now;public long nowMs(){return now;}}
    private static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    private static Long field(SppReleaseTiming.Attempt attempt,String key){return (Long)attempt.snapshot().get(key);}
    public static void main(String[] args){
        SppReleaseTiming.Ledger ledger=new SppReleaseTiming.Ledger();Clock clock=new Clock();clock.now=100;
        SppReleaseTiming.Attempt first=ledger.attempt("adapter-A","boot-1",clock);first.connectStarted();
        check(field(first,"connect_started_elapsed_ms")==100&&field(first,"prior_confirmed_release_elapsed_ms")==null&&field(first,"release_to_connect_ms")==null,"Unknown prior release was invented");
        clock.now=110;first.connectFinished();clock.now=115;first.writeStarted("AT",false);clock.now=117;first.writeCompleted();
        clock.now=130;first.writeStarted("021A900000000000",false);clock.now=140;first.releaseStarted();
        check(field(first,"confirmed_release_elapsed_ms")==null,"Close intent claimed a confirmed release");
        clock.now=150;first.releaseConfirmed();clock.now=155;first.releaseConfirmed();
        check(field(first,"first_probe_write_elapsed_ms")==115&&field(first,"first_vehicle_write_elapsed_ms")==130&&field(first,"first_write_completed_elapsed_ms")==117&&field(first,"confirmed_release_elapsed_ms")==150,"Probe/write/release moments were conflated or overwritten");
        clock.now=200;SppReleaseTiming.Attempt next=ledger.attempt("adapter-A","boot-1",clock);next.connectStarted();
        check(field(next,"prior_confirmed_release_elapsed_ms")==150&&field(next,"release_to_connect_ms")==50,"Exact same-adapter/same-boot release interval lost");
        SppReleaseTiming.Attempt wrongAdapter=ledger.attempt("adapter-B","boot-1",clock);wrongAdapter.connectStarted();check(field(wrongAdapter,"prior_confirmed_release_elapsed_ms")==null,"Another adapter's release leaked");
        SppReleaseTiming.Attempt wrongBoot=ledger.attempt("adapter-A","boot-2",clock);wrongBoot.connectStarted();check(field(wrongBoot,"prior_confirmed_release_elapsed_ms")==null,"Another boot's release leaked");
        SppReleaseTiming.Attempt unconfirmed=ledger.attempt("adapter-A","boot-1",clock);unconfirmed.connectStarted();check(field(unconfirmed,"prior_confirmed_release_elapsed_ms")==null,"Unconfirmed intervening attempt reused old history");
        clock.now=220;next.releaseConfirmed();clock.now=210;SppReleaseTiming.Attempt regressed=ledger.attempt("adapter-A","boot-1",clock);regressed.connectStarted();check(field(regressed,"release_to_connect_ms")==null&&!regressed.snapshot().get("timing_error").equals(""),"Clock regression produced a negative interval");
        SppReleaseTiming.Attempt restarted=new SppReleaseTiming.Ledger().attempt("adapter-A","boot-1",clock);restarted.connectStarted();check(field(restarted,"prior_confirmed_release_elapsed_ms")==null,"Process restart inferred an unobserved release");
        System.out.println("PASS: exact monotonic connect/probe/write/release receipts; adapter+boot isolation, unknown prior release, close intent versus confirmation, no stale history and no negative interval");
    }
}
