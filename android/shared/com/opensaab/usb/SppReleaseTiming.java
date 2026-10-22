// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.util.*;

/** Pure monotonic receipts. History is process-local: a restart makes prior release unknown. */
final class SppReleaseTiming {
    interface Clock { long nowMs(); }
    static final class Ledger {
        private final Map<List<String>,Long> releases=new LinkedHashMap<>();
        private final String unknownBoot=UUID.randomUUID().toString();
        Attempt attempt(String adapter,String bootId,Clock clock){return new Attempt(this,adapter,bootId,clock);}
        private List<String> key(String adapter,String bootId){return Arrays.asList(bootId==null?unknownBoot:bootId,adapter);}
        private synchronized Long begin(String adapter,String bootId){return releases.remove(key(adapter,bootId));}
        private synchronized void released(String adapter,String bootId,long at){
            releases.put(key(adapter,bootId),at);
            if(releases.size()>64)releases.remove(releases.keySet().iterator().next());
        }
    }
    static final class Attempt {
        private final Ledger ledger;private final String adapter,bootId;private final Clock clock;
        private Long connectedAt,connectFinishedAt,priorRelease,firstWrite,firstProbe,firstVehicle,firstWriteCompleted,releaseStarted,releasedAt,lastClock;
        private String error="";
        private Attempt(Ledger ledger,String adapter,String bootId,Clock clock){
            this.ledger=Objects.requireNonNull(ledger);this.adapter=Objects.requireNonNull(adapter);this.bootId=bootId;this.clock=Objects.requireNonNull(clock);
        }
        private Long now(){
            long value=clock.nowMs();
            if(value<0||(lastClock!=null&&value<lastClock)){error="Monotonic timing was not comparable";return null;}
            lastClock=value;return value;
        }
        synchronized void connectStarted(){
            if(connectedAt!=null)return;
            Long previous=ledger.begin(adapter,bootId);connectedAt=now();
            if(previous!=null&&connectedAt!=null&&previous<=connectedAt)priorRelease=previous;
            else if(previous!=null)error="Previous release clock was not comparable";
        }
        synchronized void connectFinished(){if(connectFinishedAt==null)connectFinishedAt=now();}
        synchronized void writeStarted(String command,boolean interrupt){
            Long at=now();if(firstWrite==null)firstWrite=at;
            if(!interrupt&&(command.equals("AT")||command.equals("ATI"))&&firstProbe==null)firstProbe=at;
            if(!interrupt&&command.matches("[0-9A-Fa-f]+")&&firstVehicle==null)firstVehicle=at;
        }
        synchronized void writeCompleted(){if(firstWriteCompleted==null)firstWriteCompleted=now();}
        synchronized void releaseStarted(){if(releaseStarted==null)releaseStarted=now();}
        synchronized void releaseConfirmed(){
            if(releasedAt!=null)return;
            releasedAt=now();if(releasedAt!=null)ledger.released(adapter,bootId,releasedAt);
        }
        synchronized Map<String,Object> snapshot(){
            Map<String,Object> fields=new LinkedHashMap<>();
            fields.put("clock","android_elapsed_realtime");fields.put("clock_boot_id",bootId);
            fields.put("release_history_scope","current_process");
            fields.put("connect_started_elapsed_ms",connectedAt);fields.put("connect_finished_elapsed_ms",connectFinishedAt);
            fields.put("prior_confirmed_release_elapsed_ms",priorRelease);
            fields.put("release_to_connect_ms",priorRelease==null||connectedAt==null?null:connectedAt-priorRelease);
            fields.put("first_transport_write_elapsed_ms",firstWrite);fields.put("first_probe_write_elapsed_ms",firstProbe);
            fields.put("first_vehicle_write_elapsed_ms",firstVehicle);fields.put("first_write_completed_elapsed_ms",firstWriteCompleted);
            fields.put("release_started_elapsed_ms",releaseStarted);fields.put("confirmed_release_elapsed_ms",releasedAt);
            fields.put("confirmed_release_time_basis","host_observed_all_transport_resources_terminated");
            fields.put("timing_error",error);return Collections.unmodifiableMap(fields);
        }
    }
}
