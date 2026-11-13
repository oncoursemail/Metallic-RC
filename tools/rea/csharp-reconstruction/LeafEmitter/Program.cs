using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Mono.Cecil;
using Mono.Cecil.Cil;
using Iced.Intel;
using ICSharpCode.Decompiler;
using ICSharpCode.Decompiler.CSharp;
using ICSharpCode.Decompiler.TypeSystem;
using COp=Mono.Cecil.Cil.OpCodes;
using ROp=System.Reflection.Emit.OpCodes;
using FlowControl=Iced.Intel.FlowControl;

internal static class Program
{
    static readonly JsonSerializerOptions Pretty=new(){WriteIndented=true};
    static string Hash(byte[] b)=>Convert.ToHexStringLower(SHA256.HashData(b));
    static uint OriginalToken(ICustomAttributeProvider member) {
        var a=member.CustomAttributes.SingleOrDefault(x=>x.AttributeType.FullName=="Il2CppDummyDll.TokenAttribute");
        if(a==null)return 0;
        string s=(string)a.Fields.Single(x=>x.Name=="Token").Argument.Value;
        return Convert.ToUInt32(s[2..],16);
    }
    static void CheckInstructions(JsonElement proof, byte[] source) {
        byte[] bytes=Convert.FromHexString(proof.GetProperty("nativeBytes").GetString());
        int offset=proof.GetProperty("sourceOffset").GetInt32();
        if(!source.AsSpan(offset,bytes.Length).SequenceEqual(bytes) || Hash(bytes)!=proof.GetProperty("nativeBytesSha256").GetString())throw new InvalidDataException("Source instruction mismatch");
        ulong ip=0x180000000+Convert.ToUInt64(proof.GetProperty("rva").GetString()[2..],16);
        var decoder=Iced.Intel.Decoder.Create(64,new ByteArrayCodeReader(bytes));decoder.IP=ip;
        var instructions=new List<Iced.Intel.Instruction>();
        while(decoder.IP<ip+(ulong)bytes.Length) {decoder.Decode(out var i);if(i.IsInvalid || i.Length==0 || i.NextIP>ip+(ulong)bytes.Length)throw new InvalidDataException("Iced rejects instruction coverage");instructions.Add(i);}
        string op=proof.GetProperty("translation").GetProperty("operation").GetString();
        if(instructions[^1].Mnemonic!=Mnemonic.Ret || instructions[^1].FlowControl!=FlowControl.Return)throw new InvalidDataException("Leaf does not end in RET");
        if(op=="return_void") {if(instructions.Count!=1)throw new InvalidDataException("Unexpected void body");return;}
        if(instructions.Count!=2 || instructions[0].FlowControl!=FlowControl.Next)throw new InvalidDataException("Non-leaf control flow");
        var first=instructions[0];
        if(op is "get" or "set") {
            if(first.MemoryBase!=Register.RCX || first.MemoryIndex!=Register.None || first.MemoryDisplacement64!=(ulong)proof.GetProperty("field").GetProperty("offset").GetInt32())throw new InvalidDataException("Wrong field addressing");
            if(op=="get" && (first.Op1Kind!=OpKind.Memory || first.Op0Kind!=OpKind.Register || first.Op0Register is not(Register.RAX or Register.EAX or Register.AL or Register.XMM0)))throw new InvalidDataException("Getter register mismatch");
            if(op=="set" && (first.Op0Kind!=OpKind.Memory || first.Op1Kind!=OpKind.Register || first.Op1Register is not(Register.RDX or Register.EDX or Register.DX or Register.DL or Register.XMM1)))throw new InvalidDataException("Setter register mismatch");
        }
    }
    static int SyntheticSemanticChecks() {
        // Execute only new, synthetic managed field operations, never game code.
        int checks=0;
        var cases=new (string Name,object[] Values)[]{
            (nameof(Probe.Int),new object[]{int.MinValue,-1,0,1,int.MaxValue}),
            (nameof(Probe.Unsigned),new object[]{0u,1u,uint.MaxValue}),
            (nameof(Probe.Long),new object[]{long.MinValue,-1L,0L,long.MaxValue}),
            (nameof(Probe.Byte),new object[]{(byte)0,(byte)255}),
            (nameof(Probe.SignedByte),new object[]{sbyte.MinValue,(sbyte)0,sbyte.MaxValue}),
            (nameof(Probe.Short),new object[]{short.MinValue,(short)0,short.MaxValue}),
            (nameof(Probe.Char),new object[]{'\0','A','\uffff'}),
            (nameof(Probe.Bool),new object[]{false,true}),
            (nameof(Probe.Float),new object[]{0f,-0f,float.NaN,float.NegativeInfinity,float.MaxValue}),
            (nameof(Probe.Double),new object[]{0d,-0d,double.NaN,double.PositiveInfinity,double.MinValue}),
            (nameof(Probe.Text),new object[]{null,"","synthetic test"})};
        foreach(var c in cases) {
            var field=typeof(Probe).GetField(c.Name);var get=new System.Reflection.Emit.DynamicMethod("get_"+c.Name,field.FieldType,new[]{typeof(Probe)},typeof(Program).Module,true);
            var il=get.GetILGenerator();il.Emit(ROp.Ldarg_0);il.Emit(ROp.Ldfld,field);il.Emit(ROp.Ret);
            var set=new System.Reflection.Emit.DynamicMethod("set_"+c.Name,typeof(void),new[]{typeof(Probe),field.FieldType},typeof(Program).Module,true);
            il=set.GetILGenerator();il.Emit(ROp.Ldarg_0);il.Emit(ROp.Ldarg_1);il.Emit(ROp.Stfld,field);il.Emit(ROp.Ret);
            foreach(var value in c.Values) {var obj=new Probe();set.Invoke(null,new[]{obj,value});object got=get.Invoke(null,new object[]{obj});bool equal=value switch {float f=>BitConverter.SingleToInt32Bits(f)==BitConverter.SingleToInt32Bits((float)got),double d=>BitConverter.DoubleToInt64Bits(d)==BitConverter.DoubleToInt64Bits((double)got),_=>Equals(value,got)};if(!equal)throw new InvalidDataException("Synthetic field operation mismatch");checks+=2;}
        }
        return checks;
    }
    public class Probe {public int Int=0;public uint Unsigned=0;public long Long=0;public byte Byte=0;public sbyte SignedByte=0;public short Short=0;public char Char='\0';public bool Bool=false;public float Float=0;public double Double=0;public string Text=null;}
    public static int Main(string[] args) {
        if(args.Length!=4)throw new ArgumentException("LeafEmitter reference-directory proof.jsonl restored-native-image NEW-output-directory");
        string input=Path.GetFullPath(args[0]),output=Path.GetFullPath(args[3]);if(Directory.Exists(output))throw new IOException("Output already exists");Directory.CreateDirectory(output);
        byte[] native=File.ReadAllBytes(args[2]);if(Hash(native)!="a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066")throw new InvalidDataException("Wrong native source");
        var proofs=new Dictionary<(string,uint),JsonElement>();
        foreach(string line in File.ReadLines(args[1])) {using var j=JsonDocument.Parse(line);var e=j.RootElement.Clone();CheckInstructions(e,native);proofs.Add((e.GetProperty("assembly").GetString(),e.GetProperty("token").GetUInt32()),e);}
        int syntheticTests=SyntheticSemanticChecks();
        var resolver=new DefaultAssemblyResolver();resolver.AddSearchDirectory(input);
        var used=new HashSet<(string,uint)>();var emitted=new List<object>();var skipped=new List<object>();var selected=new List<(string File,string FullName,int Count)>();var inputHashes=new Dictionary<string,string>();
        long methodCount=0,typeCount=0,fieldCount=0,remainingThrows=0,bodyless=0;
        string dllDir=Path.Combine(output,"PartialAssemblies.NOT_A_GAME");Directory.CreateDirectory(dllDir);
        foreach(string path in Directory.GetFiles(input)) {
            inputHashes[path]=Hash(File.ReadAllBytes(path));
            using var assembly=AssemblyDefinition.ReadAssembly(path,new ReaderParameters {AssemblyResolver=resolver,ReadingMode=ReadingMode.Immediate});
            string module=assembly.MainModule.Name;bool helper=assembly.Name.Name=="Il2CppDummyDll";
            if(!helper)foreach(var type in assembly.MainModule.GetTypes()) {
                typeCount++;fieldCount+=type.Fields.Count;int changed=0;
                foreach(var method in type.Methods) {
                    methodCount++;uint token=OriginalToken(method);
                    if(!proofs.TryGetValue((module,token),out var proof)) {if(method.HasBody)remainingThrows++;else bodyless++;continue;}
                    if(!method.HasBody || method.IsPInvokeImpl) {skipped.Add(new {module,token,reason="abstract_external_or_runtime_body"});bodyless++;continue;}
                    if(method.Name!=proof.GetProperty("method").GetString())throw new InvalidDataException("Method name/token identity mismatch");
                    string operation=proof.GetProperty("translation").GetProperty("operation").GetString();FieldDefinition field=null;
                    if(operation is "get" or "set") {
                        uint ft=proof.GetProperty("field").GetProperty("token").GetUInt32();field=type.Fields.SingleOrDefault(f=>OriginalToken(f)==ft);
                        if(field==null || field.IsStatic || field.FieldType is RequiredModifierType or OptionalModifierType) {skipped.Add(new {module,token,reason="field_identity_or_modifiers"});remainingThrows++;continue;}
                        if(operation=="set" && (field.IsInitOnly || field.IsLiteral)) {skipped.Add(new {module,token,reason="readonly_or_literal_field"});remainingThrows++;continue;}
                        string sig=operation=="get"?method.ReturnType.FullName:method.Parameters.Single().ParameterType.FullName;
                        if(sig!=field.FieldType.FullName)throw new InvalidDataException("Cecil field/signature mismatch");
                    }
                    method.Body=new Mono.Cecil.Cil.MethodBody(method) {InitLocals=false};var il=method.Body.GetILProcessor();
                    if(operation=="get") {il.Emit(COp.Ldarg_0);il.Emit(COp.Ldfld,field);}
                    else if(operation=="set") {il.Emit(COp.Ldarg_0);il.Emit(COp.Ldarg_1);il.Emit(COp.Stfld,field);}
                    else if(operation=="constant") {
                        var tr=proof.GetProperty("translation");
                        if(tr.GetProperty("managedConstant").GetString()=="null")il.Emit(COp.Ldnull);
                        else {uint v=tr.GetProperty("value").GetUInt32();int n=method.ReturnType.MetadataType switch {MetadataType.SByte=>unchecked((sbyte)v),MetadataType.Byte=>unchecked((byte)v),MetadataType.Int16=>unchecked((short)v),MetadataType.UInt16 or MetadataType.Char=>unchecked((ushort)v),_=>unchecked((int)v)};il.Emit(COp.Ldc_I4,n);}
                    } else if(operation!="return_void")throw new InvalidDataException("Unknown translation");
                    il.Emit(COp.Ret);used.Add((module,token));changed++;
                    emitted.Add(new {assembly=module,token,methodIndex=proof.GetProperty("methodIndex").GetInt32(),declaringType=type.FullName,method=method.Name,operation,rva=proof.GetProperty("rva").GetString(),nativeBytesSha256=proof.GetProperty("nativeBytesSha256").GetString(),il=method.Body.Instructions.Select(x=>x.OpCode.Name).ToArray()});
                }
                if(changed>0 && new[]{"NameServerResponse","PaintballManager","PlayerHand","PaintballGun","RecRoomGameRoot","RecNetConfiguration","RoomSettings"}.Contains(type.Name))selected.Add((module,type.FullName,changed));
            }
            assembly.Write(Path.Combine(dllDir,module));
        }
        if(methodCount!=409503 || typeCount!=63648 || fieldCount!=252928)throw new InvalidDataException("Full inventory changed");
        foreach(var kv in inputHashes)if(Hash(File.ReadAllBytes(kv.Key))!=kv.Value)throw new InvalidDataException("Input reference modified");
        // Read back every output method to check emitted opcode shapes and explicit unknowns.
        long readbackRestored=0,readbackThrows=0,readbackBodyless=0;
        resolver.AddSearchDirectory(dllDir);
        foreach(string path in Directory.GetFiles(dllDir)) {
            using var a=AssemblyDefinition.ReadAssembly(path,new ReaderParameters{AssemblyResolver=resolver});if(a.Name.Name=="Il2CppDummyDll")continue;
            foreach(var t in a.MainModule.GetTypes())foreach(var method in t.Methods) {
                if(used.Contains((a.MainModule.Name,OriginalToken(method)))) {if(!method.HasBody || method.Body.Instructions[^1].OpCode!=COp.Ret)throw new InvalidDataException("Recovered IL readback mismatch");readbackRestored++;}
                else if(method.HasBody) {var instructions=method.Body.Instructions;if(instructions.Count!=3 || instructions[0].OpCode!=COp.Ldstr || instructions[1].OpCode!=COp.Newobj || instructions[2].OpCode!=COp.Throw)throw new InvalidDataException("Unmarked unrecovered behavior");readbackThrows++;}
                else readbackBodyless++;
            }
        }
        if(readbackRestored!=used.Count || readbackThrows!=remainingThrows || readbackBodyless!=bodyless)throw new InvalidDataException("Readback counts disagree");
        string csharpDir=Path.Combine(output,"SelectedCSharp");Directory.CreateDirectory(csharpDir);var decompiled=new List<object>();
        foreach(var item in selected.Take(16)) {
            try {var settings=new DecompilerSettings {ThrowOnAssemblyResolveErrors=false};var decompiler=new CSharpDecompiler(Path.Combine(dllDir,item.File),settings);string source=decompiler.DecompileTypeAsString(new FullTypeName(item.FullName.Replace('/','+')));string filename=item.File.Replace('.','_')+"__"+string.Concat(item.FullName.Select(c=>char.IsAsciiLetterOrDigit(c)?c:'_'))+".cs";File.WriteAllText(Path.Combine(csharpDir,filename),"// PARTIAL RECONSTRUCTION: only manifest-listed leaf bodies recovered.\n// All other concrete bodies throw explicitly. Not a runnable replacement game.\n"+source);decompiled.Add(new{assembly=item.File,type=item.FullName,recoveredLeafMethods=item.Count,file=filename,status="written"});}
            catch(Exception ex){decompiled.Add(new{assembly=item.File,type=item.FullName,status="decompiler_error",error=ex.Message});}
        }
        using(var writer=new StreamWriter(Path.Combine(output,"emitted-leaf-methods.jsonl")))foreach(var item in emitted)writer.WriteLine(JsonSerializer.Serialize(item));
        var result=new {status="partial_managed_leaf_reconstruction",allMethods=methodCount,allTypes=typeCount,allFields=fieldCount,proofsIndependentlyDecodedByIced=proofs.Count,emittedLeafBodies=used.Count,unrecoveredConcreteThrowBodies=remainingThrows,bodylessMethods=bodyless,syntheticManagedChecks=syntheticTests,skipped,selectedCSharp=decompiled,allOutputMethodsReadBack=true,inputFilesUnchanged=true,nativeCodeExecuted=false,gameAssembliesExecuted=false,fullGameRebuild=false,limits=new[]{"Only exact typed branch/call-free leaf operations are reconstructed, not full gameplay.","Synthetic checks exercise emitted operation families; they do not prove every recovered method under game concurrency, exceptional inputs or other platforms.","C# output is generated from reconstructed managed IL. It is not the original developer source."}};
        File.WriteAllText(Path.Combine(output,"leaf-emission-result.json"),JsonSerializer.Serialize(result,Pretty)+"\n");Console.WriteLine(JsonSerializer.Serialize(result));return 0;
    }
}
