using System;
using System.IO;
using System.Collections.Generic;
using System.Web.Script.Serialization;
class StatusTests {
 public sealed class Vector {public string name,hex,state,age,now,vehicle;public Dictionary<string,string> receipt;}
 static int Main(string[] args) {
  var cases=new JavaScriptSerializer().Deserialize<Vector[]>(File.ReadAllText(args[0]));
  foreach(var c in cases){
   byte[] data=null;if(c.hex!=null){data=new byte[c.hex.Length/2];for(int i=0;i<data.Length;i++)data[i]=Convert.ToByte(c.hex.Substring(i*2,2),16);}
   var value=SecurityStatus.Evaluate(data,c.receipt,DateTimeOffset.Parse(c.now),c.vehicle);
   if(value.State!=c.state||value.Age!=c.age){Console.Error.WriteLine("FAIL "+c.name+": "+value.Summary);return 1;}
  }Console.WriteLine("PASS C#: "+cases.Length+" shared security-status vectors");return 0;
 }
}
