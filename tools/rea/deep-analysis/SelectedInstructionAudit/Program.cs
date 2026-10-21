using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Iced.Intel;

if(args.Length!=3) throw new ArgumentException("SelectedInstructionAudit native-image selections.json NEW-output-folder");
byte[] image=File.ReadAllBytes(args[0]);
string hash=Convert.ToHexStringLower(SHA256.HashData(image));
if(hash!="a183c7ff8262cbbc70fa4869790da4a0bf07e2fc4d2cfaf59612e2838d90b066") throw new InvalidDataException("Unexpected source image");
using var request=JsonDocument.Parse(File.ReadAllText(args[1]));
string output=Path.GetFullPath(args[2]);if(Directory.Exists(output)) throw new IOException("Output exists");Directory.CreateDirectory(output);
int pe=BitConverter.ToInt32(image,0x3c),opt=pe+24,sectionBase=opt+BitConverter.ToUInt16(image,pe+20);
int sectionCount=BitConverter.ToUInt16(image,pe+6);
int RawOffset(int rva,int length) {
    for(int i=0;i<sectionCount;i++) {int p=sectionBase+i*40,va=BitConverter.ToInt32(image,p+12),size=BitConverter.ToInt32(image,p+16),raw=BitConverter.ToInt32(image,p+20);
        if(rva>=va && (long)rva+length<=(long)va+size) return checked(raw+rva-va);
    } throw new InvalidDataException("Unbacked RVA");
}
var formatter=new IntelFormatter();var formatted=new Output();var summaries=new List<object>();int totalInvalid=0,totalInstructions=0;long totalBytes=0;
foreach(var method in request.RootElement.GetProperty("methods").EnumerateArray()) {
    int id=method.GetProperty("methodIndex").GetInt32(),rva=method.GetProperty("rva").GetInt32(),length=method.GetProperty("length").GetInt32();
    if(length<1 || length>100000) throw new InvalidDataException("Range exceeds audit bounds");
    int raw=RawOffset(rva,length);byte[] code=image.AsSpan(raw,length).ToArray();
    var decoder=Iced.Intel.Decoder.Create(64,new ByteArrayCodeReader(code));ulong start=0x180000000+(ulong)rva;decoder.IP=start;
    var rows=new List<object>();int invalid=0,covered=0;
    using(var writer=new StreamWriter(Path.Combine(output,$"m{id}.full-range.asm.txt"),false,new UTF8Encoding(false))) {
        writer.WriteLine("; Offline Iced listing of the entire declared source range, including traps and exception-related bytes. Not a reachability proof.");
        while(decoder.IP<start+(ulong)length) {
            decoder.Decode(out var instruction);
            int offset=checked((int)(instruction.IP-start));
            if(instruction.Length==0 || offset!=covered || instruction.NextIP>start+(ulong)length) throw new InvalidDataException("Decoder range drift");
            if(instruction.IsInvalid) invalid++;
            formatted.Builder.Clear();formatter.Format(instruction,formatted);
            string bytes=Convert.ToHexStringLower(code.AsSpan(offset,instruction.Length));
            rows.Add(new{address=$"0x{instruction.IP:x}",offset,length=instruction.Length,bytes,assembly=formatted.Builder.ToString(),invalid=instruction.IsInvalid,flow=instruction.FlowControl.ToString()});
            writer.WriteLine($"0x{instruction.IP:x}: {bytes,-30} {formatted.Builder}");covered+=instruction.Length;
        }
    }
    if(covered!=length) throw new InvalidDataException("Unaccounted source byte");
    var mask=new bool[length];
    foreach(var range in method.GetProperty("reaRanges").EnumerateArray()) {
        ulong a=Convert.ToUInt64(range.GetProperty("start").GetString()![2..],16),b=Convert.ToUInt64(range.GetProperty("end").GetString()![2..],16);
        for(ulong address=Math.Max(a,start);address<=Math.Min(b,start+(ulong)length-1);address++) mask[(int)(address-start)]=true;
    }
    var gaps=new List<object>();int missing=0,missingNonCC=0;
    for(int i=0;i<length;) {
        if(mask[i]) {i++;continue;}
        int begin=i;while(i<length && !mask[i]) i++;
        byte[] bytes=code.AsSpan(begin,i-begin).ToArray();missing+=bytes.Length;missingNonCC+=bytes.Count(x=>x!=0xcc);
        gaps.Add(new{rva=$"0x{rva+begin:x}",bytes=bytes.Length,allCC=bytes.All(x=>x==0xcc),sha256=Convert.ToHexStringLower(SHA256.HashData(bytes)),first64Hex=Convert.ToHexStringLower(bytes.AsSpan(0,Math.Min(64,bytes.Length)))});
    }
    var result=new{methodIndex=id,rva=$"0x{rva:x}",sourceOffset=raw,sourceBytes=length,sourceRangeBasis=method.GetProperty("rangeBasis").GetString(),sha256=Convert.ToHexStringLower(SHA256.HashData(code)),decodedInstructions=rows.Count,invalidInstructions=invalid,allBytesAccounted=true,reaCoveredBytes=length-missing,reaOmittedBytes=missing,reaOmittedNonCCBytes=missingNonCC,reaGaps=gaps,reachabilityProved=false};
    File.WriteAllText(Path.Combine(output,$"m{id}.instructions.json"),JsonSerializer.Serialize(rows,new JsonSerializerOptions{WriteIndented=true})+"\n");
    summaries.Add(result);totalInvalid+=invalid;totalInstructions+=rows.Count;totalBytes+=length;
}
var summary=new{status=totalInvalid==0?"selected_ranges_decoded_without_invalid_instructions":"invalid_instructions_retained",sourceSha256=hash,selectedFunctions=summaries.Count,sourceBytes=totalBytes,instructions=totalInstructions,invalidInstructions=totalInvalid,allSelectedBytesAccounted=true,gameCodeExecuted=false,methods=summaries,limitations=new[]{"Linear decoding accounts for byte ranges; it does not prove every instruction executes.","Compiler unwind spans may contain traps, padding or exception paths not included by the decompiler.","An omitted 0xCC byte is recorded as such, not automatically declared harmless padding.","Only these selected ranges are audited here, not every method in the game."}};
File.WriteAllText(Path.Combine(output,"summary.json"),JsonSerializer.Serialize(summary,new JsonSerializerOptions{WriteIndented=true})+"\n");
Console.WriteLine(JsonSerializer.Serialize(new{summary.status,summary.selectedFunctions,summary.sourceBytes,summary.instructions,summary.invalidInstructions}));
return totalInvalid==0?0:1;
sealed class Output:FormatterOutput {
    public StringBuilder Builder {get;}=new();
    public override void Write(string text,FormatterTextKind kind)=>Builder.Append(text);
}
