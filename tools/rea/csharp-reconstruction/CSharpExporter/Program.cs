using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ICSharpCode.Decompiler;
using ICSharpCode.Decompiler.CSharp;
using ICSharpCode.Decompiler.CSharp.Transforms;

if(args.Length is not (3 or 4))throw new ArgumentException("CSharpExporter partial-assembly-directory emitted-methods.jsonl NEW-output-directory [single-assembly]");
string input=Path.GetFullPath(args[0]), output=Path.GetFullPath(args[2]);
if(Directory.Exists(output))throw new IOException("Existing output is never overwritten");
Directory.CreateDirectory(output);
var recovered=new Dictionary<string,int>();
foreach(string line in File.ReadLines(args[1])){using var json=JsonDocument.Parse(line);string name=json.RootElement.GetProperty("assembly").GetString();recovered[name]=recovered.GetValueOrDefault(name)+1;}
var paths=Directory.GetFiles(input).Where(p=>Path.GetFileName(p)!="Il2CppDummyDll.dll").OrderByDescending(p=>recovered.GetValueOrDefault(Path.GetFileName(p))).ThenBy(p=>p,StringComparer.Ordinal).ToArray();
if(paths.Length!=419)throw new InvalidDataException("Expected exactly 419 recovered game/framework images");
if(args.Length==4){paths=paths.Where(p=>Path.GetFileName(p)==args[3]).ToArray();if(paths.Length!=1)throw new InvalidDataException("Requested assembly not found");}
int expectedAssemblies=paths.Length;
var results=new List<object>();int successes=0,failures=0;long bytesWritten=0;var total=Stopwatch.StartNew();
void Save(){File.WriteAllText(Path.Combine(output,"export-status.json"),JsonSerializer.Serialize(new{status=successes+failures==expectedAssemblies?"finished":"exporting",totalAssemblies=expectedAssemblies,successes,failures,bytesWritten,elapsedSeconds=total.Elapsed.TotalSeconds,methodBodiesRecoveredBeforeExport=recovered.Values.Sum(),gameAssembliesExecuted=false,fullGameRebuild=false,results},new JsonSerializerOptions{WriteIndented=true})+"\n");}
foreach(string path in paths){
    string name=Path.GetFileName(path);var elapsed=Stopwatch.StartNew();
    try{
        using var cancellation=new CancellationTokenSource(TimeSpan.FromSeconds(name=="Assembly-CSharp.dll"?450:90));
        var settings=new DecompilerSettings{ThrowOnAssemblyResolveErrors=false};
        var decompiler=new CSharpDecompiler(path,settings){CancellationToken=cancellation.Token};
        decompiler.AstTransforms.Add(new EscapeInvalidIdentifiers());
        string source=decompiler.DecompileWholeModuleAsString();
        string file=name+".partial.cs";
        string header=$"// METALLIC-RC ANALYSIS RECONSTRUCTION — {name}\n// {recovered.GetValueOrDefault(name)} manifest-listed simple leaf bodies were reconstructed.\n// All other concrete bodies explicitly throw NotSupportedException.\n// Abstract/external methods remain bodyless. This is NOT a runnable replacement game.\n// Generated C# is not original developer source; compilation has not been established.\n// Consult emitted-leaf-methods.jsonl for exact recovered methods and native-byte provenance.\n\n";
        byte[] content=Encoding.UTF8.GetBytes(header+source);File.WriteAllBytes(Path.Combine(output,file),content);bytesWritten+=content.Length;successes++;
        results.Add(new{assembly=name,file,bytes=content.Length,sha256=Convert.ToHexStringLower(SHA256.HashData(content)),recoveredLeafBodies=recovered.GetValueOrDefault(name),status="written",seconds=elapsed.Elapsed.TotalSeconds});
        Console.WriteLine(JsonSerializer.Serialize(new{assembly=name,status="written",successes,bytes=content.Length,seconds=elapsed.Elapsed.TotalSeconds}));
    }catch(Exception error){failures++;results.Add(new{assembly=name,status="failed",error=error.ToString(),seconds=elapsed.Elapsed.TotalSeconds});Console.WriteLine(JsonSerializer.Serialize(new{assembly=name,status="failed",error=error.Message}));}
    Save();
    GC.Collect();GC.WaitForPendingFinalizers();
}
Save();
Console.WriteLine(JsonSerializer.Serialize(new{status="finished",successes,failures,bytesWritten,seconds=total.Elapsed.TotalSeconds}));
return failures==0?0:1;
