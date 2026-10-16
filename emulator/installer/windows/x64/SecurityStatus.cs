// SPDX-License-Identifier: MPL-2.0
using System;
using System.IO;
using System.Text;
using System.Linq;
using System.Collections.Generic;
using System.Security.Cryptography;
using System.Globalization;
using System.Text.RegularExpressions;
sealed class SecurityStatus {
 public const int Size=714, Offset=0xfe0000;
 public const string Explanation="Fresh means written less than 3 hours ago; Stale means 3 hours or older. This is an age reminder, not a confirmed vehicle-access expiry. Follow the firmware if it requests security access again.";
 public string State, Age="Age unknown", Timestamp="Timestamp unavailable", Summary, Color, Details;
 static string Get(IDictionary<string,string> r,string k){return r.ContainsKey(k)?r[k]:"";}
 static string Hash(byte[] b){using(var h=SHA256.Create())return BitConverter.ToString(h.ComputeHash(b)).Replace("-","").ToLowerInvariant();}
 static bool All(byte[] b,int start,int count,byte v){return b.Skip(start).Take(count).All(c=>c==v);}
 public static string Analyze(byte[] b){
  if(b==null||b.Length<Size)return "UNAVAILABLE";
  int seed=b[0x30]*256+b[0x31];
  if(All(b,0x14,17,255)&&All(b,0x26,8,255)&&(seed==0||seed==65535))return "INIT_AUTH";
  var chars=b.Skip(0x14).Take(17).Where(c=>c!=0&&c!=255).ToArray();
  if(chars.Length>0&&chars.All(c=>"ABCDEFGHJKLMNPRSTUVWXYZ0123456789".IndexOf((char)c)>=0)){
   if(All(b,0x26,8,255))return "PRE_AUTH";if(!All(b,0x26,8,0))return "POST_AUTH";
  }return "INVALID";
 }
 static DateTimeOffset Time(string s){if(!Regex.IsMatch(s,@"(Z|[+-]\d{2}:\d{2})$"))throw new FormatException();return DateTimeOffset.Parse(s,CultureInfo.InvariantCulture);}
 public static SecurityStatus Evaluate(byte[] b,IDictionary<string,string> r,DateTimeOffset now,string vehicle){
  var x=new SecurityStatus();x.State=Analyze(b);r=r??new Dictionary<string,string>();
  byte[] vin=b!=null&&b.Length>=Size?b.Skip(0x14).Take(17).ToArray():new byte[0];
  string v=Encoding.ASCII.GetString(vin);bool valid=Regex.IsMatch(v,@"^[A-HJ-NPR-Z0-9]{17}$");
  bool mismatch=!String.IsNullOrEmpty(vehicle)&&x.State!="INIT_AUTH"&&(!valid||v!=vehicle);
  if(mismatch)x.State="UNAVAILABLE";
  string label="[N/A]",message="No readable security data yet.";x.Color="#bdcbd8";
  switch(x.State){
   case "INIT_AUTH":label="[INIT_AUTH]";message="Security data cleared. Collect fresh data from the vehicle.";x.Color="#9ed8ff";break;
   case "PRE_AUTH":label="[PRE-AUTH]";message="Vehicle data collected. Security response has not been written.";x.Color="#ffd77c";break;
   case "POST_AUTH":label="[POST-AUTH]";message="Processed security data is present. Continue in the firmware.";x.Color="#9fe0b5";break;
   case "INVALID":label="[INVALID]";message="Security data does not match a recognized state.";x.Color="#ffb59a";break;
  }
  if(mismatch)message="Security data belongs to another or unverified vehicle.";
  try{
   bool matches=Get(r,"schema")=="1"&&valid&&Get(r,"vehicle_hash")==Hash(vin)&&new[]{"processing","processed","imported","failed"}.Contains(Get(r,"stage"));
   if(matches){Time(Get(r,"started_utc"));foreach(var field in new[]{"pre_auth_utc","processed_utc","imported_utc","failed_utc"})if(r.ContainsKey(field))Time(Get(r,field));}
   if(matches&&x.State=="POST_AUTH"&&Get(r,"stage")=="imported"&&Get(r,"ssa_hash")==Hash(b)){
    Time(Get(r,"started_utc"));Time(Get(r,"processed_utc"));var age=(now-Time(Get(r,"imported_utc"))).TotalSeconds;
    if(age>=0){x.Age=age<10800?"Fresh":"Stale";x.Timestamp="Post-auth written · "+Get(r,"imported_utc");if(x.Age=="Stale")x.Color="#ffd77c";}
   }else if(matches&&x.State=="PRE_AUTH"&&new[]{"processing","processed","failed"}.Contains(Get(r,"stage"))&&Get(r,"pre_auth_utc")!=""){
    Time(Get(r,"pre_auth_utc"));x.Timestamp="Pre-auth collected · "+Get(r,"pre_auth_utc");
   }
  }catch(FormatException){}catch(ArgumentException){}
  x.Summary="Security: "+label+(x.State=="INIT_AUTH"?" · cleared":x.State=="POST_AUTH"?" · "+x.Age:"");
  x.Details=x.Summary+"\n"+message+"\n"+x.Timestamp+"\n\n"+Explanation+"\nThis is card data, not confirmation of current vehicle authorization.";return x;
 }
 public static SecurityStatus Read(string card,string receipt,string vehicle){
  byte[] b=null;var r=new Dictionary<string,string>();
  try{string checkpoint=Path.Combine(Path.GetDirectoryName(card),"working-ssa.bin"); if(File.Exists(checkpoint)){b=SessionSnapshot.Bytes(checkpoint);if(b.Length!=Size)b=null;}else using(var f=new FileStream(card,FileMode.Open,FileAccess.Read,FileShare.ReadWrite|FileShare.Delete)){f.Seek(Offset,SeekOrigin.Begin);b=new byte[Size];int n=0,k;while(n<Size&&(k=f.Read(b,n,Size-n))>0)n+=k;if(n!=Size)b=null;}}catch(IOException){}catch(ArgumentException){}catch(UnauthorizedAccessException){}
  try{if(new FileInfo(receipt).Length<=8192)foreach(var line in File.ReadAllLines(receipt)){if(line.StartsWith("#")||line.StartsWith("!"))continue;int n=line.IndexOf('=');if(n>=0)r[line.Substring(0,n)]=line.Substring(n+1).Replace(@"\:",":");}}catch(IOException){}catch(ArgumentException){}catch(UnauthorizedAccessException){}
  return Evaluate(b,r,DateTimeOffset.UtcNow,vehicle);
 }
}
