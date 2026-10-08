// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.Arrays;

/** The whole ASCII TX request crosses the production codec before the gate.
 * Optional files are synthetic lines exported by the production Rust SocketUsb. */
public final class NanoBridgeEnvelopeTest {
    static int checks,externalFixtures;
    interface Action {void run() throws Exception;}
    static void check(boolean value){checks++;if(!value)throw new AssertionError("envelope check "+checks);}
    static void rejectsIO(Action action) throws Exception {try{action.run();throw new AssertionError("Expected codec rejection");}catch(IOException expected){checks++;}}
    static void rejectsBound(Action action) throws Exception {try{action.run();throw new AssertionError("Expected invalid bound");}catch(IllegalArgumentException expected){checks++;}}
    static String repeat(char c,int count){char[] v=new char[count];Arrays.fill(v,c);return new String(v);}
    static InputStream stream(String text){return new ByteArrayInputStream(text.getBytes(StandardCharsets.US_ASCII));}
    static String line(byte[] wire){return "TX "+UsbBridgeCodec.hex(wire,wire.length)+"\n";}
    static byte[] decodeCommand(InputStream in) throws Exception {
        String command=UsbBridgeCodec.readLine(in,NanoStartupGate.MAX_COMMAND_CHARS);
        if(!command.startsWith("TX "))throw new IOException("Not a TX command");
        return UsbBridgeCodec.unhex(command.substring(3),NanoStartupGate.MAX_TX_HEX_CHARS);
    }
    static NanoStartupGate beforeA1(){
        NanoStartupGate gate=new NanoStartupGate(NanoStartupGate.Mode.INITIALIZE);
        NanoStartupGateTest.identify(gate);
        NanoStartupGateTest.phase(gate,0xa0,8,9);
        NanoStartupGateTest.phase(gate,0x84,32,33);
        NanoStartupGateTest.phase(gate,0xa0,8,9);
        return gate;
    }
    static void acceptedA1(String ascii,byte[] expected) throws Exception {
        byte[] wire=decodeCommand(stream(ascii));if(expected!=null)check(Arrays.equals(wire,expected));
        NanoStartupGate gate=beforeA1();check(gate.allowed(wire));check(!gate.allowed(wire));check(!gate.channelReady());
        NanoStartupGateTest.rx(gate,0xa1,new byte[]{0});
        NanoStartupGateTest.phase(gate,0xa2,32,33);check(gate.handshakeComplete());check(gate.channelReady());
    }
    public static void main(String[] args) throws Exception {
        check(NanoStartupGate.MAX_WIRE_BYTES==332);check(NanoStartupGate.MAX_TX_HEX_CHARS==664);check(NanoStartupGate.MAX_COMMAND_CHARS==667);
        byte[] base=new byte[160],wire=NanoStartupGateTest.frame(0,0xa1,0,base);
        check(wire.length==167);check(line(wire).length()==338);
        // The legacy line failure seen in init.1 is reproduced. The legacy hex
        // bound would also reject A1 if that line reached the decoder.
        rejectsIO(()->UsbBridgeCodec.readLine(stream(line(wire))));
        rejectsIO(()->UsbBridgeCodec.unhex(UsbBridgeCodec.hex(wire,wire.length)));
        acceptedA1(line(wire),wire);
        // Exercise every byte value (including each VCX escape) in a legal-width A1.
        for(int value=0;value<256;value++){
            byte[] payload=new byte[160];Arrays.fill(payload,(byte)value);
            byte[] encoded=NanoStartupGateTest.frame(0,0xa1,0,payload);acceptedA1(line(encoded),encoded);
        }
        // All160 payload bytes AND the checksum escape; fixed header bytes never escape.
        byte[] escaped=new byte[160];Arrays.fill(escaped,0,66,(byte)0xbb);Arrays.fill(escaped,66,160,(byte)0xee);
        byte[] maximum=NanoStartupGateTest.frame(0,0xa1,0,escaped);
        check(maximum.length==328);check(line(maximum).length()==660);acceptedA1(line(maximum),maximum);
        rejectsIO(()->UsbBridgeCodec.readLine(stream(line(maximum))));
        rejectsIO(()->UsbBridgeCodec.unhex(UsbBridgeCodec.hex(maximum,maximum.length)));
        // Line and hex limits remain independent, with exact boundary acceptance.
        check(UsbBridgeCodec.readLine(stream(repeat('X',667)+"\n"),667).length()==667);
        rejectsIO(()->UsbBridgeCodec.readLine(stream(repeat('X',668)+"\n"),667));
        check(UsbBridgeCodec.unhex(repeat('A',664),664).length==332);
        rejectsIO(()->UsbBridgeCodec.unhex(repeat('A',666),664));
        check(UsbBridgeCodec.readLine(stream(repeat('X',260)+"\n")).length()==260);
        rejectsIO(()->UsbBridgeCodec.readLine(stream(repeat('X',261)+"\n")));
        check(UsbBridgeCodec.unhex(repeat('A',256)).length==128);
        rejectsIO(()->UsbBridgeCodec.unhex(repeat('A',258)));
        for(int bound:new int[]{0,-1,4097}){
            rejectsBound(()->UsbBridgeCodec.readLine(stream("READ\n"),bound));
            rejectsBound(()->UsbBridgeCodec.unhex("BB",bound));
        }
        for(String malformed:new String[]{"TX "+repeat('A',665)+"\n","TX AAaA\n","TX GG\n","TX A\n","TX \n","TX BB\r\n","TX BB\u0000\n","TX BB"})rejectsIO(()->decodeCommand(stream(malformed)));
        rejectsIO(()->decodeCommand(new ByteArrayInputStream(new byte[]{(byte)0xff,'\n'})));
        rejectsIO(()->decodeCommand(stream("OTHER "+UsbBridgeCodec.hex(wire,wire.length)+"\n")));
        // Valid hex can still be a wrong-sized, wrong-header, truncated or bad-checksum frame.
        for(byte[] bad:new byte[][]{NanoStartupGateTest.frame(0,0xa1,0,new byte[159]),NanoStartupGateTest.frame(0,0xa1,0,new byte[161]),NanoStartupGateTest.frame(1,0xa1,0,new byte[160]),NanoStartupGateTest.frame(0,0xa1,1,new byte[160]),new byte[]{(byte)0xbb,(byte)0x80,0,(byte)0xa1,0,(byte)0xdd,(byte)0xbb}}){
            check(!beforeA1().allowed(decodeCommand(stream(line(bad)))));
        }
        byte[] corrupt=wire.clone();corrupt[1]^=1;check(!beforeA1().allowed(decodeCommand(stream(line(corrupt)))));
        // Coalesced socket commands cannot lose the byte following the TX newline.
        InputStream coalesced=stream(line(maximum)+"READ\n");check(Arrays.equals(maximum,decodeCommand(coalesced)));check(UsbBridgeCodec.readLine(coalesced,667).equals("READ"));
        for(String path:args){
            byte[] actual=Files.readAllBytes(Paths.get(path));check(actual.length<=668);check(actual.length>0 && actual[actual.length-1]=='\n');
            for(byte b:actual)check(b=='\n' || (b>=32 && b<=126));
            String ascii=new String(actual,StandardCharsets.US_ASCII);check(ascii.indexOf('\n')==ascii.length()-1);acceptedA1(ascii,null);externalFixtures++;
        }
        System.out.println("NANO_BRIDGE_ENVELOPE PASS "+checks+" checks; production-codec limits, external Rust fixtures="+externalFixtures+"; synthetic only, no hardware");
    }
}
