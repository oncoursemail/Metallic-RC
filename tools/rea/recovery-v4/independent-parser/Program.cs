using System.Security.Cryptography;
using System.Text.Json;
using Il2CppDumper;

if (args.Length != 2) throw new ArgumentException("Usage: VerifyMetadata input.dat new-output-directory");
byte[] original = File.ReadAllBytes(args[0]);
string originalHash = Convert.ToHexStringLower(SHA256.HashData(original));
string output = Path.GetFullPath(args[1]);
if (Directory.Exists(output)) throw new IOException("Output already exists");
Directory.CreateDirectory(output);
// The upstream parser constructs a BinaryWriter, so give it a private writable
// memory copy. Never open the evidence file for writing.
using var memory = new MemoryStream((byte[])original.Clone(), writable: true);
using var metadata = new Metadata(memory);
if (metadata.Version != 31) throw new InvalidDataException("Unexpected metadata version");
int[] owners = Enumerable.Repeat(-1, metadata.typeDefs.Length).ToArray();
for (int i=0; i<metadata.imageDefs.Length; i++) {
    var img=metadata.imageDefs[i];
    for (int t=img.typeStart; t<img.typeStart+img.typeCount; t++) {
        if (owners[t]!=-1) throw new InvalidDataException("Type belongs to multiple images");
        owners[t]=i;
    }
}
if (owners.Any(x=>x<0)) throw new InvalidDataException("Unowned types");
long stringLookups=0;
string ReadName(uint index) { stringLookups++; return metadata.GetStringFromIndex(index); }
string[] imageNames=metadata.imageDefs.Select(x=>ReadName(x.nameIndex)).ToArray();
string[] typeNames=metadata.typeDefs.Select(x=>ReadName(x.nameIndex)).ToArray();
string[] namespaces=metadata.typeDefs.Select(x=>ReadName(x.namespaceIndex)).ToArray();
int modulePlaceholders=0;
using(var writer=new StreamWriter(Path.Combine(output,"types.jsonl"),false,new System.Text.UTF8Encoding(false))) {
    for(int i=0;i<metadata.typeDefs.Length;i++) {
        var t=metadata.typeDefs[i];
        if(t.genericContainerIndex < -1 || t.genericContainerIndex>=metadata.genericContainers.Length) throw new InvalidDataException("Generic index out of range");
        if(typeNames[i]=="<Module>") modulePlaceholders++;
        writer.WriteLine(JsonSerializer.Serialize(new {typeIndex=i,imageIndex=owners[i],assembly=imageNames[owners[i]],name=typeNames[i],@namespace=namespaces[i],token=t.token,flags=t.flags,byvalNativeTypeIndex=t.byvalTypeIndex,fieldStart=t.fieldStart,fieldCount=t.field_count,methodStart=t.methodStart,methodCount=t.method_count}));
    }
}
using(var writer=new StreamWriter(Path.Combine(output,"methods.jsonl"),false,new System.Text.UTF8Encoding(false))) {
    for(int i=0;i<metadata.methodDefs.Length;i++) {
        var m=metadata.methodDefs[i];
        if(m.declaringType<0 || m.declaringType>=owners.Length) throw new InvalidDataException("Method owner invalid");
        string name=ReadName(m.nameIndex);
        for(int p=0;p<m.parameterCount;p++) ReadName(metadata.parameterDefs[m.parameterStart+p].nameIndex);
        writer.WriteLine(JsonSerializer.Serialize(new {methodIndex=i,imageIndex=owners[m.declaringType],assembly=imageNames[owners[m.declaringType]],declaringTypeIndex=m.declaringType,typeName=typeNames[m.declaringType],@namespace=namespaces[m.declaringType],name,token=m.token,returnNativeTypeIndex=m.returnType,parameterStart=m.parameterStart,parameterCount=m.parameterCount,flags=m.flags,slot=m.slot}));
    }
}
foreach(var f in metadata.fieldDefs) ReadName(f.nameIndex);
foreach(var p in metadata.propertyDefs) ReadName(p.nameIndex);
foreach(var e in metadata.eventDefs) ReadName(e.nameIndex);
foreach(var g in metadata.genericParameters) ReadName(g.nameIndex);
foreach(var a in metadata.assemblyDefs) ReadName(a.aname.nameIndex);
if(!memory.ToArray().AsSpan().SequenceEqual(original)) throw new InvalidDataException("Upstream parser unexpectedly changed its input memory");
var result=new {status="independent_upstream_metadata_parse_passed",upstream="Perfare/Il2CppDumper",upstreamCommit="4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a",metadataVersion=metadata.Version,inputSha256=originalHash,sourceFilesModified=false,inputMemoryUnchanged=true,counts=new {images=metadata.imageDefs.Length,assemblies=metadata.assemblyDefs.Length,types=metadata.typeDefs.Length,methods=metadata.methodDefs.Length,fields=metadata.fieldDefs.Length,parameters=metadata.parameterDefs.Length,properties=metadata.propertyDefs.Length,events=metadata.eventDefs.Length,genericContainers=metadata.genericContainers.Length,genericParameters=metadata.genericParameters.Length,stringLiterals=metadata.stringLiterals.Length,attributeRanges=metadata.attributeDataRanges.Length,modulePlaceholders},identifierLookups=stringLookups,nativeMethodBodiesRecovered=false,originalSourceRecovered=false,limits=new[]{"The independent upstream metadata parser validates parseability; this is not a native registration or original-source recovery result.","JSON indices describe metadata, not rebuilt game assemblies."}};
string json=JsonSerializer.Serialize(result,new JsonSerializerOptions {WriteIndented=true});
File.WriteAllText(Path.Combine(output,"independent-parser-result.json"),json+"\n");
Console.WriteLine(json);
