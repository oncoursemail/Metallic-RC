using System.Text.Json;
using Metallic.Reconstruction;
if(args.Length is <2 or >3) throw new ArgumentException("BindingProbe input.jsonl NEW-output.jsonl [sorted-mutant]");
using var output=new StreamWriter(new FileStream(args[1],FileMode.CreateNew));int count=0;
foreach(string line in File.ReadLines(args[0])) {
    using var j=JsonDocument.Parse(line);var input=j.RootElement;
    var values=input.GetProperty("values").EnumerateArray().Select(x=>x.GetString()).ToArray();
    if(values.Length!=31 || ++count>10000) throw new InvalidDataException("Fixture bounds");
    int id=input.GetProperty("id").GetInt32(),failAt=input.GetProperty("failAt").GetInt32();
    var routes=new ObservedServiceRoutes {
        AI=values[0],
        Auth=values[1],
        API=values[2],
        Commerce=values[3],
        Matchmaking=values[4],
        Notifications=values[5],
        Images=values[6],
        CDN=values[7],
        Storage=values[8],
        Chat=values[9],
        Leaderboard=values[10],
        Accounts=values[11],
        Link=values[12],
        Lists=values[13],
        RoomComments=values[14],
        Clubs=values[15],
        Rooms=values[16],
        PlatformNotifications=values[17],
        Moderation=values[18],
        DataCollection=values[19],
        BugReporting=values[20],
        Discovery=values[21],
        PlayerSettings=values[22],
        Studio=values[23],
        GameLogs=values[24],
        Strings=values[25],
        StringsCDN=values[26],
        WWW=values[27],
        Econ=values[28],
        Data=values[29],
        Cards=values[30],
    };
    var trace=new List<object>();string status="completed";
    void Update(int service,string? value) {
        int index=trace.Count;trace.Add(new {serviceId=service,value});
        if(index==failAt) throw new DeliberateCallbackFailure();
    }
    try {
        if(args.Length==3 && args[2]=="sorted-mutant") {
            // Deliberate wrong reconstruction used to test whether order matters.
            for(int service=0;service<31;service++) Update(service,values[service]);
        } else ObservedServiceBindingSequence.Apply(routes,Update);
    } catch(DeliberateCallbackFailure) {status="callback_stopped";}
    output.WriteLine(JsonSerializer.Serialize(new {id,status,trace}));
}
Console.WriteLine(JsonSerializer.Serialize(new {status="completed",cases=count,gameLoaded=false}));
sealed class DeliberateCallbackFailure:Exception {}
