// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import java.io.IOException;
import java.net.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import javax.net.ssl.SSLHandshakeException;

/** Synthetic transport only: never sends a VIN or security request to a live service. */
public final class SecurityApiClientTest {
    static final byte[] PAYLOAD={1,2,3}, REPLY={4,5,6};
    static void check(boolean value,String why){if(!value)throw new AssertionError(why);}
    static final class Fake implements SecurityApiClient.Transport {
        final List<String> endpoints=new ArrayList<>();
        Object primary=new SecurityApiClient.Response(200,REPLY);
        Object fallback=new SecurityApiClient.Response(200,REPLY);
        Runnable afterPrimary=()->{};
        public SecurityApiClient.Response post(String endpoint,byte[] payload)throws IOException{
            check(Arrays.equals(PAYLOAD,payload),"Original payload was not preserved");
            payload[0]=99; // A transport must not be able to change the next request.
            endpoints.add(endpoint);
            boolean first=endpoints.size()==1;
            check(endpoint.equals(first?SecurityApiClient.PRIMARY:SecurityApiClient.FALLBACK),"Wrong provider order");
            check(endpoints.size()<=2,"Unbounded retry");
            if(first)afterPrimary.run();
            Object value=first?primary:fallback;
            if(value instanceof IOException)throw (IOException)value;
            return (SecurityApiClient.Response)value;
        }
    }
    static SecurityApiClient.Result run(Fake fake,boolean consent)throws IOException{
        return SecurityApiClient.process(PAYLOAD,consent,()->false,fake);
    }
    static void rejects(Fake fake,boolean consent,int calls)throws Exception{
        try{run(fake,consent);throw new AssertionError("Failure accepted");}catch(IOException expected){}
        check(fake.endpoints.size()==calls,"Unexpected provider request");
    }
    public static void main(String[] args)throws Exception{
        Fake success=new Fake();
        check(run(success,true).endpoint.equals(SecurityApiClient.PRIMARY)&&success.endpoints.size()==1,"Primary not preferred");
        for(int status:new int[]{500,502,503,504}){
            Fake fake=new Fake();fake.primary=new SecurityApiClient.Response(status,new byte[0]);
            SecurityApiClient.Result result=run(fake,true);
            check(result.endpoint.equals(SecurityApiClient.FALLBACK)&&result.fallbackReason.equals("HTTP "+status),"Outage did not fall back");
            Fake denied=new Fake();denied.primary=fake.primary;rejects(denied,false,1);
        }
        for(int status:new int[]{301,302,307,308,400,401,403,404,408,409,422,429,501}){
            Fake fake=new Fake();fake.primary=new SecurityApiClient.Response(status,new byte[0]);rejects(fake,true,1);
        }
        for(IOException error:new IOException[]{new SocketTimeoutException(),new ConnectException(),new UnknownHostException(),new NoRouteToHostException()}){
            Fake fake=new Fake();fake.primary=error;
            check(run(fake,true).endpoint.equals(SecurityApiClient.FALLBACK),"Network outage did not fall back");
            Fake denied=new Fake();denied.primary=error;rejects(denied,false,1);
        }
        for(IOException error:new IOException[]{new SSLHandshakeException("certificate"),new IOException("response too large")}){
            Fake fake=new Fake();fake.primary=error;rejects(fake,true,1);
        }
        Fake invalid=new Fake();invalid.primary=new SecurityApiClient.Response(200,new byte[]{0});
        SecurityApiClient.Result raw=run(invalid,true);
        check(raw.body.length==1&&invalid.endpoints.size()==1,"Invalid successful body retried");
        byte[] input=new byte[SsaData.SIZE];Arrays.fill(input,(byte)255);input[0]=(byte)0xb1;
        System.arraycopy("YS3FF49Y541000000".getBytes("US-ASCII"),0,input,0x14,17);
        input[0x132]=0;input[0x133]=1;input[0x134]=3;input[0x135]=0x61;input[0x136]=0x12;input[0x137]=0x34;
        try{SsaData.validateReply(input,raw.body);throw new AssertionError("Invalid reply accepted");}catch(IllegalArgumentException expected){}
        for(Object error:new Object[]{new SecurityApiClient.Response(503,new byte[0]),new SocketTimeoutException()}){
            Fake fake=new Fake();fake.primary=new ConnectException();fake.fallback=error;rejects(fake,true,2);
        }
        AtomicBoolean cancelled=new AtomicBoolean();Fake stopped=new Fake();
        stopped.primary=new SocketTimeoutException();stopped.afterPrimary=()->cancelled.set(true);
        try{SecurityApiClient.process(PAYLOAD,true,cancelled::get,stopped);throw new AssertionError("Cancelled request accepted");}catch(IOException expected){}
        check(stopped.endpoints.size()==1,"Cancellation sent data to fallback");
        Fake never=new Fake();
        try{SecurityApiClient.process(PAYLOAD,true,()->true,never);throw new AssertionError("Pre-cancel accepted");}catch(IOException expected){}
        check(never.endpoints.isEmpty(),"Pre-cancel sent data");
        System.out.println("PASS: primary preference, outage fallback, consent, payload preservation, denial/TLS/redirect/invalid-response rejection, bounded retries and cancellation; synthetic transport only");
    }
}
