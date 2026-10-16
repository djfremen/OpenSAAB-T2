// SPDX-License-Identifier: MPL-2.0
// Portable coordinator fixtures, explicitly excluding WinRT, WinForms and physical I/O.
// Windows --self-test runs these same production sources with Framework's real serializer.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

sealed class Pump : SynchronizationContext
{
    readonly BlockingCollection<Action> queue=new BlockingCollection<Action>();
    public override void Post(SendOrPostCallback callback,object state) {queue.Add(()=>callback(state));}
    public void Run(Task task) {while(!task.IsCompleted){Action action;if(queue.TryTake(out action,100))action();}task.GetAwaiter().GetResult();}
}
static class Program
{
    static int Main(string[] args)
    {
        if(args.Length!=2)throw new ArgumentException("Pass shared worker binary and isolated test directory");
        var ui=new Pump();SynchronizationContext.SetSynchronizationContext(ui);
        Task<int> test=BluetoothTests.RunAsync(args[0],args[1]);ui.Run(test);
        Console.WriteLine("PASS: "+test.Result+" portable C# coordinator/profile checks using the actual Rust worker. Native Windows radio APIs are not exercised.");return 0;
    }
}
sealed class WindowsBluetoothLink : IWindowsBluetoothLink
{
    public WindowsBluetoothLink(Func<string,bool> allowed) {throw new InvalidOperationException("Portable fixtures cannot open a Windows radio");}
    public void Open(Action<Dictionary<string,object>> callback) {throw new InvalidOperationException();}
    public void Send(Dictionary<string,object> value) {throw new InvalidOperationException();}
}
static class EmulatorSession {public static string Quote(string s) {if(s.Contains("\"")||s.Contains("\n"))throw new ArgumentException("Invalid path");return "\""+s+"\"";}}
namespace System.Web.Script.Serialization
{
    // JSON compatibility adapter used only by this portable harness.
    public sealed class JavaScriptSerializer
    {
        public int MaxJsonLength {get;set;} public int RecursionLimit {get;set;}
        public string Serialize(object value) {return JsonSerializer.Serialize(value);}
        public T Deserialize<T>(string text)
        {
            if(text.Length>MaxJsonLength)throw new InvalidDataException("JSON too large");
            using(var document=JsonDocument.Parse(text,new JsonDocumentOptions{MaxDepth=RecursionLimit}))return (T)Value(document.RootElement);
        }
        static object Value(JsonElement element)
        {
            switch(element.ValueKind)
            {
                case JsonValueKind.Object:var dictionary=new Dictionary<string,object>();foreach(var p in element.EnumerateObject())dictionary[p.Name]=Value(p.Value);return dictionary;
                case JsonValueKind.Array:var list=new List<object>();foreach(var v in element.EnumerateArray())list.Add(Value(v));return list.ToArray();
                case JsonValueKind.String:return element.GetString();
                case JsonValueKind.Number:int number;return element.TryGetInt32(out number)?(object)number:element.GetDouble();
                case JsonValueKind.True:return true;case JsonValueKind.False:return false;default:return null;
            }
        }
    }
}
