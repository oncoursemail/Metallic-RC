// Behavioral reconstruction for Steam build 23191908. NOT original source.
// Generated formatter control flow was checked against captured instructions.
// System.Text.Json is our bounded JSON adapter, not the original Utf8Json runtime.
using System.Text;
using System.Text.Json;
using System.Text.Encodings.Web;
namespace Metallic.RC.Compatibility;

public sealed class DiscoveryResponse
{
    public string?[] Values { get; } = new string?[32];
    public string? RecNetStatus { get => Values[0]; set => Values[0] = value; }
}

public static class DiscoveryContract
{
    private static readonly string[] Canonical = ["RecNetStatus",
        "AI",
        "Auth",
        "API",
        "WWW",
        "Notifications",
        "Images",
        "CDN",
        "Commerce",
        "Matchmaking",
        "Storage",
        "Chat",
        "Leaderboard",
        "Accounts",
        "Link",
        "Lists",
        "RoomComments",
        "Clubs",
        "Rooms",
        "PlatformNotifications",
        "Moderation",
        "DataCollection",
        "BugReporting",
        "Discovery",
        "PlayerSettings",
        "Studio",
        "GameLogs",
        "Strings",
        "Econ",
        "StringsCDN",
        "Data",
        "Cards"];
    public const string SteamBuildId = "23191908";
    public static IReadOnlyList<string> CanonicalNames { get; } = Array.AsReadOnly(Canonical);
    private static readonly Dictionary<string,int> Aliases = new(StringComparer.Ordinal)
    {
        ["RecNetStatus"] = 0,
        ["recNetStatus"] = 0,
        ["recnetstatus"] = 0,
        ["AI"] = 1,
        ["aI"] = 1,
        ["ai"] = 1,
        ["Auth"] = 2,
        ["auth"] = 2,
        ["API"] = 3,
        ["aPI"] = 3,
        ["api"] = 3,
        ["WWW"] = 4,
        ["wWW"] = 4,
        ["www"] = 4,
        ["Notifications"] = 5,
        ["notifications"] = 5,
        ["Images"] = 6,
        ["images"] = 6,
        ["CDN"] = 7,
        ["cDN"] = 7,
        ["cdn"] = 7,
        ["Commerce"] = 8,
        ["commerce"] = 8,
        ["Matchmaking"] = 9,
        ["matchmaking"] = 9,
        ["Storage"] = 10,
        ["storage"] = 10,
        ["Chat"] = 11,
        ["chat"] = 11,
        ["Leaderboard"] = 12,
        ["leaderboard"] = 12,
        ["Accounts"] = 13,
        ["accounts"] = 13,
        ["Link"] = 14,
        ["link"] = 14,
        ["Lists"] = 15,
        ["lists"] = 15,
        ["RoomComments"] = 16,
        ["roomComments"] = 16,
        ["roomcomments"] = 16,
        ["Clubs"] = 17,
        ["clubs"] = 17,
        ["Rooms"] = 18,
        ["rooms"] = 18,
        ["PlatformNotifications"] = 19,
        ["platformNotifications"] = 19,
        ["platformnotifications"] = 19,
        ["Moderation"] = 20,
        ["moderation"] = 20,
        ["DataCollection"] = 21,
        ["dataCollection"] = 21,
        ["datacollection"] = 21,
        ["BugReporting"] = 22,
        ["bugReporting"] = 22,
        ["bugreporting"] = 22,
        ["Discovery"] = 23,
        ["discovery"] = 23,
        ["PlayerSettings"] = 24,
        ["playerSettings"] = 24,
        ["playersettings"] = 24,
        ["Studio"] = 25,
        ["studio"] = 25,
        ["GameLogs"] = 26,
        ["gameLogs"] = 26,
        ["gamelogs"] = 26,
        ["Strings"] = 27,
        ["strings"] = 27,
        ["Econ"] = 28,
        ["econ"] = 28,
        ["StringsCDN"] = 29,
        ["stringsCDN"] = 29,
        ["stringscdn"] = 29,
        ["Data"] = 30,
        ["data"] = 30,
        ["Cards"] = 31,
        ["cards"] = 31,
    };
    // Captured dispatch order is not the response-field order or numeric sorting.
    private static readonly int[] Order = [0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,27,25,26,28,29,30];
    private static readonly string[] Names = ["AI","Auth","API","Commerce","Matchmaking","Notifications","Images","CDN","Storage","Chat","Leaderboard","Accounts","Link","Lists","RoomComments","Clubs","Rooms","PlatformNotifications","Moderation","DataCollection","BugReporting","Discovery","PlayerSettings","Studio","GameLogs","Strings","StringsCDN","WWW","Econ","Data","Cards"];
    public static IReadOnlyList<int> ServiceOrder { get; } = Array.AsReadOnly(Order);
    public static IReadOnlyList<string> ServiceNames { get; } = Array.AsReadOnly(Names);
    public static DiscoveryResponse? Read(ReadOnlySpan<byte> utf8)
    {
        if(utf8.Length>1_048_576) throw new JsonException("Fixture exceeds the declared 1 MiB boundary.");
        var reader=new Utf8JsonReader(utf8,new JsonReaderOptions {MaxDepth=64,CommentHandling=JsonCommentHandling.Disallow,AllowTrailingCommas=false});
        if(!reader.Read()) throw new JsonException("Missing JSON value.");
        if(reader.TokenType==JsonTokenType.Null) {if(reader.Read())throw new JsonException("Trailing data.");return null;}
        if(reader.TokenType!=JsonTokenType.StartObject) throw new JsonException("Expected object or null.");
        if(!reader.Read())throw new JsonException("Unterminated object.");
        if(reader.TokenType==JsonTokenType.EndObject){if(reader.Read())throw new JsonException("Trailing data.");return null;}
        var result=new DiscoveryResponse();
        while(reader.TokenType!=JsonTokenType.EndObject){
            if(reader.TokenType!=JsonTokenType.PropertyName)throw new JsonException("Expected property name.");
            // Exact raw ASCII aliases are the admitted contract profile. Escaped
            // property names are not silently normalized into a recognized key.
            string name=Encoding.UTF8.GetString(reader.ValueSpan);
            bool known=Aliases.TryGetValue(name,out int index);
            if(!reader.Read())throw new JsonException("Missing property value.");
            if(known){
                if(reader.TokenType is not (JsonTokenType.String or JsonTokenType.Null)) throw new KnownValueTypeException(name);
                string? value=reader.TokenType==JsonTokenType.Null?null:reader.GetString();
                result.Values[index]=value;
            }else reader.Skip();
            if(!reader.Read())throw new JsonException("Unterminated object.");
        }
        if(reader.Read())throw new JsonException("Trailing data.");
        return result;
    }
    public static byte[] Write(DiscoveryResponse? value)
    {
        using var stream=new MemoryStream();using(var writer=new Utf8JsonWriter(stream,new JsonWriterOptions{Encoder=JavaScriptEncoder.UnsafeRelaxedJsonEscaping})){
            if(value is null)writer.WriteNullValue();
            else {writer.WriteStartObject();for(int i=0;i<32;i++)writer.WriteString(CanonicalNames[i],value.Values[i]);writer.WriteEndObject();}
        }return stream.ToArray();
    }
    public static string CacheKey(string? partition)=>string.Concat("CachedNameServerResponse_",partition);
    public static void ApplyResponse(DiscoveryResponse? response,Dictionary<int,Uri> services,bool cacheWasLoaded,Action<DiscoveryResponse> saveCache,Action invalidateCache)
    {
        // This models the proven synchronous response-application stage only.
        // Original transport/async state machines are not replaced by this code.
        if(response is null)throw new DiscoveryResponseMissingException();
        if(!string.IsNullOrEmpty(response.RecNetStatus))throw new DiscoveryStatusException(response.RecNetStatus);
        var context=new CachedResponseContext{CachedNameServerResponseLoaded=cacheWasLoaded};
        foreach(int service in ServiceOrder){
            int field=Array.IndexOf(Canonical,ServiceNames[service]);
            ObservedServiceAddressUpdate.Apply(services,service,response.Values[field],ref context);
        }
        if(!cacheWasLoaded){
            var stored=new DiscoveryResponse();foreach(int service in ServiceOrder)stored.Values[Array.IndexOf(Canonical,ServiceNames[service])]=services[service].OriginalString;
            saveCache(stored);
        }
        if(context.CachedNameServerResponseChanged){invalidateCache();throw new DiscoveryChangedException();}
    }
}
public sealed class KnownValueTypeException(string property):JsonException("Expected string or null for "+property);
public sealed class DiscoveryResponseMissingException():Exception("Missing discovery response (reconstructed diagnostic).");
public sealed class DiscoveryStatusException(string status):Exception(status);
public sealed class DiscoveryChangedException():Exception("Service addresses changed; reconnect is required (reconstructed diagnostic).");
