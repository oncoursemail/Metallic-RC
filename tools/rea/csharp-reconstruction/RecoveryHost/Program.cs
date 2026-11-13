using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Il2CppDumper;
using Mono.Cecil;
using Mono.Cecil.Cil;

internal static class RecoveryHost
{
    static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };
    static string Hash(byte[] bytes) => Convert.ToHexStringLower(SHA256.HashData(bytes));
    public static int Main(string[] args)
    {
        if (args.Length != 5) { Console.Error.WriteLine("RecoveryHost image metadata registration-report NEW-output-directory capture-report"); return 2; }
        string output = Path.GetFullPath(args[3]);
        if (Directory.Exists(output)) throw new IOException("Choose a new output directory; existing evidence is never overwritten.");
        Directory.CreateDirectory(output);
        string status = "initializing";
        void Record(string stage) { status=stage; File.WriteAllText(Path.Combine(output,"operation.json"), JsonSerializer.Serialize(new {status,observedUtc=DateTimeOffset.UtcNow,targetExecuted=false},JsonOptions)); Console.WriteLine(stage); }
        try
        {
            byte[] binary=File.ReadAllBytes(args[0]), metadataBytes=File.ReadAllBytes(args[1]);
            using var report=JsonDocument.Parse(File.ReadAllText(args[2]));
            if(Hash(binary)!=report.RootElement.GetProperty("analysisImageSha256").GetString() || Hash(metadataBytes)!=report.RootElement.GetProperty("metadataSha256").GetString()) throw new InvalidDataException("Pinned artifact hash mismatch");
            ulong code=Convert.ToUInt64(report.RootElement.GetProperty("canonicalCodeRegistration").GetString()[2..],16);
            ulong meta=Convert.ToUInt64(report.RootElement.GetProperty("metadataRegistration").GetString()[2..],16);
            using var metadataStream=new MemoryStream(metadataBytes,true);
            using var metadata=new Metadata(metadataStream);
            using var binaryStream=new MemoryStream(binary,true);
            using var native=new PE(binaryStream);
            native.SetProperties(31,metadata.metadataUsagesCount);
            Record("reading_explicit_validated_native_registrations");
            // No auto-search and no PELoader/LoadLibrary path: parse bytes only.
            native.Init(code,meta);
            if(native.types.Length!=201385 || native.codeGenModules.Count!=419 || native.methodSpecs.Length!=1275652) throw new InvalidDataException("Native registration count disagreement");
            // Some module method-pointer arrays live in the runtime-only BSS tail.
            // The upstream PE reader otherwise maps those RVAs into unrelated disk
            // bytes. Replace pointer TABLE VALUES only with the independently hashed
            // mapped capture; never graft runtime type/class pointers into static data.
            using var captured=JsonDocument.Parse(File.ReadAllText(args[4]));
            var runtimeRecord=captured.RootElement.GetProperty("artifacts").EnumerateArray().Single(x=>x.GetProperty("kind").GetString()=="mapped_binary_evidence");
            if(!runtimeRecord.GetProperty("coverage").GetProperty("complete").GetBoolean())throw new InvalidDataException("Incomplete runtime evidence");
            string runtimeName=runtimeRecord.GetProperty("file").GetString();
            if(runtimeName!=Path.GetFileName(runtimeName))throw new InvalidDataException("Unexpected capture filename");
            byte[] runtime=File.ReadAllBytes(Path.Combine(Path.GetDirectoryName(Path.GetFullPath(args[4])),runtimeName));
            if(Hash(runtime)!="8f69ea448097cfffb3e3a4e61eb92981db99c14c3e1fbaba572afbaf8a63a732")throw new InvalidDataException("Runtime hash mismatch");
            ulong runtimeBase=runtimeRecord.GetProperty("module").GetProperty("base").GetUInt64();
            var codeRanges=captured.RootElement.GetProperty("gameAssemblyDiskLayout").GetProperty("sections").EnumerateArray().Where(x=>x.GetProperty("name").GetString() is ".text" or "il2cpp").Select(x=>(Start:x.GetProperty("rva").GetUInt64(),End:x.GetProperty("rva").GetUInt64()+x.GetProperty("virtualSize").GetUInt64())).ToArray();
            var pointerCorrections=new List<object>();
            foreach(var kv in native.codeGenModules) {
                var module=kv.Value;var table=new ulong[module.methodPointerCount];
                if(module.methodPointerCount>0) {
                    ulong start=checked(module.methodPointers-native.ImageBase);
                    if(start+checked((ulong)module.methodPointerCount)*8>(ulong)runtime.Length)throw new InvalidDataException("Module table outside complete capture");
                    for(ulong i=0;i<checked((ulong)module.methodPointerCount);i++) {
                        ulong value=BitConverter.ToUInt64(runtime,checked((int)(start+i*8)));
                        if(value!=0 && (value<runtimeBase || !codeRanges.Any(r=>r.Start<=value-runtimeBase && value-runtimeBase<r.End)))throw new InvalidDataException("Runtime method pointer outside code");
                        table[i]=value==0?0:native.ImageBase+value-runtimeBase;
                        ulong prior=native.codeGenModuleMethodPointers[kv.Key][i];
                        if(prior!=table[i])pointerCorrections.Add(new {assembly=kv.Key,row=i+1,priorStaticRead="0x"+prior.ToString("x"),verifiedRuntimeValue="0x"+table[i].ToString("x"),reason="use captured pointer table, not virtual-only BSS mapped onto later disk section"});
                    }
                }
                native.codeGenModuleMethodPointers[kv.Key]=table;
            }
            File.WriteAllText(Path.Combine(output,"runtime-pointer-corrections.json"),JsonSerializer.Serialize(new {sourceSha256=Hash(runtime),correctionCount=pointerCorrections.Count,corrections=pointerCorrections},JsonOptions));
            var executor=new Il2CppExecutor(metadata,native);
            int[] owner=Enumerable.Repeat(-1,metadata.typeDefs.Length).ToArray();
            string[] imageNames=metadata.imageDefs.Select(x=>metadata.GetStringFromIndex(x.nameIndex)).ToArray();
            for(int i=0;i<metadata.imageDefs.Length;i++) {
                var img=metadata.imageDefs[i];
                for(int t=img.typeStart;t<img.typeStart+img.typeCount;t++) { if(owner[t]!=-1) throw new InvalidDataException("Image overlap");owner[t]=i; }
            }
            Record("rendering_all_native_types_and_managed_signatures");
            string TypeName(int index) { if(index<0 || index>=native.types.Length)throw new InvalidDataException("Native type index");return executor.GetTypeName(native.types[index],true,false); }
            using(var writer=new StreamWriter(Path.Combine(output,"native-types.jsonl"),false,new UTF8Encoding(false))) {
                for(int i=0;i<native.types.Length;i++) {var t=native.types[i];writer.WriteLine(JsonSerializer.Serialize(new {nativeTypeIndex=i,name=TypeName(i),kind=t.type.ToString(),t.attrs,t.byref,t.valuetype,data=t.datapoint}));}
            }
            long parameters=0, nonNull=0, fields=0;
            using(var writer=new StreamWriter(Path.Combine(output,"method-signatures.jsonl"),false,new UTF8Encoding(false))) {
                for(int i=0;i<metadata.methodDefs.Length;i++) {
                    var m=metadata.methodDefs[i];string image=imageNames[owner[m.declaringType]];
                    var ps=Enumerable.Range(0,m.parameterCount).Select(j=> {var p=metadata.parameterDefs[m.parameterStart+j];parameters++;return new {name=metadata.GetStringFromIndex(p.nameIndex),type=TypeName(p.typeIndex),nativeTypeIndex=p.typeIndex,attributes=native.types[p.typeIndex].attrs,byref=native.types[p.typeIndex].byref,token=p.token};}).ToArray();
                    ulong pointer=native.GetMethodPointer(image,m);if(pointer!=0)nonNull++;
                    writer.WriteLine(JsonSerializer.Serialize(new {methodIndex=i,assembly=image,declaringTypeIndex=m.declaringType,declaringType=TypeName(metadata.typeDefs[m.declaringType].byvalTypeIndex),name=metadata.GetStringFromIndex(m.nameIndex),returnType=TypeName(m.returnType),returnByref=native.types[m.returnType].byref,parameters=ps,token=m.token,flags=m.flags,iflags=m.iflags,slot=m.slot,nativeRva=pointer==0?null:"0x"+native.GetRVA(pointer).ToString("x"),bodyStatus="not_reconstructed"}));
                }
            }
            using(var writer=new StreamWriter(Path.Combine(output,"field-layouts.jsonl"),false,new UTF8Encoding(false))) {
                for(int ti=0;ti<metadata.typeDefs.Length;ti++) {
                    var t=metadata.typeDefs[ti];
                    for(int j=0;j<t.field_count;j++) {
                        int fi=t.fieldStart+j;var f=metadata.fieldDefs[fi];var nt=native.types[f.typeIndex];bool isStatic=(nt.attrs&0x10)!=0;
                        int offset=native.GetFieldOffsetFromIndex(ti,j,fi,t.IsValueType,isStatic);fields++;
                        writer.WriteLine(JsonSerializer.Serialize(new {fieldIndex=fi,declaringTypeIndex=ti,assembly=imageNames[owner[ti]],declaringType=TypeName(t.byvalTypeIndex),name=metadata.GetStringFromIndex(f.nameIndex),type=TypeName(f.typeIndex),nativeTypeIndex=f.typeIndex,attributes=nt.attrs,isStatic,declaringTypeIsValueType=t.IsValueType,offset,offsetStatus=offset>=0?"concrete":"sentinel_or_unavailable",token=f.token}));
                    }
                }
            }
            if(parameters!=373383 || nonNull!=367865 || fields!=252928) throw new InvalidDataException("Signature/field cross-count mismatch");
            Record("writing_reference_only_csharp_declarations");
            var config=new Config {DumpMethod=true,DumpField=true,DumpProperty=true,DumpAttribute=false,DumpFieldOffset=true,DumpMethodOffset=true,DumpTypeDefIndex=true,GenerateDummyDll=false,GenerateStruct=false,RequireAnyKey=false};
            new Il2CppDecompiler(executor).Decompile(config,output+Path.DirectorySeparatorChar);
            string dump=Path.Combine(output,"dump.cs");
            string header="// REFERENCE DECLARATIONS ONLY. Empty bodies below are placeholders, not recovered behavior.\n// Do not compile or execute as a replacement game. See reconstruction-status.json.\n";
            string reference=Path.Combine(output,"CSharp.Declarations.reference.cs");
            using(var dest=File.Create(reference)) {byte[] h=Encoding.UTF8.GetBytes(header);dest.Write(h);using var input=File.OpenRead(dump);input.CopyTo(dest);}
            File.Delete(dump); // This is our new intermediate only.
            Record("generating_managed_reference_assemblies");
            var generator=new DummyAssemblyGenerator(executor,true);
            var referenceTypes=generator.Assemblies.Single(a=>a.Name.Name=="Il2CppDummyDll").MainModule.TypeSystem;
            var dllDir=Path.Combine(output,"ReferenceAssemblies.DO_NOT_RUN");Directory.CreateDirectory(dllDir);
            long stubbed=0, bodyless=0, actualMethods=0, actualTypes=0, actualFields=0;
            foreach(var assembly in generator.Assemblies) {
                bool helper=assembly.Name.Name=="Il2CppDummyDll";
                if(!helper) {
                    foreach(var type in assembly.MainModule.GetTypes()) {
                        actualTypes++;actualFields+=type.Fields.Count;
                        foreach(var method in type.Methods) {
                            actualMethods++;
                            if(!method.HasBody) {bodyless++;continue;}
                            // Replace the dumper's default-return stubs with an explicit failure.
                            // No copied proprietary method body is executed by this host.
                            var exception=new TypeReference("System","NotSupportedException",method.Module,method.Module.TypeSystem.CoreLibrary);
                            var ctor=new MethodReference(".ctor",referenceTypes.Void,exception) {HasThis=true};ctor.Parameters.Add(new ParameterDefinition(referenceTypes.String));
                            method.Body=new Mono.Cecil.Cil.MethodBody(method) {InitLocals=false};
                            var il=method.Body.GetILProcessor();il.Append(il.Create(OpCodes.Ldstr,"Metallic-RC analysis reference: native method body has not been reconstructed."));il.Append(il.Create(OpCodes.Newobj,ctor));il.Append(il.Create(OpCodes.Throw));stubbed++;
                        }
                    }
                }
                assembly.Write(Path.Combine(dllDir,assembly.MainModule.Name));
            }
            if(actualTypes!=metadata.typeDefs.Length || actualMethods!=metadata.methodDefs.Length || actualFields!=metadata.fieldDefs.Length)throw new InvalidDataException("Reference assembly inventory mismatch");
            if(Hash(binaryStream.ToArray())!=report.RootElement.GetProperty("analysisImageSha256").GetString() || Hash(metadataStream.ToArray())!=report.RootElement.GetProperty("metadataSha256").GetString()) throw new InvalidDataException("Parser changed input bytes");
            var result=new {status="typed_csharp_reference_reconstructed",nativeTypes=native.types.Length,images=metadata.imageDefs.Length,types=actualTypes,methods=actualMethods,fields=actualFields,parameters,nonNullNativeMethodEntries=nonNull,explicitThrowStubs=stubbed,bodylessMethods=bodyless,upstreamCommit="4741d46ba9cd6159c5d853eb9d6fc48b4bfa2b1a",nativeVersion=native.Version,inputBytesUnchanged=true,targetExecuted=false,completeGameRebuild=false,methodBodiesRecovered=0,limits=new[]{"Readable declarations and reference assemblies are not recovered gameplay logic.","Each generated concrete body throws explicitly; abstract/external methods remain bodyless.","Original symbols may be obfuscated. C# syntax in the declaration dump is for inspection, not a buildable Unity project."}};
            File.WriteAllText(Path.Combine(output,"reconstruction-status.json"),JsonSerializer.Serialize(result,JsonOptions)+"\n");Record("complete_reference_only");Console.WriteLine(JsonSerializer.Serialize(result));return 0;
        } catch(Exception ex) {File.WriteAllText(Path.Combine(output,"failure.json"),JsonSerializer.Serialize(new {status="failed",lastStage=status,error=ex.ToString(),targetExecuted=false},JsonOptions));Console.Error.WriteLine(ex);return 1;}
    }
}
