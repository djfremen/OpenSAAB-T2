// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
/** Exact captured requests and one-use budgets at the physical USB boundary. */
public final class MongooseUsbGate {
 private static int word(byte[] b,int i){return (b[i]&255)|((b[i+1]&255)<<8);}
 private static byte[] hex(String s){byte[] b=new byte[s.length()/2];for(int n=0;n<b.length;n++)b[n]=(byte)Integer.parseInt(s.substring(n*2,n*2+2),16);return b;}
 private static boolean body(byte[] b,String s){return java.util.Arrays.equals(java.util.Arrays.copyOfRange(b,16,b.length),hex(s));}
 private static boolean sw(String mode){return "swcan".equals(mode)||"swcan_wake".equals(mode);}
 public static boolean allowed(byte[] b,boolean cancelled){return allowed(b,cancelled,"hs_channel");}
 public static boolean allowed(byte[] b,boolean cancelled,String mode){
  if(!(sw(mode)||"startup".equals(mode)||"hs_channel".equals(mode)||"vin".equals(mode)))return false;
  if(b.length<16||b.length>35||word(b,0)!=b.length-4||word(b,2)!=(word(b,0)^0x51e6))return false;
  int dst=word(b,4),op=word(b,8),seq=word(b,10);if(word(b,6)!=0||seq<1||seq>255)return false;
  // Exact captured transmit header, distinct from zero-word command headers.
  if(b[12]!=(op==8?1:0))return false;
  for(int i=13;i<16;i++)if(b[i]!=0)return false;
  if(cancelled)return (b.length==16&&((dst==1&&op==5)||(dst==(sw(mode)?0x1801:0x601)&&op==7)))||(sw(mode)&&dst==0x1801&&op==8&&body(b,"4000000000000000050000000000024120"));
  if(dst==1){
   if(op==0x100||op==0x109||op==0x111||op==0x103||op==3||op==5)return b.length==16;
   if(b.length!=20)return false;
   if(op==0x13)return word(b,16)==0&&word(b,18)==0;
   if(op==0xc)return word(b,18)==0&&(word(b,16)==0x2f||word(b,16)==0x2b||word(b,16)==0x2a);
  }
  if(dst==0x601&&!sw(mode)&&!"startup".equals(mode)){
   if(op==7)return b.length==16;
   if(op==6)return body(b,"0008000020a10700");
   if(op==0x12)return body(b,"01000000060000000e000000");
   if("vin".equals(mode)){
    if(op==0xd)return body(b,"0200000040000000ff000007e800000007e000");
    if(op==8)return body(b,"400000000000000006000000000007e01a90");
   }
  }
  if(sw(mode)&&(dst==0x1801||dst==0x1701)){
   if(op==6)return body(b,"0008000035820000");
   if(dst==0x1801){
    if(op==7)return b.length==16;
    if(op==0x12)return body(b,"010000000100000000000000");
    if(op==0xb)return body(b,"2d00000000000000");
    if(op==0xd)return body(b,"0200000040000000ff00000641000000024100");
    if(op==8)return body(b,"40000000000000000700000000000241aa0101")||body(b,"400000000000000006000000000002411a90")||body(b,"4000000000000000050000000000024120");
   }else{
    if(op==0xd)return body(b,"0000000000000104ffffffff00000541");
    if(op==8)return body(b,"00000000000000000700000000000101fe013e")||("swcan_wake".equals(mode)&&body(b,"00040000000000000400000000000100"));
   }
  }
  return false;
 }
 public static final class Run {
  private final String mode;private int next=1,transmits;private boolean status;private final java.util.HashSet<String> issued=new java.util.HashSet<>();
  public Run(String mode){this.mode=mode;}
  public boolean admit(byte[] b,boolean cancelled){
   if(!allowed(b,cancelled,mode)||word(b,10)!=next)return false;
   if(word(b,8)==8){
    String key=java.util.Arrays.toString(java.util.Arrays.copyOfRange(b,16,b.length));
    if(issued.contains(key)||transmits>=(sw(mode)?("swcan_wake".equals(mode)?5:4):1))return false;
    boolean stop=word(b,4)==0x1801&&body(b,"4000000000000000050000000000024120");
    if(stop&&!status)return false;if(cancelled&&!stop)return false;
    if(word(b,4)==0x1801&&body(b,"40000000000000000700000000000241aa0101"))status=true;
    issued.add(key);transmits++;
   }
   next=next==255?1:next+1;return true;
  }
  public int transmitAttempts(){return transmits;}
 }
 private MongooseUsbGate(){}
}
