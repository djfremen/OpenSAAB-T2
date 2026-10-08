// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;
import java.net.*;
import java.util.function.BooleanSupplier;

/** One primary request and, with operator consent, one outage-only fallback. */
public final class SecurityApiClient {
    public static final String PRIMARY="https://relevant-diann-djfremen2-c013cdc3.koyeb.app/api/process";
    public static final String FALLBACK="https://sas.mysaab.info/api/process";
    public interface Transport { Response post(String endpoint,byte[] payload)throws IOException; }
    public static final class Response {
        public final int status;
        public final byte[] body;
        public Response(int status,byte[] body){this.status=status;this.body=body;}
    }
    public static final class Result {
        public final String endpoint;
        public final byte[] body;
        public final String fallbackReason;
        Result(String endpoint,byte[] body,String reason){this.endpoint=endpoint;this.body=body;this.fallbackReason=reason;}
    }
    private static boolean outage(int status){return status==500||status==502||status==503||status==504;}
    private static boolean outage(IOException error){
        // Certificate/TLS failures, malformed responses and other IO errors fail closed.
        return error instanceof SocketTimeoutException || error instanceof ConnectException
            || error instanceof UnknownHostException || error instanceof NoRouteToHostException;
    }
    private static void active(BooleanSupplier cancelled)throws IOException{
        if(cancelled.getAsBoolean())throw new IOException("Security processing cancelled; no data imported.");
    }
    public static Result process(byte[] payload,boolean allowFallback,BooleanSupplier cancelled,Transport transport)throws IOException{
        active(cancelled);
        String reason;
        try{
            Response response=transport.post(PRIMARY,payload.clone());
            active(cancelled);
            if(response.status==200)return new Result(PRIMARY,response.body,"");
            if(!outage(response.status))throw new IOException("OpenSAAB returned HTTP "+response.status+"; no fallback or import.");
            reason="HTTP "+response.status;
        }catch(IOException error){
            active(cancelled);
            if(!outage(error))throw error;
            reason=error.getClass().getSimpleName();
        }
        if(!allowFallback)throw new IOException("OpenSAAB unavailable ("+reason+"). No data sent to Bojer. Retry and allow fallback if desired.");
        active(cancelled);
        Response response=transport.post(FALLBACK,payload.clone());
        active(cancelled);
        if(response.status!=200)throw new IOException("Bojer returned HTTP "+response.status+"; no security data imported.");
        // Reply parsing and SSA validation happen before any card import, never as a fallback trigger.
        return new Result(FALLBACK,response.body,reason);
    }
    private SecurityApiClient(){}
}
