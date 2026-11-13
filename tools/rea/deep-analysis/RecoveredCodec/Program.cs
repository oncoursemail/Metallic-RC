using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text.Json;
using Metallic.Reconstruction;

if(args.Length!=1)throw new ArgumentException("Provide a new result JSON path");
if(File.Exists(args[0]))throw new IOException("Result exists");
int checks=0,validCases=0,invalidCases=0,partialWriteCases=0;
void Check(bool value,string label){checks++;if(!value)throw new InvalidDataException(label);}
var random=new Random(23191908);
int[] fixedValues={0,1,-1,int.MinValue,int.MaxValue,unchecked((int)0xdeadbeef),0x12345678,256,-256};
for(int test=0;test<10000+fixedValues.Length;test++) {
    int value=test<fixedValues.Length?fixedValues[test]:unchecked((int)random.NextInt64(int.MinValue,(long)int.MaxValue+1));
    byte[] actual=new byte[24];random.NextBytes(actual);byte[] expected=(byte[])actual.Clone();int offset=random.Next(0,21);
    BinaryPrimitives.WriteInt32LittleEndian(expected.AsSpan(offset,4),value);
    ObservedByteCodec.WriteInt32(actual,offset,value);
    Check(actual.AsSpan().SequenceEqual(expected),"Valid write differs from independent platform codec");
    Check(ObservedByteCodec.ReadInt32(actual,offset)==value,"Signed read round-trip");
    Check(ObservedByteCodec.ReadInt32(actual,offset)==BinaryPrimitives.ReadInt32LittleEndian(expected.AsSpan(offset,4)),"Read differs from independent codec");
    validCases++;
}
for(int size=0;size<=12;size++)for(int index=-3;index<=size+2;index++)foreach(int value in fixedValues) {
    if(index>=0 && index<=size-4)continue;
    byte[] before=Enumerable.Repeat((byte)0xa5,size).ToArray();byte[] actual=(byte[])before.Clone();byte[] expected=(byte[])before.Clone();
    int expectedWrites=0;
    for(int byteIndex=0;byteIndex<4;byteIndex++) {
        long pos=(long)index+byteIndex;if(pos<0 || pos>=size)break;
        expected[(int)pos]=unchecked((byte)((uint)value>>(8*byteIndex)));expectedWrites++;
    }
    bool wroteThrow=false;try{ObservedByteCodec.WriteInt32(actual,index,value);}catch(IndexOutOfRangeException){wroteThrow=true;}
    Check(wroteThrow,"Invalid write should use managed bounds failure");
    Check(actual.AsSpan().SequenceEqual(expected),"Partial write order mismatch");
    if(expectedWrites>0)partialWriteCases++;
    bool readThrow=false;try{_ = ObservedByteCodec.ReadInt32(before,index);}catch(IndexOutOfRangeException){readThrow=true;}
    Check(readThrow,"Invalid read should use managed bounds failure");invalidCases++;
}
foreach(int offset in new[]{int.MinValue,int.MaxValue}) {
    byte[] a=new byte[8];bool failed=false;try{ObservedByteCodec.WriteInt32(a,offset,-1);}catch(IndexOutOfRangeException){failed=true;}
    Check(failed && a.All(x=>x==0),"Extreme offset must not mutate the array");
    failed=false;try{_ = ObservedByteCodec.ReadInt32(a,offset);}catch(IndexOutOfRangeException){failed=true;}
    Check(failed,"Extreme read index");
}
bool nullFailed=false;try{ObservedByteCodec.WriteInt32(null!,0,0);}catch(NullReferenceException){nullFailed=true;}
Check(nullFailed,"Null destination follows managed null dereference semantics");
var report=new {status="synthetic_behavior_checks_passed",validCases,invalidCases,partialWriteCases,checks,originalNativeCodeExecuted=false,originalGameAssembliesExecuted=false,methods=new[]{387023,387055},helperMethods=new[]{380446,380444},evidence=new[]{"ev_7ccae70f7d3aabb662691b87958ec7ed6b1abcba95b60fc6f56ccb843ed44001","ev_ce2b49c27a20da1249dfbf6a7edee10d56571dbb8fb34874d4acccdcaf9a7aa1","ev_fe99c9ac15b89999937757e71439c4fd5aa0799a9665e393d00a0d1c3eb10863","ev_94f8df47a1802d364bad8863bec8bc5214fbadc3c1862c98dbf7eb13d86f76b3"},scope="Two bounded C# codec reconstructions. These tests run new managed code, not the original native implementation.",limitations=new[]{"Native exception types, hardware faults and concurrency have not been tested against a running original client.","No full bytecode instruction set, circuit compiler or game rebuild is established.","These two reconstructions are separate from the earlier 27302 leaf-body count."}};
File.WriteAllText(args[0],JsonSerializer.Serialize(report,new JsonSerializerOptions{WriteIndented=true})+"\n");
Console.WriteLine(JsonSerializer.Serialize(report));return 0;
