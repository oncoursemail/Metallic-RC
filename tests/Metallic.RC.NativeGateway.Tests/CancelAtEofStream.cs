sealed class CancelAtEofStream(byte[] bytes,CancellationTokenSource cancellation) : MemoryStream(bytes)
{
    public override async ValueTask<int> ReadAsync(Memory<byte> buffer,CancellationToken cancellationToken=default)
    {
        int count=await base.ReadAsync(buffer,cancellationToken);
        if(count==0)cancellation.Cancel();
        return count;
    }
}
