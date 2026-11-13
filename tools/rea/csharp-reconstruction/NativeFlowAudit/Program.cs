using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Iced.Intel;

if(args.Length!=3)throw new ArgumentException("NativeFlowAudit restored-image capture-report NEW-output-directory");
string output=Path.GetFullPath(args[2]);if(Directory.Exists(output))throw new IOException("Output already exists");Directory.CreateDirectory(output);
byte[] image=File.ReadAllBytes(args[0]);string hash=Convert.ToHexStringLower(SHA256.HashData(image));
if(hash!="a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066")throw new InvalidDataException("Expected exact restored native image");
using var capture=JsonDocument.Parse(File.ReadAllText(args[1]));
var sections=capture.RootElement.GetProperty("gameAssemblyDiskLayout").GetProperty("sections").EnumerateArray().Select(s=>new {Name=s.GetProperty("name").GetString(),Rva=s.GetProperty("rva").GetUInt32(),VirtualSize=s.GetProperty("virtualSize").GetUInt32(),Raw=s.GetProperty("rawOffset").GetUInt32(),RawSize=s.GetProperty("rawSize").GetUInt32()}).ToArray();
ulong imageBase=capture.RootElement.GetProperty("gameAssemblyDiskLayout").GetProperty("imageBase").GetUInt64();
int FileOffset(uint rva,uint bytes){foreach(var s in sections)if(rva>=s.Rva && (ulong)rva+bytes<=(ulong)s.Rva+s.RawSize)return checked((int)(s.Raw+rva-s.Rva));throw new InvalidDataException("RVA not fully file backed");}
bool IsCode(uint start,uint end)=>end>start && sections.Any(s=>(s.Name is ".text" or "il2cpp") && start>=s.Rva && (ulong)end<=(ulong)s.Rva+Math.Min(s.VirtualSize,s.RawSize));
uint U32(int p)=>BinaryPrimitives.ReadUInt32LittleEndian(image.AsSpan(p,4));
int pe=checked((int)U32(0x3c));int optional=pe+24;if(BinaryPrimitives.ReadUInt16LittleEndian(image.AsSpan(optional,2))!=0x20b)throw new InvalidDataException("Not PE32+");
uint exceptionRva=U32(optional+112+3*8),exceptionSize=U32(optional+112+3*8+4);if(exceptionSize%12!=0)throw new InvalidDataException("Exception table stride");
int exceptionRaw=FileOffset(exceptionRva,exceptionSize);var ranges=new List<(uint Start,uint End,uint Unwind)>();var unique=new HashSet<ulong>();int duplicates=0;var rejected=new List<object>();
for(int i=0;i<exceptionSize/12;i++){
    int p=exceptionRaw+i*12;uint start=U32(p),end=U32(p+4),unwind=U32(p+8);
    if(!IsCode(start,end)){rejected.Add(new {row=i,start="0x"+start.ToString("x"),end="0x"+end.ToString("x"),reason="not_an_in_bounds_nonempty_code_range"});continue;}
    ulong key=((ulong)start<<32)|end;if(!unique.Add(key)){duplicates++;continue;}ranges.Add((start,end,unwind));
}
ranges.Sort((a,b)=>a.Start!=b.Start?a.Start.CompareTo(b.Start):a.End.CompareTo(b.End));
long unionBytes=0;uint unionStart=0,unionEnd=0;int overlapping=0;
foreach(var range in ranges){if(range.Start<unionEnd)overlapping++;if(range.Start<=unionEnd)unionEnd=Math.Max(unionEnd,range.End);else{unionBytes+=unionEnd-unionStart;unionStart=range.Start;unionEnd=range.End;}}unionBytes+=unionEnd-unionStart;
long instructions=0,invalidInstructions=0,validInstructionBytes=0,invalidBytes=0,totalListedBytes=0,directCalls=0,tailJumps=0,indirectCalls=0,indirectJumps=0,conditionalBranches=0;int rangesWithInvalid=0;var mnemonicCounts=new Dictionary<string,long>();
using(var functions=new StreamWriter(Path.Combine(output,"unwind-function-ranges.csv"),false,new UTF8Encoding(false)))
using(var edges=new StreamWriter(Path.Combine(output,"direct-call-and-tail-edges.csv"),false,new UTF8Encoding(false)))
using(var unresolved=new StreamWriter(Path.Combine(output,"indirect-flows.csv"),false,new UTF8Encoding(false))){
    functions.WriteLine("start_rva,end_rva,unwind_rva,bytes,instructions,invalid_instructions,direct_calls,indirect_calls,conditional_branches,sha256");
    edges.WriteLine("function_rva,instruction_rva,target_address,kind,target_in_captured_code");
    unresolved.WriteLine("function_rva,instruction_rva,flow,operand_kind,memory_base,memory_index,ip_relative_slot_address");
    int processed=0;
    foreach(var range in ranges){
        int bytes=checked((int)(range.End-range.Start)),raw=FileOffset(range.Start,(uint)bytes);totalListedBytes+=bytes;
        var decoder=Iced.Intel.Decoder.Create(64,new ByteArrayCodeReader(image,raw,bytes));decoder.IP=imageBase+range.Start;long localInstructions=0,localInvalid=0,localDirect=0,localIndirect=0,localConditional=0;
        while(decoder.IP<imageBase+range.End){
            ulong before=decoder.IP;decoder.Decode(out var instruction);if(instruction.Length<=0 || decoder.IP<=before || decoder.IP>imageBase+range.End)throw new InvalidDataException("Decoder escaped admitted source range");
            instructions++;localInstructions++;
            if(instruction.IsInvalid){invalidInstructions++;localInvalid++;invalidBytes+=instruction.Length;continue;}
            validInstructionBytes+=instruction.Length;string mnemonic=instruction.Mnemonic.ToString();mnemonicCounts[mnemonic]=mnemonicCounts.GetValueOrDefault(mnemonic)+1;
            if(instruction.FlowControl==FlowControl.ConditionalBranch){conditionalBranches++;localConditional++;}
            bool direct=instruction.Op0Kind is OpKind.NearBranch16 or OpKind.NearBranch32 or OpKind.NearBranch64;
            if(direct && (instruction.FlowControl==FlowControl.Call || (instruction.FlowControl==FlowControl.UnconditionalBranch && (instruction.NearBranchTarget<imageBase+range.Start || instruction.NearBranchTarget>=imageBase+range.End)))){
                ulong target=instruction.NearBranchTarget;bool targetCode=target>=imageBase && target-imageBase<=uint.MaxValue && sections.Any(s=>(s.Name is ".text" or "il2cpp") && target-imageBase>=s.Rva && target-imageBase<(ulong)s.Rva+s.VirtualSize);
                string kind=instruction.FlowControl==FlowControl.Call?"call":"out_of_range_jump_candidate";
                if(kind=="call"){directCalls++;localDirect++;}else tailJumps++;
                edges.WriteLine($"0x{range.Start:x},0x{instruction.IP-imageBase:x},0x{target:x},{kind},{targetCode}");
            }
            if(instruction.FlowControl is FlowControl.IndirectCall or FlowControl.IndirectBranch){
                if(instruction.FlowControl==FlowControl.IndirectCall){indirectCalls++;localIndirect++;}else indirectJumps++;
                string slot=instruction.IsIPRelativeMemoryOperand?"0x"+instruction.IPRelativeMemoryAddress.ToString("x"):"";
                unresolved.WriteLine($"0x{range.Start:x},0x{instruction.IP-imageBase:x},{instruction.FlowControl},{instruction.Op0Kind},{instruction.MemoryBase},{instruction.MemoryIndex},{slot}");
            }
        }
        if(localInvalid>0)rangesWithInvalid++;
        string functionHash=Convert.ToHexStringLower(SHA256.HashData(image.AsSpan(raw,bytes)));
        functions.WriteLine($"0x{range.Start:x},0x{range.End:x},0x{range.Unwind:x},{bytes},{localInstructions},{localInvalid},{localDirect},{localIndirect},{localConditional},{functionHash}");
        processed++;if(processed%100000==0)Console.WriteLine(JsonSerializer.Serialize(new {processed,total=ranges.Count,instructions,invalidInstructions}));
    }
}
if(validInstructionBytes+invalidBytes!=totalListedBytes)throw new InvalidDataException("Decoded byte accounting mismatch");
var files=Directory.GetFiles(output).Select(path=>{using var f=File.OpenRead(path);return new {name=Path.GetFileName(path),bytes=f.Length,sha256=Convert.ToHexStringLower(SHA256.HashData(f))};}).ToArray();
var summary=new{status="unwind_range_static_instruction_audit_completed",sourceSha256=hash,imageBase="0x"+imageBase.ToString("x"),unwindTableEntries=exceptionSize/12,admittedUniqueRanges=ranges.Count,duplicateRanges=duplicates,rejectedRangeCount=rejected.Count,rejectedRanges=rejected.Take(30),overlappingRanges=overlapping,unionRangeBytes=unionBytes,totalListedRangeBytes=totalListedBytes,instructions,invalidInstructions,validInstructionBytes,invalidBytes,rangesWithInvalid,directCalls,outOfRangeJumpCandidates=tailJumps,indirectCalls,indirectJumps,conditionalBranches,opcodeCounts=mnemonicCounts.OrderByDescending(x=>x.Value).ToDictionary(x=>x.Key,x=>x.Value),files,allNativeBytesUnderstood=false,gameCodeExecuted=false,limits=new[]{"PE unwind entries are compiler-provided ranges, not one-to-one original C# methods.","Linear disassembly inside an admitted range does not establish all instructions are reachable or all bytes are code.","Overlapping ranges can repeat listed-byte/instruction counts; unionRangeBytes removes interval overlap.","Direct targets are observed static operands. Indirect targets remain unresolved; an out-of-range jump is not automatically a proven tail call.","Leaf functions without unwind entries and all semantic reconstruction outside admitted ranges remain separate work."},recordedUtc=DateTimeOffset.UtcNow};
File.WriteAllText(Path.Combine(output,"native-flow-audit.json"),JsonSerializer.Serialize(summary,new JsonSerializerOptions{WriteIndented=true})+"\n");
Console.WriteLine(JsonSerializer.Serialize(new{summary.status,summary.unwindTableEntries,summary.admittedUniqueRanges,unionBytes,instructions,invalidInstructions,directCalls,indirectCalls}));
